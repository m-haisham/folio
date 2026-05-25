//! `folio new` — interactive wizard for creating a new invoice or quote.
//!
//! Prompts for the document type (invoice or quote), client (with autocomplete
//! from `clients/`), date, and line items. Auto-generates the next sequential
//! ID based on `id_format` in `folio.toml`. Writes the TOML to
//! `invoices/{year}/{id}.toml` or `quotes/{year}/{id}.toml`.
//! Does **not** build or send — those are separate steps.
//!
//! If no clients exist yet, the wizard offers to create one inline so the user
//! never hits a dead end on their first run.

use chrono::{Datelike, Local};
use clap::Args;
use dialoguer::{Confirm, Input, Select};
use eyre::Result;
use folio_core::{
    config::{find_root, load_config},
    store::{ClientStore, FilesystemStore, InvoiceStore, QuoteStore},
    types::{Invoice, InvoiceFilter, LineItem, Quote, QuoteFilter},
};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Create a new invoice or quote interactively.
///
/// Prompts for document type (unless `--type` is given), client, date, and
/// line items, then writes the TOML to the appropriate directory. The next
/// sequential ID is auto-generated from `id_format` in `folio.toml`.
///
/// If no clients exist yet you will be offered the option to create one now.
///
/// Examples:
///
/// ```sh
/// folio new
/// folio new --type invoice
/// folio new --type quote
/// folio new --client acme
/// folio new --client acme --date 2026-05-01
/// ```
#[derive(Args)]
pub struct NewArgs {
    /// Client slug to pre-fill (must match a file in `clients/`).
    #[arg(long)]
    pub client: Option<String>,

    /// Invoice date in `YYYY-MM-DD` format (defaults to today).
    #[arg(long)]
    pub date: Option<String>,

    /// Document type to create: `invoice` or `quote`.
    #[arg(long = "type")]
    pub doc_type: Option<String>,
}

pub async fn run(args: NewArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    // ── Document type ─────────────────────────────────────────────────────
    let doc_type = if let Some(ref t) = args.doc_type {
        t.clone()
    } else {
        let choices = &["invoice", "quote"];
        let idx = Select::new()
            .with_prompt("Document type")
            .items(choices)
            .interact()?;
        choices[idx].to_string()
    };

    // ── Client selection ──────────────────────────────────────────────────
    let client_slug = if let Some(c) = args.client {
        // Validate the slug exists; give an actionable hint if not.
        store.get_client(&c).await.map_err(|e| {
            eyre::eyre!(e).wrap_err(format!(
                "expected file at {}/{}.toml",
                store.clients_dir().display(),
                c
            ))
        })?;
        c
    } else {
        let clients = store.list_clients().await?;
        if clients.is_empty() {
            // Offer to create a client inline rather than bailing.
            eprintln!("No clients found in {}.", store.clients_dir().display());
            let create = Confirm::new()
                .with_prompt("Create a client now?")
                .default(true)
                .interact()?;
            if !create {
                eyre::bail!(
                    "no clients found — add a TOML file to {}/ first",
                    store.clients_dir().display()
                );
            }
            crate::cmd::client::create_client_interactive(&store).await?
        } else {
            let names: Vec<&str> = clients.iter().map(|c| c.slug.as_str()).collect();
            let idx = Select::new()
                .with_prompt("Client")
                .items(&names)
                .interact()?;
            clients[idx].slug.clone()
        }
    };

    // ── Date ─────────────────────────────────────────────────────────────
    let date_str = if let Some(d) = args.date {
        d
    } else {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let prompt = if doc_type == "quote" {
            "Quote date"
        } else {
            "Invoice date"
        };
        Input::new()
            .with_prompt(prompt)
            .default(today)
            .interact_text()?
    };

    let date = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|_| eyre::eyre!("invalid date {:?} — expected YYYY-MM-DD", date_str))?;

    let year = date.format("%Y").to_string();

    // ── ID generation ─────────────────────────────────────────────────────
    let (id, is_quote) = if doc_type == "quote" {
        let id_format = config
            .quote
            .as_ref()
            .and_then(|q| q.id_format.as_deref())
            .or(config.defaults.quote_id_format.as_deref())
            .unwrap_or("QUO-{year}-{seq:03}");
        let next_seq = next_quote_seq(&store, date.year()).await?;
        (format_id(id_format, date.year(), next_seq), true)
    } else {
        let id_format = config
            .invoice
            .as_ref()
            .and_then(|i| i.id_format.as_deref())
            .or(config.defaults.id_format.as_deref())
            .unwrap_or("INV-{year}-{seq:03}");
        let next_seq = next_invoice_seq(&store, date.year()).await?;
        (format_id(id_format, date.year(), next_seq), false)
    };

    if is_quote {
        println!("Creating quote {}", id);
    } else {
        println!("Creating invoice {}", id);
    }

    // ── Line items — loop until the user submits an empty description ─────
    let mut items = Vec::new();
    loop {
        let desc: String = Input::new()
            .with_prompt("Item description (empty to finish)")
            .allow_empty(true)
            .interact_text()?;
        if desc.is_empty() {
            break;
        }

        let qty_str: String = Input::new()
            .with_prompt("Quantity")
            .default("1.0".to_string())
            .interact_text()?;
        let quantity: Decimal = Decimal::from_str(&qty_str)
            .map_err(|_| eyre::eyre!("invalid quantity {:?}", qty_str))?;

        let unit: String = Input::new()
            .with_prompt("Unit (e.g. hours, project)")
            .default("hours".to_string())
            .interact_text()?;

        let rate_str: String = Input::new().with_prompt("Rate").interact_text()?;
        let rate: Decimal =
            Decimal::from_str(&rate_str).map_err(|_| eyre::eyre!("invalid rate {:?}", rate_str))?;

        items.push(LineItem {
            description: desc,
            quantity,
            unit: Some(unit),
            rate,
        });
    }

    if items.is_empty() {
        eyre::bail!("no items added — at least one line item is required");
    }

    // ── Save ──────────────────────────────────────────────────────────────
    if is_quote {
        let quote = Quote {
            id: id.clone(),
            client: client_slug,
            date,
            expires: None,
            currency: None,
            template: None,
            primary_color: None,
            tax_rate: None,
            notes: None,
            items,
            sent: None,
            accepted: None,
            declined: None,
        };
        store.save_quote(&quote).await?;
        println!("✓ Created quotes/{}/{}.toml", year, id);
    } else {
        let invoice = Invoice {
            id: id.clone(),
            client: client_slug,
            date,
            due: None,
            currency: None,
            template: None,
            primary_color: None,
            tax_rate: None,
            notes: None,
            items,
            sent: None,
            paid: None,
            voided: None,
        };
        store.save(&invoice).await?;
        println!("✓ Created invoices/{}/{}.toml", year, id);
    }

    Ok(())
}

