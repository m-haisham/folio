//! `folio void` — void an invoice.
//!
//! Writes a `[voided]` block to the invoice TOML with today's date and an
//! optional reason. A voided invoice is excluded from financial summaries and
//! shown with a `voided` status in `folio list`.

use chrono::Local;
use clap::Args;
use eyre::Result;
use folio_core::{
    config::{find_root, load_config},
    store::{FilesystemStore, InvoiceStore},
    types::VoidedInfo,
};

/// Void an invoice.
///
/// Writes a `[voided]` block to the invoice TOML. Voided invoices are
/// excluded from financial totals in `folio summary` and displayed with a
/// `voided` status in `folio list`.
///
/// Examples:
///
/// ```sh
/// folio void INV-2026-001
/// folio void INV-2026-001 --reason "Duplicate invoice"
/// ```
#[derive(Args)]
pub struct VoidArgs {
    /// Invoice ID to void (e.g. `INV-2026-001`).
    pub id: String,

    /// Reason for voiding (stored in the TOML for the audit trail).
    #[arg(long)]
    pub reason: Option<String>,
}

pub async fn run(args: VoidArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let mut invoice = store.get(&args.id).await?;

    invoice.voided = Some(VoidedInfo {
        at: Local::now().date_naive(),
        reason: args.reason,
    });

    store.save(&invoice).await?;
    println!("✓ Invoice {} voided", args.id);

    Ok(())
}
