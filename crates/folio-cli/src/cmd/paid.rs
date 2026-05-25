//! `folio paid` — mark an invoice as paid.
//!
//! Writes a `[paid]` block to the invoice TOML containing the payment date,
//! amount, method, and optional reference. Status becomes `paid` (or
//! `partially_paid` if the amount is less than the invoice total).

use chrono::Local;
use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    store::{ClientStore, FilesystemStore, InvoiceStore},
    types::PaidInfo,
};
use rust_decimal::Decimal;

/// Mark an invoice as paid.
///
/// Writes a `[paid]` block to the invoice TOML. `--amount` defaults to the
/// computed invoice total; `--date` defaults to today. The invoice status
/// becomes `paid` (or `partially_paid` if the amount is less than the total).
///
/// Examples:
///
/// ```sh
/// folio paid INV-2026-001
/// folio paid INV-2026-001 --amount 2400.00
/// folio paid INV-2026-001 --amount 2400.00 --method bank_transfer --ref TXN-88821
/// folio paid INV-2026-001 --date 2026-05-15
/// ```
#[derive(Args)]
pub struct PaidArgs {
    /// Invoice ID to mark as paid (e.g. `INV-2026-001`).
    pub id: String,

    /// Payment amount (defaults to the invoice total).
    #[arg(long)]
    pub amount: Option<Decimal>,

    /// Payment method (e.g. `bank_transfer`, `paypal`).
    #[arg(long)]
    pub method: Option<String>,

    /// Payment reference or transaction ID.
    #[arg(long, name = "ref")]
    pub reference: Option<String>,

    /// Payment date in `YYYY-MM-DD` format (defaults to today).
    #[arg(long)]
    pub date: Option<String>,
}

pub async fn run(args: PaidArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let mut invoice = store
        .get(&args.id)
        .await
        .map_err(|e| eyre::eyre!(e).wrap_err(format!("could not load invoice {}", args.id)))?;

    if invoice.paid.is_some() {
        eyre::bail!("Invoice {} is already marked as paid", args.id);
    }

    let client = store.get_client(&invoice.client).await?;
    let computed = compute_invoice(&invoice, &client, &config);

    // Default to the full invoice total if no amount was specified
    let amount = args.amount.unwrap_or(computed.total);

    let date = if let Some(d) = args.date {
        chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .map_err(|e| eyre::eyre!("Invalid date: {}", e))?
    } else {
        Local::now().date_naive()
    };

    invoice.paid = Some(PaidInfo {
        at: date,
        amount,
        method: args.method,
        reference: args.reference,
    });

    store.save(&invoice).await?;
    println!("✓ Invoice {} marked as paid ({})", args.id, amount);

    Ok(())
}
