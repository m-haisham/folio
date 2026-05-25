//! # folio
//!
//! `folio` is a CLI tool for managing freelance and consulting invoices entirely
//! in plaintext. All data lives in TOML files, templates are HTML/CSS, and the
//! entire workflow is designed to be tracked in a git repository.
//!
//! ## Quick start
//!
//! ```sh
//! folio init                        # initialise a new repo
//! folio new --client acme           # create a new invoice
//! folio build INV-2026-001          # render to PDF
//! folio send  INV-2026-001          # email the PDF and record [sent]
//! folio paid  INV-2026-001          # mark as paid
//! folio list                        # view all invoices
//! folio summary                     # financial overview
//! ```

mod cmd;

use clap::{Parser, Subcommand};
use eyre::Result;

/// Plaintext invoice management for freelancers.
///
/// All data lives in TOML files under the current directory (or the nearest
/// ancestor containing `folio.toml`). Run `folio init` to set up a new repo.
#[derive(Parser)]
#[command(
    name = "folio",
    version,
    author,
    propagate_version = true,
    after_help = "Run `folio <COMMAND> --help` for detailed usage of any subcommand."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise a new folio repository in the current directory.
    Init(cmd::init::InitArgs),
    /// Create a new invoice interactively.
    New(cmd::new::NewArgs),
    /// Render one or more invoices to PDF.
    Build(cmd::build::BuildArgs),
    /// Send an invoice by email and record the [sent] block.
    Send(cmd::send::SendArgs),
    /// Mark an invoice as paid.
    Paid(cmd::paid::PaidArgs),
    /// Void an invoice.
    Void(cmd::void::VoidArgs),
    /// List invoices in a table with status and PDF freshness.
    List(cmd::list::ListArgs),
    /// Show an aggregate financial summary.
    Summary(cmd::summary::SummaryArgs),
    /// Open the rendered HTML for an invoice in the default browser.
    Preview(cmd::preview::PreviewArgs),
    /// List available templates, or export a bundled one for customisation.
    Templates(cmd::templates::TemplatesArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Init(args) => cmd::init::run(args).await,
        Commands::New(args) => cmd::new::run(args).await,
        Commands::Build(args) => cmd::build::run(args).await,
        Commands::Send(args) => cmd::send::run(args).await,
        Commands::Paid(args) => cmd::paid::run(args).await,
        Commands::Void(args) => cmd::void::run(args).await,
        Commands::List(args) => cmd::list::run(args).await,
        Commands::Summary(args) => cmd::summary::run(args).await,
        Commands::Preview(args) => cmd::preview::run(args).await,
        Commands::Templates(args) => cmd::templates::run(args).await,
    }
}
