//! `folio render` — convert a Markdown file to a styled PDF.
//!
//! Parses the Markdown (with optional YAML frontmatter), resolves a template,
//! renders it to HTML via Tera, then exports to PDF through headless Chrome.
//!
//! Does not require a folio repository — falls back to an empty `MeConfig` and
//! the `basic` template when run outside one.

use clap::Args;
use eyre::Result;
use folio_core::{
    config::{find_root, load_config},
    markdown::parse_doc,
    pdf::html_to_pdf,
    store::FilesystemStore,
    templates::{get_doc_template_html, render_doc_html},
    types::MeConfig,
};
use std::{fs, path::PathBuf};

/// Convert a Markdown file to a styled PDF using a folio template.
///
/// Supports YAML frontmatter for metadata (`title`, `date`, `author`,
/// `template`, `primary_color`). The document title is taken from a
/// `title:` frontmatter key or the first `# Heading` in the file.
///
/// Examples:
///
/// ```sh
/// folio render README.md
/// folio render notes.md --template modern --open
/// folio render report.md --output ~/Desktop/report.pdf
/// ```
#[derive(Args)]
pub struct RenderArgs {
    /// Path to the Markdown file to render.
    pub file: PathBuf,

    /// Output PDF path. Defaults to the same directory as the input file,
    /// with a `.pdf` extension.
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Template to use. Overrides frontmatter `template:` and folio.toml default.
    #[arg(long)]
    pub template: Option<String>,

    /// Open the PDF after rendering.
    #[arg(long)]
    pub open: bool,
}

pub async fn run(args: RenderArgs) -> Result<()> {
    // Read the Markdown source
    let md_source = fs::read_to_string(&args.file)
        .map_err(|e| eyre::eyre!("Could not read {}: {}", args.file.display(), e))?;

    // Parse frontmatter + body
    let doc = parse_doc(&md_source);

    // Resolve folio config (optional — graceful fallback outside a repo)
    let cwd = std::env::current_dir()?;
    let (me, templates_dir) = if let Some(root) = find_root(&cwd) {
        match load_config(&root) {
            Ok(config) => {
                let store = FilesystemStore::with_paths(&root, config.paths().clone());
                (config.me.clone(), store.templates_dir())
            }
            Err(_) => (MeConfig::default(), PathBuf::from("templates")),
        }
    } else {
        (MeConfig::default(), PathBuf::from("templates"))
    };

    // Resolve template name (CLI flag > frontmatter > folio.toml default > "basic")
    let template_name = args
        .template
        .as_deref()
        .or(doc.template.as_deref())
        .unwrap_or("basic")
        .to_string();

    let template_html =
        get_doc_template_html(&template_name, &templates_dir).map_err(|e| eyre::eyre!("{}", e))?;

    let html =
        render_doc_html(&template_html, &doc, &me, None).map_err(|e| eyre::eyre!("{}", e))?;

    // Resolve output path
    let output_path = args.output.clone().unwrap_or_else(|| {
        let stem = args
            .file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let dir = args
            .file
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        dir.join(format!("{}.pdf", stem))
    });

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    html_to_pdf(&html, &output_path).map_err(|e| eyre::eyre!("{}", e))?;
    println!("✓ Rendered {}", output_path.display());

    if args.open {
        let _ = open::that(&output_path);
    }

    Ok(())
}
