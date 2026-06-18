//! `folio doc` — manage client-scoped Markdown documents.
//!
//! Documents live under `clients/<slug>/documents/**/*.md` inside the folio
//! repository. PDFs are written to a mirrored path under `output/clients/`.
//!
//! Subcommands:
//!   build  — render one file, a subtree, or all docs across all clients
//!   list   — show every document and whether its PDF is up to date
//!   new    — scaffold a new .md file with starter frontmatter

use chrono::Local;
use clap::{Args, Subcommand};
use colored::Colorize;
use eyre::Result;
use folio_core::{
    config::{find_root, load_config},
    doc_store::{doc_output_path, doc_rel_path, documents_dir, list_all_docs, list_docs},
    markdown::parse_doc,
    pdf::PdfMargins,
    pdf::html_to_pdf,
    store::FilesystemStore,
    templates::{
        get_doc_footer_html, get_doc_template_html, render_doc_footer_html, render_doc_html,
    },
};
use std::{fs, path::PathBuf};

// ── Top-level args ────────────────────────────────────────────────────────────

/// Manage client-scoped Markdown documents.
///
/// Documents live at `clients/<slug>/documents/**/*.md`.
/// Rendered PDFs mirror that layout under `output/clients/`.
///
/// Examples:
///
/// ```sh
/// folio doc new  acme proposal.md
/// folio doc new  acme 2026/q3-brief.md
/// folio doc build acme
/// folio doc build acme proposal.md
/// folio doc build --all
/// folio doc list
/// folio doc list acme
/// ```
#[derive(Args)]
pub struct DocArgs {
    #[command(subcommand)]
    pub command: DocCommand,
}

#[derive(Subcommand)]
pub enum DocCommand {
    /// Render documents to PDF.
    Build(BuildArgs),
    /// List documents and their PDF status.
    List(ListArgs),
    /// Create a new Markdown document with starter frontmatter.
    New(NewArgs),
}

pub async fn run(args: DocArgs) -> Result<()> {
    match args.command {
        DocCommand::Build(a) => build(a).await,
        DocCommand::List(a) => list(a).await,
        DocCommand::New(a) => new(a).await,
    }
}

// ── doc build ────────────────────────────────────────────────────────────────

/// Render client documents to PDF.
///
/// Without `--all`, a client slug is required. An optional path argument
/// narrows the build to a single file or subdirectory within that client's
/// `documents/` folder.
///
/// Examples:
///
/// ```sh
/// folio doc build acme
/// folio doc build acme proposal.md
/// folio doc build acme 2026/q3
/// folio doc build --all
/// folio doc build --all --template modern
/// ```
#[derive(Args)]
pub struct BuildArgs {
    /// Client slug. Required unless `--all` is set.
    pub client: Option<String>,

    /// Relative path within `documents/` to build (file or directory prefix).
    pub path: Option<PathBuf>,

    /// Build documents for all clients.
    #[arg(long)]
    pub all: bool,

    /// Override the template for this build.
    #[arg(long)]
    pub template: Option<String>,

    /// Rebuild even if the PDF is already newer than the source.
    #[arg(long)]
    pub force: bool,

    /// Open the PDF after building (only effective for single-file builds).
    #[arg(long)]
    pub open: bool,
}

