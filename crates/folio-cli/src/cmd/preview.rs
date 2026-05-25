//! `folio preview` — open the rendered invoice HTML in a browser.
//!
//! Renders the invoice with its Tera template and opens the resulting HTML
//! in the system default browser. No PDF is produced. Useful for iterating
//! on template design without needing Chrome.

use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    store::{FilesystemStore, InvoiceStore},
    templates::{get_template_html, render_invoice_html},
};
use std::io::Write;

/// Open the rendered invoice HTML in the default browser (no PDF produced).
///
/// Renders the invoice using its Tera template and writes the HTML to a
/// temporary file, then opens it with `xdg-open` / `open` / `start`.
/// Useful for iterating on template design without needing Chrome.
///
/// Examples:
///
/// ```sh
/// folio preview INV-2026-001
/// folio preview INV-2026-001 --template minimal
/// ```
#[derive(Args)]
pub struct PreviewArgs {
    /// Invoice ID to preview (e.g. `INV-2026-001`).
    pub id: String,

    /// Override the template for this preview only.
    #[arg(long)]
    pub template: Option<String>,
}

pub async fn run(args: PreviewArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let invoice = store.get(&args.id).await?;
    let client = store.get_client(&invoice.client).await?;
    let computed = compute_invoice(&invoice, &client, &config);

    let template_name = args.template.as_deref().unwrap_or(&computed.template);
    let template_html = get_template_html(template_name, &store.templates_dir())
        .map_err(|e| eyre::eyre!("{}", e))?;

    let client_json = serde_json::to_value(&client)?;
    let html = render_invoice_html(&template_html, &computed, &client_json, &config.me, &config)
        .map_err(|e| eyre::eyre!("{}", e))?;

    // Write to a temp file; the browser needs to be able to read it before we
    // drop the handle, so we sleep briefly after opening.
    let mut tmp = tempfile::Builder::new().suffix(".html").tempfile()?;
    tmp.write_all(html.as_bytes())?;
    let tmp_path = tmp.into_temp_path();

    open::that(tmp_path.to_str().unwrap())?;

    // Keep the temp file alive long enough for the browser to load it
    std::thread::sleep(std::time::Duration::from_secs(3));

    Ok(())
}