/// Find the next sequential invoice number for the given year.
///
/// Scans existing invoices for the year, extracts the trailing numeric segment
/// from each ID, and returns `max + 1`.
pub async fn next_invoice_seq(store: &FilesystemStore, year: i32) -> Result<u32> {
    let filter = InvoiceFilter {
        year: Some(year),
        ..Default::default()
    };
    let invoices = store.list(&filter).await?;

    let max_seq = invoices
        .iter()
        .filter_map(|inv| inv.id.split('-').last()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);

    Ok(max_seq + 1)
}

/// Find the next sequential quote number for the given year.
///
/// Scans existing quotes for the year, extracts the trailing numeric segment
/// from each ID, and returns `max + 1`.
async fn next_quote_seq(store: &FilesystemStore, year: i32) -> Result<u32> {
    let filter = QuoteFilter {
        year: Some(year),
        ..Default::default()
    };
    let quotes = store.list_quotes(&filter).await?;

    let max_seq = quotes
        .iter()
        .filter_map(|q| q.id.split('-').last()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);

    Ok(max_seq + 1)
}

/// Format an invoice ID from a format string, replacing `{year}` and `{seq:NNN}`.
///
/// The `{seq:NNN}` token is zero-padded to the width implied by the number of
/// characters in the spec (e.g. `{seq:03}` pads to 3 digits).
pub fn format_id(format: &str, year: i32, seq: u32) -> String {
    let mut result = format.to_string();
    result = result.replace("{year}", &year.to_string());

    // Handle {seq:NNN} with zero-padding
    if let Some(start) = result.find("{seq:") {
        if let Some(end_rel) = result[start..].find('}') {
            let spec = &result[start + 5..start + end_rel];
            let width: usize = spec.len().max(1);
            let padded = format!("{:0>width$}", seq, width = width);
            result = format!(
                "{}{}{}",
                &result[..start],
                padded,
                &result[start + end_rel + 1..]
            );
        }
    } else {
        result = result.replace("{seq}", &seq.to_string());
    }

    result
}
