use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    store::{FilesystemStore, InvoiceStore},
    types::{InvoiceFilter, InvoiceStatus},
};
use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Args)]
pub struct SummaryArgs {
    #[arg(long)]
    pub year: Option<i32>,
    #[arg(long)]
    pub client: Option<String>,
}

pub async fn run(args: SummaryArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::new(&root);

    let filter = InvoiceFilter {
        year: args.year,
        client: args.client.clone(),
        ..Default::default()
    };

    let invoices = store.list(&filter).await?;

    let mut total_billed = Decimal::ZERO;
    let mut total_paid = Decimal::ZERO;
    let mut total_outstanding = Decimal::ZERO;
    let mut by_client: HashMap<String, (Decimal, Decimal)> = HashMap::new();
    let mut by_currency: HashMap<String, Decimal> = HashMap::new();
    let mut status_counts: HashMap<String, usize> = HashMap::new();

    for invoice in &invoices {
        let client = store.get_client(&invoice.client).await?;
        let computed = compute_invoice(invoice, &client, &config);

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

    Ok(())
}
