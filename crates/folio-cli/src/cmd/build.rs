//! `folio build` — render invoices to PDF.
//!
//! Loads the invoice TOML, resolves the client and template, renders HTML via
//! Tera, then converts it to a PDF through headless Chrome. Tracks source
//! hashes in `.folio/index.toml` so unchanged invoices are skipped on
//! subsequent runs.

use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    index::{BuildIndex, PdfState, check_pdf_state, compute_source_hash},
    pdf::PdfMargins,
    pdf::html_to_pdf,
    store::{ClientStore, FilesystemStore, InvoiceStore},
    templates::{get_template_html, render_invoice_html},
    types::{FolioConfig, InvoiceFilter},
};
use std::{fs, path::PathBuf};

/// Render one or more invoices to PDF.
///
/// Skips invoices whose source hash has not changed since the last build
/// (use `--force` to override). Requires Chrome/Chromium; set
/// `FOLIO_CHROME_PATH` if it is not on `$PATH`.
///
/// Examples:
///
/// ```sh
/// folio build INV-2026-001
/// folio build --all
/// folio build --year 2026
/// folio build --client acme
/// folio build --status draft
/// folio build INV-2026-001 --force --open
/// ```
#[derive(Args)]
pub struct BuildArgs {
    /// Invoice ID to build (e.g. `INV-2026-001`).
    pub id: Option<String>,

    /// Build all invoices.
    #[arg(long)]
    pub all: bool,

    /// Build all invoices for the given year.
    #[arg(long)]
    pub year: Option<i32>,

    /// Build all invoices for the given client slug.
    #[arg(long)]
    pub client: Option<String>,

    /// Build invoices matching the given status.
    #[arg(long)]
    pub status: Option<String>,

    /// Rebuild even if the source hash is unchanged.
    #[arg(long)]
    pub force: bool,

    /// Open the PDF in the default viewer after building.
    #[arg(long)]
    pub open: bool,

    /// Override the template for this build only.
    #[arg(long)]
    pub template: Option<String>,

    /// Write the PDF to this path instead of `output/{id}.pdf`.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub async fn run(args: BuildArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    // Create a store that respects any [paths] overrides in folio.toml
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let ids_to_build: Vec<String> = if let Some(ref id) = args.id {
        vec![id.clone()]
    } else if args.all || args.year.is_some() || args.client.is_some() || args.status.is_some() {
        let filter = InvoiceFilter {
            year: args.year,
            client: args.client.clone(),
            ..Default::default()
        };
        store
            .list(&filter)
            .await?
            .into_iter()
            .map(|i| i.id)
            .collect()
    } else {
        eyre::bail!("Specify an invoice ID, --all, --year, or --client");
    };

    let mut index = BuildIndex::load(&root)?;

    for id in &ids_to_build {
        // Auto-detect document type: if no invoice file but a quote file exists, build as quote
        if !store.invoice_path(id).exists() && store.quote_path(id).exists() {
            crate::cmd::quote::build_quote_one(
                &config,
                &store,
                &mut index,
                id,
                args.force,
                args.open,
                args.template.as_deref(),
            )
            .await?;
            continue;
        }
        build_one(&config, &store, &mut index, id, &args).await?;
    }

    index.save(&root)?;
    Ok(())
}

/// Build a single invoice and update the build index entry.
///
/// This function is also called by `folio send` when a PDF does not yet exist.
/// Returns the path of the written PDF.
pub async fn build_one(
    config: &FolioConfig,
    store: &FilesystemStore,
    index: &mut BuildIndex,
    id: &str,
    args: &BuildArgs,
) -> Result<PathBuf> {
    let invoice = store.get(id).await?;
    let client = store.get_client(&invoice.client).await?;

    let computed = compute_invoice(&invoice, &client, config);

    let template_name = args.template.as_deref().unwrap_or(&computed.template);
    let template_html = get_template_html(template_name, &store.templates_dir())
        .map_err(|e| eyre::eyre!("{}", e))?;

    // Compute a short SHA-256 hash over the source inputs to detect staleness
    let invoice_toml = toml::to_string(&invoice)?;
    let client_path = store.clients_dir().join(format!("{}.toml", client.slug));
    let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
    let source_hash = compute_source_hash(&invoice_toml, &client_toml, &template_html, config);

    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| store.output_dir().join(format!("{}.pdf", id)));

    // Skip if fresh and the PDF already exists on disk
    if !args.force {
        let state = check_pdf_state(index, id, &source_hash);
        if state == PdfState::Fresh && output_path.exists() {
            println!("  {} already up to date (use --force to rebuild)", id);
            return Ok(output_path);
        }
    }

    // Render HTML then convert to PDF via headless Chrome
    let client_json = serde_json::to_value(&client)?;
    let html = render_invoice_html(&template_html, &computed, &client_json, &config.me, config)
        .map_err(|e| eyre::eyre!("{}", e))?;

    fs::create_dir_all(output_path.parent().unwrap())?;

    html_to_pdf(&html, &output_path, PdfMargins::none()).map_err(|e| eyre::eyre!("{}", e))?;

    index.record(id, &source_hash);
    println!("✓ Built {}", output_path.display());

    if args.open {
        let _ = open::that(&output_path);
    }

    Ok(output_path)
}
