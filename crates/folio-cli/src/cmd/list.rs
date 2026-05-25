//! `folio list` — tabular view of invoices and/or quotes.
//!
//! Prints documents as colour-coded table rows showing ID, client, dates,
//! total, derived status, and whether the PDF is fresh, stale, or not yet
//! built. Supports filtering by year, client, status, and document type.

use clap::Args;
use colored::Colorize;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    index::{BuildIndex, PdfState, check_pdf_state, compute_source_hash},
    quote_compute::compute_quote,
    store::{FilesystemStore, InvoiceStore, QuoteStore},
    templates::get_template_html,
    types::{InvoiceFilter, InvoiceStatus, QuoteFilter, QuoteStatus},
};
use std::fs;

/// List invoices and/or quotes in a colour-coded table with status and PDF freshness.
///
/// PDF state indicators:
/// - `✓` — PDF is up to date
/// - `~` — PDF exists but source has changed
/// - `—` — PDF has never been built
///
/// Status colour coding for invoices: overdue=red, sent=yellow, paid=green, draft=dim.
/// Status colour coding for quotes: expired=red, sent=yellow, accepted=green, draft/declined=dim.
///
/// The special status filter `unpaid` matches invoice statuses `draft`, `sent`, and `overdue`.
///
/// Examples:
///
/// ```sh
/// folio list
/// folio list --type invoice
/// folio list --type quote
/// folio list --type all
/// folio list --year 2026
/// folio list --client acme
/// folio list --status unpaid
/// folio list --status overdue
/// folio list --status paid
/// ```
#[derive(Args)]
pub struct ListArgs {
    /// Show only documents for the given year.
    #[arg(long)]
    pub year: Option<i32>,

    /// Show only documents for the given client slug.
    #[arg(long)]
    pub client: Option<String>,

    /// Show only documents with the given status.
    ///
    /// Accepts any derived status (`draft`, `sent`, `overdue`, `paid`,
    /// `partially_paid`, `voided`) or the special value `unpaid` (which
    /// matches invoice statuses `draft`, `sent`, and `overdue`).
    #[arg(long)]
    pub status: Option<String>,

    /// Document type to list: `invoice`, `quote`, or `all` (default).
    #[arg(long = "type", default_value = "all")]
    pub doc_type: String,
}

