//! `folio summary` — aggregate financial report.
//!
//! Totals billed, paid, and outstanding across all non-voided invoices.
//! Breaks the numbers down by client and, when multiple currencies are
//! present, by currency. Also shows a count of invoices per status.

use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    store::{FilesystemStore, InvoiceStore, QuoteStore},
    types::{InvoiceFilter, InvoiceStatus},
};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Show an aggregate financial summary.
///
/// Sums billed, paid, and outstanding amounts across all non-voided invoices.
/// Provides a per-client breakdown and, when multiple currencies are involved,
/// a per-currency subtotal. A status count is printed at the end.
///
/// Examples:
///
/// ```sh
/// folio summary
/// folio summary --year 2026
/// folio summary --client acme
/// ```
#[derive(Args)]
pub struct SummaryArgs {
    /// Restrict the report to the given year.
    #[arg(long)]
    pub year: Option<i32>,

    /// Restrict the report to the given client slug.
    #[arg(long)]
    pub client: Option<String>,
}

pub async fn run(args: SummaryArgs) -> Result<()> {
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

    let mut total_billed = Decimal::ZERO;
    let mut total_paid = Decimal::ZERO;
    let mut total_outstanding = Decimal::ZERO;
    // (billed, paid) per client slug
    let mut by_client: HashMap<String, (Decimal, Decimal)> = HashMap::new();
    let mut by_currency: HashMap<String, Decimal> = HashMap::new();
    let mut status_counts: HashMap<String, usize> = HashMap::new();

    for invoice in &invoices {
        let client = store.get_client(&invoice.client).await?;
        let computed = compute_invoice(invoice, &client, &config);

        // Voided invoices are excluded from all financial totals
        if matches!(computed.status, InvoiceStatus::Voided) {
            continue;
        }

        total_billed += computed.total;
        if let Some(ref paid) = invoice.paid {
            total_paid += paid.amount;
        }
        total_outstanding += computed.outstanding;

        let entry = by_client
            .entry(client.slug.clone())
            .or_insert((Decimal::ZERO, Decimal::ZERO));
        entry.0 += computed.total;
        if let Some(ref paid) = invoice.paid {
            entry.1 += paid.amount;
        }

        *by_currency
            .entry(computed.currency.clone())
            .or_insert(Decimal::ZERO) += computed.total;
        *status_counts
            .entry(computed.status.to_string())
            .or_insert(0) += 1;
    }

    println!("=== Summary ===\n");
    println!("Total billed:       {:>12.2}", total_billed);
    println!("Total paid:         {:>12.2}", total_paid);
    println!("Total outstanding:  {:>12.2}", total_outstanding);

    println!("\n--- By Client ---");
    let mut clients: Vec<_> = by_client.iter().collect();
    clients.sort_by_key(|(k, _)| k.as_str());
    for (slug, (billed, paid)) in &clients {
        println!(
            "  {:<15} billed: {:>10.2}  paid: {:>10.2}",
            slug, billed, paid
        );
    }

    // Currency breakdown is only shown when there is more than one currency
    if by_currency.len() > 1 {
        println!("\n--- By Currency ---");
        for (currency, total) in &by_currency {
            println!("  {}: {:.2}", currency, total);
        }
    }

    println!("\n--- By Status ---");
    let mut statuses: Vec<_> = status_counts.iter().collect();
    statuses.sort_by_key(|(k, _)| k.as_str());
    for (status, count) in &statuses {
        println!("  {:<15} {}", status, count);
    }

    // ── Quote pipeline ────────────────────────────────────────────────────────
    let quote_filter = folio_core::types::QuoteFilter {
        year: args.year,
        client: args.client.clone(),
        ..Default::default()
    };
    let quotes = store.list_quotes(&quote_filter).await?;

    if !quotes.is_empty() {
        let mut quoted_total = Decimal::ZERO;
        let mut accepted_total = Decimal::ZERO;
        let mut quote_status_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for quote in &quotes {
            let client = store.get_client(&quote.client).await?;
            let computed = folio_core::quote_compute::compute_quote(quote, &client, &config);
            quoted_total += computed.total;
            if matches!(computed.status, folio_core::types::QuoteStatus::Accepted) {
                accepted_total += computed.total;
            }
            *quote_status_counts
                .entry(computed.status.to_string())
                .or_insert(0) += 1;
        }

        let pending_total = quoted_total - accepted_total;

        println!("\n=== Quote Pipeline ===\n");
        println!("Total quoted:       {:>12.2}", quoted_total);
        println!("Accepted:           {:>12.2}", accepted_total);
        println!("Pending:            {:>12.2}", pending_total);
        println!("\n--- By Status ---");
        let mut q_statuses: Vec<_> = quote_status_counts.iter().collect();
        q_statuses.sort_by_key(|(k, _)| k.as_str());
        for (status, count) in &q_statuses {
            println!("  {:<15} {}", status, count);
        }
    }

    Ok(())
}
