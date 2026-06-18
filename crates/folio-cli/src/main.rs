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
mod theme;

use clap::{Parser, Subcommand};

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
    /// Manage invoices (new, list, build, send, paid, void, preview, summary).
    Invoice(cmd::invoice::InvoiceArgs),
    /// Manage clients.
    Client(cmd::client::ClientArgs),
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
    /// Update folio to the latest release.
    Update(cmd::update::UpdateArgs),
    /// Manage quotations.
    Quote(cmd::quote::QuoteArgs),
    /// Render a Markdown file to PDF using a folio template.
    Render(cmd::render::RenderArgs),
    /// Manage client-scoped Markdown documents.
    Doc(cmd::doc::DocArgs),
}

#[tokio::main]
async fn main() {
    install_error_hook();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init(args) => cmd::init::run(args).await,
        Commands::Invoice(args) => cmd::invoice::run(args).await,
        Commands::Client(args) => cmd::client::run(args).await,
        Commands::New(args) => cmd::new::run(args).await,
        Commands::Build(args) => cmd::build::run(args).await,
        Commands::Send(args) => cmd::send::run(args).await,
        Commands::Paid(args) => cmd::paid::run(args).await,
        Commands::Void(args) => cmd::void::run(args).await,
        Commands::List(args) => cmd::list::run(args).await,
        Commands::Summary(args) => cmd::summary::run(args).await,
        Commands::Preview(args) => cmd::preview::run(args).await,
        Commands::Templates(args) => cmd::templates::run(args).await,
        Commands::Update(args) => cmd::update::run(args).await,
        Commands::Quote(args) => cmd::quote::run(args).await,
        Commands::Render(args) => cmd::render::run(args).await,
        Commands::Doc(args) => cmd::doc::run(args).await,
    };

    if let Err(err) = result {
        print_error(&err);
        std::process::exit(1);
    }
}

/// Install an eyre hook that suppresses the default backtrace/spantrace output.
/// Actual rendering is done by [`print_error`] so we control the format exactly.
fn install_error_hook() {
    // Use the color-eyre hook for richer context capture, but we override the
    // display ourselves in `print_error` so the user never sees the raw hook output.
    color_eyre::config::HookBuilder::default()
        .display_env_section(false)
        .install()
        .expect("failed to install eyre hook");
}

/// Print an error in the spec's format:
///
/// ```text
/// error: <primary message>
///   → <context line 1>
///   → <context line 2>
/// ```
///
/// Each `wrap_err` / `wrap_err_with` call added by the CLI contributes one
/// context line. The innermost cause (the root error) is the primary message.
fn print_error(err: &eyre::Report) {
    // eyre's chain goes from outermost wrapper → root cause.
    // We want root cause first, wrappers as hints.
    let chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();

    // The last entry is the root cause; everything before it is context.
    let (hints, root) = match chain.split_last() {
        Some((last, rest)) => (rest, last.as_str()),
        None => return,
    };

    eprintln!("error: {}", root);
    for hint in hints.iter().rev() {
        eprintln!("  \u{2192} {}", hint);
    }
}
