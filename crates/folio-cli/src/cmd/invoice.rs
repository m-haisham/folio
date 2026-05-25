//! `folio invoice` — manage invoices as a first-class subcommand group.
//!
//! Each subcommand delegates directly to the existing per-command module so
//! there is no logic duplication. The individual top-level shorthands
//! (`folio build`, `folio list`, etc.) remain as convenience aliases.

use clap::{Args, Subcommand};
use eyre::Result;

/// Manage invoices.
///
/// All invoice operations are available here. The top-level shorthands
/// (`folio build`, `folio list`, etc.) remain as convenience aliases.
///
/// Examples:
///
/// ```sh
/// folio invoice new --client acme
/// folio invoice list
/// folio invoice list --status overdue
/// folio invoice build INV-2026-001
/// folio invoice send  INV-2026-001
/// folio invoice paid  INV-2026-001 --amount 2400.00
/// folio invoice void  INV-2026-001 --reason "Duplicate"
/// folio invoice preview INV-2026-001
/// folio invoice summary
/// ```
#[derive(Args)]
pub struct InvoiceArgs {
    #[command(subcommand)]
    pub command: InvoiceCommand,
}

#[derive(Subcommand)]
pub enum InvoiceCommand {
    /// Create a new invoice interactively.
    New(super::new::NewArgs),
    /// List invoices in a colour-coded table.
    List(super::list::ListArgs),
    /// Render one or more invoices to PDF.
    Build(super::build::BuildArgs),
    /// Send an invoice by email and record the [sent] block.
    Send(super::send::SendArgs),
    /// Mark an invoice as paid.
    Paid(super::paid::PaidArgs),
    /// Void an invoice.
    Void(super::void::VoidArgs),
    /// Open the rendered invoice HTML in the default browser.
    Preview(super::preview::PreviewArgs),
    /// Show an aggregate financial summary.
    Summary(super::summary::SummaryArgs),
}

pub async fn run(args: InvoiceArgs) -> Result<()> {
    match args.command {
        InvoiceCommand::New(a) => super::new::run(a).await,
        InvoiceCommand::List(a) => super::list::run(a).await,
        InvoiceCommand::Build(a) => super::build::run(a).await,
        InvoiceCommand::Send(a) => super::send::run(a).await,
        InvoiceCommand::Paid(a) => super::paid::run(a).await,
        InvoiceCommand::Void(a) => super::void::run(a).await,
        InvoiceCommand::Preview(a) => super::preview::run(a).await,
        InvoiceCommand::Summary(a) => super::summary::run(a).await,
    }
}