async fn build(args: BuildArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    // Collect (slug, absolute_source_path) pairs to build.
    let targets: Vec<(String, PathBuf)> = if args.all {
        list_all_docs(&store.clients_dir())
    } else {
        let slug = args
            .client
            .as_deref()
            .ok_or_else(|| eyre::eyre!("Provide a client slug or use --all"))?;

        // Verify client exists.
        let client_toml = store.clients_dir().join(format!("{}.toml", slug));
        if !client_toml.exists() {
            eyre::bail!("Client {:?} not found", slug);
        }

        let all_for_client = list_docs(&store.clients_dir(), slug);

        if let Some(ref rel) = args.path {
            // Filter to the specified file or directory prefix.
            let docs_dir = documents_dir(&store.clients_dir(), slug);
            let abs_filter = docs_dir.join(rel);

            let filtered: Vec<_> = all_for_client
                .into_iter()
                .filter(|p| p.starts_with(&abs_filter) || p == &abs_filter)
                .collect();

            if filtered.is_empty() {
                eyre::bail!(
                    "No .md files found under {}/{}/documents/{}",
                    store.clients_dir().display(),
                    slug,
                    rel.display()
                );
            }
            filtered
                .into_iter()
                .map(|p| (slug.to_string(), p))
                .collect()
        } else {
            all_for_client
                .into_iter()
                .map(|p| (slug.to_string(), p))
                .collect()
        }
    };

    if targets.is_empty() {
        println!("No documents found.");
        return Ok(());
    }

    let mut built_path: Option<PathBuf> = None;

    for (slug, source) in &targets {
        let output_path = doc_output_path(source, &store.clients_dir(), &store.output_dir());

        // Freshness check: skip if PDF is newer than source and --force not set.
        if !args.force && output_path.exists() {
            let src_mtime = fs::metadata(source).and_then(|m| m.modified()).ok();
            let pdf_mtime = fs::metadata(&output_path).and_then(|m| m.modified()).ok();
            if let (Some(s), Some(p)) = (src_mtime, pdf_mtime) {
                if p >= s {
                    let rel = doc_rel_path(source, &store.clients_dir(), slug);
                    println!("  {}/{} already up to date", slug, rel.display());
                    continue;
                }
            }
        }

        build_one(
            source,
            &output_path,
            slug,
            &store,
            &config.me,
            args.template.as_deref(),
            config.defaults.template.as_deref(),
        )?;

        built_path = Some(output_path);
    }

    // --open only makes sense for single-file builds.
    if args.open {
        if let Some(path) = built_path {
            if targets.len() == 1 {
                let _ = open::that(&path);
            }
        }
    }

    Ok(())
}

/// Render a single `.md` source file to PDF and print the result line.
///
/// Template resolution: `template_override` (CLI flag) > frontmatter `template:` >
/// `config_default` (`[defaults].template` from folio.toml) > `"basic"`.
fn build_one(
    source: &PathBuf,
    output_path: &PathBuf,
    slug: &str,
    store: &FilesystemStore,
    me: &folio_core::types::MeConfig,
    template_override: Option<&str>,
    config_default: Option<&str>,
) -> Result<()> {
    let md = fs::read_to_string(source)
        .map_err(|e| eyre::eyre!("Cannot read {}: {}", source.display(), e))?;

    let doc = parse_doc(&md);

    // CLI flag > frontmatter > folio.toml default > "basic"
    let template_name = template_override
        .or(doc.template.as_deref())
        .or(config_default)
        .unwrap_or("basic");

    let template_html = get_doc_template_html(template_name, &store.templates_dir())
        .map_err(|e| eyre::eyre!("{}", e))?;
    let footer_tmpl = get_doc_footer_html(template_name, &store.templates_dir());

    let html = render_doc_html(&template_html, &doc, me, None).map_err(|e| eyre::eyre!("{}", e))?;
    let footer_html = render_doc_footer_html(footer_tmpl.as_deref(), &doc, me, None)
        .map_err(|e| eyre::eyre!("{}", e))?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    html_to_pdf(
        &html,
        output_path,
        PdfMargins::document(),
        footer_html.as_deref(),
    )
    .map_err(|e| eyre::eyre!("{}", e))?;

    let rel = doc_rel_path(source, &store.clients_dir(), slug);
    println!("✓ {}/{} → {}", slug, rel.display(), output_path.display());

    Ok(())
}

// ── doc list ──────────────────────────────────────────────────────────────────

/// List client documents and their PDF status.
///
/// PDF indicators:
///   ✓  PDF exists and is newer than the source
///   ~  PDF exists but source is newer (stale)
///   —  No PDF has been built yet
///
/// Examples:
///
/// ```sh
/// folio doc list
/// folio doc list acme
/// ```
#[derive(Args)]
pub struct ListArgs {
    /// Restrict listing to this client slug.
    pub client: Option<String>,
}

