//! `folio list` — tabular view of all invoices.
//!
//! Prints every invoice as a colour-coded table row showing ID, client, dates,
//! total, derived status, and whether the PDF is fresh, stale, or not yet
//! built. Supports filtering by year, client, or status.

use clap::Args;
use colored::Colorize;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    index::{check_pdf_state, compute_source_hash, BuildIndex, PdfState},
    store::{FilesystemStore, InvoiceStore},
    templates::get_template_html,
    types::{InvoiceFilter, InvoiceStatus},
};
use std::fs;

/// List invoices in a colour-coded table with status and PDF freshness.
///
/// PDF state indicators:
/// - `✓` — PDF is up to date
/// - `~` — PDF exists but source has changed
/// - `—` — PDF has never been built
///
/// Status colour coding: overdue=red, sent=yellow, paid=green, draft=dim.
///
/// The special status filter `unpaid` matches `draft`, `sent`, and `overdue`.
///
/// Examples:
///
/// ```sh
/// folio list
/// folio list --year 2026
/// folio list --client acme
/// folio list --status unpaid
/// folio list --status overdue
/// folio list --status paid
/// ```
#[derive(Args)]
pub struct ListArgs {
    /// Show only invoices for the given year.
    #[arg(long)]
    pub year: Option<i32>,

    /// Show only invoices for the given client slug.
    #[arg(long)]
    pub client: Option<String>,

    /// Show only invoices with the given status.
    ///
    /// Accepts any derived status (`draft`, `sent`, `overdue`, `paid`,
    /// `partially_paid`, `voided`) or the special value `unpaid` (which
    /// matches `draft`, `sent`, and `overdue`).
    #[arg(long)]
    pub status: Option<String>,
}

pub async fn run(args: ListArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let filter = InvoiceFilter {
        year: args.year,
        client: args.client.clone(),
        ..Default::default()
    };

    let invoices = store.list(&filter).await?;
    let index = BuildIndex::load(&root)?;

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

        // Determine PDF freshness by comparing the stored hash to the current one
        let template_html =
            get_template_html(&computed.template, &store.templates_dir()).unwrap_or_default();
        let invoice_toml = toml::to_string(invoice).unwrap_or_default();
        let client_path = store.clients_dir().join(format!("{}.toml", client.slug));
        let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
        let me_toml = toml::to_string(&config.me).unwrap_or_default();
        let hash = compute_source_hash(&invoice_toml, &client_toml, &template_html, &me_toml);

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

    Ok(())
}