pub async fn run(args: ListArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());
    let index = BuildIndex::load(&root)?;

    match args.doc_type.as_str() {
        "invoice" => {
            // Invoice-only view (existing behaviour)
            let filter = InvoiceFilter {
                year: args.year,
                client: args.client.clone(),
                ..Default::default()
            };
            let invoices = store.list(&filter).await?;

            println!(
                "{:<16} {:<12} {:<12} {:<12} {:<14} {:<16} {}",
                "ID", "CLIENT", "DATE", "DUE", "TOTAL", "STATUS", "PDF"
            );
            println!("{}", "-".repeat(95));

            for invoice in &invoices {
                let client = store.get_client(&invoice.client).await?;
                let computed = compute_invoice(invoice, &client, &config);

                // Apply status filter if specified
                if let Some(ref status_filter) = args.status {
                    let matches = match status_filter.as_str() {
                        "unpaid" => matches!(
                            computed.status,
                            InvoiceStatus::Draft | InvoiceStatus::Sent | InvoiceStatus::Overdue
                        ),
                        s => computed.status.to_string() == s,
                    };
                    if !matches {
                        continue;
                    }
                }

                let template_html = get_template_html(&computed.template, &store.templates_dir())
                    .unwrap_or_default();
                let invoice_toml = toml::to_string(invoice).unwrap_or_default();
                let client_path = store.clients_dir().join(format!("{}.toml", client.slug));
                let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
                let me_toml = toml::to_string(&config.me).unwrap_or_default();
                let hash =
                    compute_source_hash(&invoice_toml, &client_toml, &template_html, &me_toml);

                let pdf_indicator = match check_pdf_state(&index, &invoice.id, &hash) {
                    PdfState::Fresh => "✓",
                    PdfState::Stale => "~",
                    PdfState::NeverBuilt => "—",
                };

                let total_str = format!("{} {:.2}", computed.currency, computed.total);
                let status_str = computed.status.to_string();

                let row = format!(
                    "{:<16} {:<12} {:<12} {:<12} {:<14} {:<16} {}",
                    invoice.id,
                    client.slug,
                    computed.date.format("%Y-%m-%d"),
                    computed.due.format("%Y-%m-%d"),
                    total_str,
                    status_str,
                    pdf_indicator,
                );

                let colored_row = match computed.status {
                    InvoiceStatus::Overdue => row.red().to_string(),
                    InvoiceStatus::Sent => row.yellow().to_string(),
                    InvoiceStatus::Paid => row.green().to_string(),
                    InvoiceStatus::Draft => row.dimmed().to_string(),
                    _ => row,
                };

                println!("{}", colored_row);
            }
        }

        "quote" => {
            // Quote-only view
            let filter = QuoteFilter {
                year: args.year,
                client: args.client.clone(),
                ..Default::default()
            };
            let quotes = store.list_quotes(&filter).await?;

            println!(
                "{:<16} {:<12} {:<12} {:<12} {:<14} {:<16} {}",
                "ID", "CLIENT", "DATE", "EXPIRES", "TOTAL", "STATUS", "PDF"
            );
            println!("{}", "-".repeat(95));

            for quote in &quotes {
                let client = store.get_client(&quote.client).await?;
                let computed = compute_quote(quote, &client, &config);

                if let Some(ref sf) = args.status {
                    if computed.status.to_string() != *sf {
                        continue;
                    }
                }

                let template_html = get_template_html(&computed.template, &store.templates_dir())
                    .unwrap_or_default();
                let quote_toml = toml::to_string(quote).unwrap_or_default();
                let client_path = store.clients_dir().join(format!("{}.toml", client.slug));
                let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
                let me_toml = toml::to_string(&config.me).unwrap_or_default();
                let hash = compute_source_hash(&quote_toml, &client_toml, &template_html, &me_toml);

                let pdf_indicator = match check_pdf_state(&index, &quote.id, &hash) {
                    PdfState::Fresh => "✓",
                    PdfState::Stale => "~",
                    PdfState::NeverBuilt => "—",
                };

                let total_str = format!("{} {:.2}", computed.currency, computed.total);
                let row = format!(
                    "{:<16} {:<12} {:<12} {:<12} {:<14} {:<16} {}",
                    quote.id,
                    client.slug,
                    computed.date.format("%Y-%m-%d"),
                    computed.expires.format("%Y-%m-%d"),
                    total_str,
                    computed.status.to_string(),
                    pdf_indicator,
                );

                let colored_row = match computed.status {
                    QuoteStatus::Expired => row.red().to_string(),
                    QuoteStatus::Sent => row.yellow().to_string(),
                    QuoteStatus::Accepted => row.green().to_string(),
                    QuoteStatus::Draft | QuoteStatus::Declined => row.dimmed().to_string(),
                };
                println!("{}", colored_row);
            }
        }

        _ => {
            // All mode — show invoices and quotes together with a TYPE column
            let inv_filter = InvoiceFilter {
                year: args.year,
                client: args.client.clone(),
                ..Default::default()
            };
            let quote_filter = QuoteFilter {
                year: args.year,
                client: args.client.clone(),
                ..Default::default()
            };
            let invoices = store.list(&inv_filter).await?;
            let quotes = store.list_quotes(&quote_filter).await?;

            println!(
                "{:<9} {:<16} {:<12} {:<12} {:<13} {:<14} {:<16} {}",
                "TYPE", "ID", "CLIENT", "DATE", "DUE/EXPIRES", "TOTAL", "STATUS", "PDF"
            );
            println!("{}", "-".repeat(105));

            // Invoices
            for invoice in &invoices {
                let client = store.get_client(&invoice.client).await?;
                let computed = compute_invoice(invoice, &client, &config);

                if let Some(ref status_filter) = args.status {
                    let matches = match status_filter.as_str() {
                        "unpaid" => matches!(
                            computed.status,
                            InvoiceStatus::Draft | InvoiceStatus::Sent | InvoiceStatus::Overdue
                        ),
                        s => computed.status.to_string() == s,
                    };
                    if !matches {
                        continue;
                    }
                }

                let template_html = get_template_html(&computed.template, &store.templates_dir())
                    .unwrap_or_default();
                let invoice_toml = toml::to_string(invoice).unwrap_or_default();
                let client_path = store.clients_dir().join(format!("{}.toml", client.slug));
                let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
                let me_toml = toml::to_string(&config.me).unwrap_or_default();
                let hash =
                    compute_source_hash(&invoice_toml, &client_toml, &template_html, &me_toml);

                let pdf_indicator = match check_pdf_state(&index, &invoice.id, &hash) {
                    PdfState::Fresh => "✓",
                    PdfState::Stale => "~",
                    PdfState::NeverBuilt => "—",
                };

                let total_str = format!("{} {:.2}", computed.currency, computed.total);
                let row = format!(
                    "{:<9} {:<16} {:<12} {:<12} {:<13} {:<14} {:<16} {}",
                    "invoice",
                    invoice.id,
                    client.slug,
                    computed.date.format("%Y-%m-%d"),
                    computed.due.format("%Y-%m-%d"),
                    total_str,
                    computed.status.to_string(),
                    pdf_indicator,
                );

                let colored_row = match computed.status {
                    InvoiceStatus::Overdue => row.red().to_string(),
                    InvoiceStatus::Sent => row.yellow().to_string(),
                    InvoiceStatus::Paid => row.green().to_string(),
                    InvoiceStatus::Draft => row.dimmed().to_string(),
                    _ => row,
                };
                println!("{}", colored_row);
            }

            // Quotes
            for quote in &quotes {
                let client = store.get_client(&quote.client).await?;
                let computed = compute_quote(quote, &client, &config);

                if let Some(ref sf) = args.status {
                    if computed.status.to_string() != *sf {
                        continue;
                    }
                }

                let template_html = get_template_html(&computed.template, &store.templates_dir())
                    .unwrap_or_default();
                let quote_toml = toml::to_string(quote).unwrap_or_default();
                let client_path = store.clients_dir().join(format!("{}.toml", client.slug));
                let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
                let me_toml = toml::to_string(&config.me).unwrap_or_default();
                let hash = compute_source_hash(&quote_toml, &client_toml, &template_html, &me_toml);

                let pdf_indicator = match check_pdf_state(&index, &quote.id, &hash) {
                    PdfState::Fresh => "✓",
                    PdfState::Stale => "~",
                    PdfState::NeverBuilt => "—",
                };

                let total_str = format!("{} {:.2}", computed.currency, computed.total);
                let row = format!(
                    "{:<9} {:<16} {:<12} {:<12} {:<13} {:<14} {:<16} {}",
                    "quote",
                    quote.id,
                    client.slug,
                    computed.date.format("%Y-%m-%d"),
                    computed.expires.format("%Y-%m-%d"),
                    total_str,
                    computed.status.to_string(),
                    pdf_indicator,
                );

                let colored_row = match computed.status {
                    QuoteStatus::Expired => row.red().to_string(),
                    QuoteStatus::Sent => row.yellow().to_string(),
                    QuoteStatus::Accepted => row.green().to_string(),
                    QuoteStatus::Draft | QuoteStatus::Declined => row.dimmed().to_string(),
                };
                println!("{}", colored_row);
            }
        }
    }

    Ok(())
}