async fn list(args: ListArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let targets: Vec<(String, PathBuf)> = if let Some(ref slug) = args.client {
        list_docs(&store.clients_dir(), slug)
            .into_iter()
            .map(|p| (slug.clone(), p))
            .collect()
    } else {
        list_all_docs(&store.clients_dir())
    };

    if targets.is_empty() {
        println!("No documents found.");
        return Ok(());
    }

    println!("{:<14} {:<40} {:<28} {}", "CLIENT", "PATH", "TITLE", "PDF");
    println!("{}", "─".repeat(88));

    for (slug, source) in &targets {
        let output_path = doc_output_path(source, &store.clients_dir(), &store.output_dir());

        let pdf_state = if !output_path.exists() {
            "—"
        } else {
            let src_mtime = fs::metadata(source).and_then(|m| m.modified()).ok();
            let pdf_mtime = fs::metadata(&output_path).and_then(|m| m.modified()).ok();
            match (src_mtime, pdf_mtime) {
                (Some(s), Some(p)) if p >= s => "✓",
                _ => "~",
            }
        };

        // Extract title cheaply — read file and parse frontmatter/H1.
        let title = fs::read_to_string(source)
            .ok()
            .map(|md| parse_doc(&md).title)
            .unwrap_or_default();

        let rel = doc_rel_path(source, &store.clients_dir(), slug);

        let row = format!(
            "{:<14} {:<40} {:<28} {}",
            slug,
            rel.display(),
            truncate(&title, 27),
            pdf_state,
        );

        let colored = match pdf_state {
            "✓" => row.dimmed().to_string(),
            "~" => row.yellow().to_string(),
            _ => row,
        };

        println!("{}", colored);
    }

    Ok(())
}

// ── doc new ───────────────────────────────────────────────────────────────────

/// Create a new Markdown document with starter frontmatter.
///
/// `name` is relative to `clients/<client>/documents/` and may include
/// subdirectory segments. The `.md` extension is appended if omitted.
///
/// Examples:
///
/// ```sh
/// folio doc new acme proposal
/// folio doc new acme 2026/q3-brief.md
/// folio doc new acme onboarding/welcome --template signature
/// ```
#[derive(Args)]
pub struct NewArgs {
    /// Client slug.
    pub client: String,

    /// Document name or relative path (e.g. `proposal` or `2026/q3-brief`).
    pub name: String,

    /// Template to set in the frontmatter (default: omitted, inherits folio.toml).
    #[arg(long)]
    pub template: Option<String>,
}

async fn new(args: NewArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    // Verify client exists.
    let client_toml = store.clients_dir().join(format!("{}.toml", &args.client));
    if !client_toml.exists() {
        eyre::bail!("Client {:?} not found", args.client);
    }

    // Build the file path, appending .md if needed.
    let name = if args.name.ends_with(".md") {
        args.name.clone()
    } else {
        format!("{}.md", args.name)
    };

    let docs_dir = documents_dir(&store.clients_dir(), &args.client);
    let dest = docs_dir.join(&name);

    if dest.exists() {
        eyre::bail!("{} already exists", dest.display());
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    // Derive a human title from the filename stem.
    let stem = PathBuf::from(&name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .replace(['-', '_'], " ");
    // Capitalise first letter.
    let title = {
        let mut chars = stem.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        }
    };

    let today = Local::now().format("%Y-%m-%d").to_string();
    let author = &config.me.name;

    let template_line = args
        .template
        .as_deref()
        .map(|t| format!("template: {}\n", t))
        .unwrap_or_default();

    let frontmatter = format!(
        "---\ntitle: {}\ndate: {}\nauthor: {}\n{}---\n\n# {}\n\n",
        title, today, author, template_line, title
    );

    fs::write(&dest, &frontmatter)?;

    println!("✓ Created {}", dest.display());

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max - 1).collect();
        format!("{}…", t)
    }
}
