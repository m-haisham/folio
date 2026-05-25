use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    index::{check_pdf_state, compute_source_hash, BuildIndex, PdfState},
    pdf::html_to_pdf,
    store::{FilesystemStore, InvoiceStore},
    templates::{get_template_html, render_invoice_html},
    types::{FolioConfig, InvoiceFilter},
};
use std::{fs, path::PathBuf};

#[derive(Args)]
pub struct BuildArgs {
    pub id: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub year: Option<i32>,
    #[arg(long)]
    pub client: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub open: bool,
    #[arg(long)]
    pub template: Option<String>,
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub async fn run(args: BuildArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::new(&root);

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
        build_one(&root, &config, &store, &mut index, id, &args).await?;
    }

    index.save(&root)?;
    Ok(())
}

pub async fn build_one(
    root: &std::path::Path,
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
    let template_html = get_template_html(template_name, root).map_err(|e| eyre::eyre!("{}", e))?;

    // Compute source hash
    let invoice_toml = toml::to_string(&invoice)?;
    let client_path = root.join("clients").join(format!("{}.toml", client.slug));
    let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
    let me_toml = toml::to_string(&config.me)?;
    let source_hash = compute_source_hash(&invoice_toml, &client_toml, &template_html, &me_toml);

    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| root.join("output").join(format!("{}.pdf", id)));

    // Check if already fresh
    if !args.force {
        let state = check_pdf_state(index, id, &source_hash);
        if state == PdfState::Fresh && output_path.exists() {
            println!("  {} already up to date (use --force to rebuild)", id);
            return Ok(output_path);
        }
    }

    // Render HTML
    let client_json = serde_json::to_value(&client)?;
    let html = render_invoice_html(&template_html, &computed, &client_json, &config.me, config)
        .map_err(|e| eyre::eyre!("{}", e))?;

    fs::create_dir_all(output_path.parent().unwrap())?;

    html_to_pdf(&html, &output_path).map_err(|e| eyre::eyre!("{}", e))?;

    index.record(id, &source_hash);
    println!("✓ Built {}", output_path.display());

    if args.open {
        let _ = open::that(&output_path);
    }

    Ok(output_path)
}
