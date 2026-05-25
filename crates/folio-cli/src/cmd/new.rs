//! `folio new` — interactive wizard for creating a new invoice.
//!
//! Prompts for the client (with autocomplete from `clients/`), invoice date,
//! and line items. Auto-generates the next sequential ID based on `id_format`
//! in `folio.toml`. Writes the invoice TOML to `invoices/{year}/{id}.toml`.
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
    store::{FilesystemStore, InvoiceStore},
    types::{Client, Invoice, InvoiceFilter, LineItem},
};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Create a new invoice interactively.
///
/// Prompts for client, date, and line items, then writes the invoice TOML to
/// `invoices/{year}/{id}.toml`. The next sequential ID is auto-generated from
/// the `id_format` in `folio.toml`.
///
/// If no clients exist yet you will be offered the option to create one now.
///
/// Examples:
///
/// ```sh
/// folio new
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
}

pub async fn run(args: NewArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

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
            create_client_interactive(&store).await?
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
        Input::new()
            .with_prompt("Invoice date")
            .default(today)
            .interact_text()?
    };

    let date = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|_| eyre::eyre!("invalid date {:?} — expected YYYY-MM-DD", date_str))?;

    let year = date.format("%Y").to_string();

    // ── ID generation ─────────────────────────────────────────────────────
    let id_format = config
        .defaults
        .id_format
        .as_deref()
        .unwrap_or("INV-{year}-{seq:03}");

    let next_seq = next_invoice_seq(&store, date.year()).await?;
    let id = format_id(id_format, date.year(), next_seq);

    println!("Creating invoice {}", id);

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

    Ok(())
}

/// Interactively prompt for client details and write `clients/{slug}.toml`.
///
/// Returns the slug of the newly created client.
async fn create_client_interactive(store: &FilesystemStore) -> Result<String> {
    println!();
    let name: String = Input::new().with_prompt("Client name").interact_text()?;

    // Derive a default slug from the name (lowercase, spaces → hyphens)
    let default_slug = name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");

    let slug: String = Input::new()
        .with_prompt("Client slug (used as filename)")
        .default(default_slug)
        .interact_text()?;

    let contact: String = Input::new()
        .with_prompt("Contact name")
        .allow_empty(true)
        .interact_text()?;

    let email: String = Input::new().with_prompt("Billing email").interact_text()?;

    let addr1: String = Input::new()
        .with_prompt("Address line 1")
        .allow_empty(true)
        .interact_text()?;
    let addr2: String = Input::new()
        .with_prompt("Address line 2")
        .allow_empty(true)
        .interact_text()?;

    let mut address = Vec::new();
    if !addr1.is_empty() {
        address.push(addr1);
    }
    if !addr2.is_empty() {
        address.push(addr2);
    }

    let client = Client {
        name,
        contact: if contact.is_empty() {
            None
        } else {
            Some(contact)
        },
        email,
        address,
        currency: None,
        due_days: None,
        template: None,
        email_opts: None,
        notes: None,
        slug: slug.clone(),
    };

    store.save_client(&client).await?;
    println!(
        "✓ Created {}/{}.toml\n",
        store.clients_dir().display(),
        slug
    );

    Ok(slug)
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
