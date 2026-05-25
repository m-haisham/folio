//! `folio quote` — manage quotations.
//!
//! Subcommands: new, list, build, send, accept, decline, preview.

use chrono::{Datelike, Local, Utc};
use clap::{Args, Subcommand};
use colored::Colorize;
use dialoguer::{Input, Select};
use eyre::Result;
use folio_core::{
    config::{find_root, load_config},
    email::{EmailMessage, send_email},
    index::{BuildIndex, PdfState, check_pdf_state, compute_source_hash},
    pdf::html_to_pdf,
    quote_compute::compute_quote,
    store::{FilesystemStore, InvoiceStore, QuoteStore},
    templates::{
        get_email_template, get_template_html, render_email_body, render_email_subject,
        render_quote_html,
    },
    types::{
        AcceptedInfo, DeclinedInfo, FolioConfig, Invoice, LineItem, Quote, QuoteFilter,
        QuoteStatus, SentInfo,
    },
};
use rust_decimal::Decimal;
use std::{fs, io::Write, path::PathBuf, str::FromStr};

/// Manage quotations.
///
/// Examples:
///
/// ```sh
/// folio quote new --client acme
/// folio quote list
/// folio quote build QUO-2026-001
/// folio quote send  QUO-2026-001
/// folio quote accept QUO-2026-001 --convert
/// folio quote decline QUO-2026-001 --reason "Over budget"
/// folio quote preview QUO-2026-001
/// ```
#[derive(Args)]
pub struct QuoteArgs {
    #[command(subcommand)]
    pub command: QuoteCommand,
}

#[derive(Subcommand)]
pub enum QuoteCommand {
    /// Create a new quote interactively.
    New(QuoteNewArgs),
    /// List quotes in a colour-coded table.
    List(QuoteListArgs),
    /// Render a quote to PDF.
    Build(QuoteBuildArgs),
    /// Email a quote and record the [sent] block.
    Send(QuoteSendArgs),
    /// Mark a quote as accepted, optionally converting it to an invoice.
    Accept(QuoteAcceptArgs),
    /// Mark a quote as declined.
    Decline(QuoteDeclineArgs),
    /// Open the rendered quote HTML in the default browser.
    Preview(QuotePreviewArgs),
}

pub async fn run(args: QuoteArgs) -> Result<()> {
    match args.command {
        QuoteCommand::New(a) => new(a).await,
        QuoteCommand::List(a) => list(a).await,
        QuoteCommand::Build(a) => build(a).await,
        QuoteCommand::Send(a) => send(a).await,
        QuoteCommand::Accept(a) => accept(a).await,
        QuoteCommand::Decline(a) => decline(a).await,
        QuoteCommand::Preview(a) => preview(a).await,
    }
}

// ─── new ───────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct QuoteNewArgs {
    /// Client slug (must match a file in `clients/`).
    #[arg(long)]
    pub client: Option<String>,

    /// Quote date in `YYYY-MM-DD` format (defaults to today).
    #[arg(long)]
    pub date: Option<String>,
}

async fn new(args: QuoteNewArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    // Client selection
    let client_slug = if let Some(c) = args.client {
        store.get_client(&c).await.map_err(|e| {
            eyre::eyre!(e).wrap_err(format!(
                "expected file at {}/{}.toml",
                store.clients_dir().display(),
                c
            ))
        })?;
        c
    } else {
        let clients = store.list_clients().await?;
        if clients.is_empty() {
            eyre::bail!(
                "no clients found — add a TOML file to {}/ first",
                store.clients_dir().display()
            );
        }
        let names: Vec<&str> = clients.iter().map(|c| c.slug.as_str()).collect();
        let idx = Select::new()
            .with_prompt("Client")
            .items(&names)
            .interact()?;
        clients[idx].slug.clone()
    };

    // Date
    let date_str = if let Some(d) = args.date {
        d
    } else {
        let today = Local::now().format("%Y-%m-%d").to_string();
        Input::new()
            .with_prompt("Quote date")
            .default(today)
            .interact_text()?
    };
    let date = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|_| eyre::eyre!("invalid date {:?} — expected YYYY-MM-DD", date_str))?;
    let year = date.format("%Y").to_string();

    // ID generation
    let id_format = config
        .quote
        .as_ref()
        .and_then(|q| q.id_format.as_deref())
        .or(config.defaults.quote_id_format.as_deref())
        .unwrap_or("QUO-{year}-{seq:03}");
    let next_seq = next_quote_seq(&store, date.year()).await?;
    let id = crate::cmd::new::format_id(id_format, date.year(), next_seq);
    println!("Creating quote {}", id);

    // Line items
    let mut items = Vec::new();
    loop {
        let desc: String = Input::new()
            .with_prompt("Item description (empty to finish)")
            .allow_empty(true)
            .interact_text()?;
        if desc.is_empty() {
            break;
        }

        let qty_str: String = Input::new()
            .with_prompt("Quantity")
            .default("1.0".to_string())
            .interact_text()?;
        let quantity = Decimal::from_str(&qty_str)
            .map_err(|_| eyre::eyre!("invalid quantity {:?}", qty_str))?;
        let unit: String = Input::new()
            .with_prompt("Unit (e.g. hours, project)")
            .default("hours".to_string())
            .interact_text()?;
        let rate_str: String = Input::new().with_prompt("Rate").interact_text()?;
        let rate =
            Decimal::from_str(&rate_str).map_err(|_| eyre::eyre!("invalid rate {:?}", rate_str))?;

        items.push(LineItem {
            description: desc,
            quantity,
            unit: Some(unit),
            rate,
        });
    }
    if items.is_empty() {
        eyre::bail!("no items added — at least one line item is required");
    }

    let quote = Quote {
        id: id.clone(),
        client: client_slug,
        date,
        expires: None,
        currency: None,
        template: None,
        primary_color: None,
        tax_rate: None,
        notes: None,
        items,
        sent: None,
        accepted: None,
        declined: None,
    };

    store.save_quote(&quote).await?;
    println!("✓ Created quotes/{}/{}.toml", year, id);
    Ok(())
}

async fn next_quote_seq(store: &FilesystemStore, year: i32) -> Result<u32> {
    let filter = QuoteFilter {
        year: Some(year),
        ..Default::default()
    };
    let quotes = store.list_quotes(&filter).await?;
    let max = quotes
        .iter()
        .filter_map(|q| q.id.split('-').last()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    Ok(max + 1)
}

// ─── list ──────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct QuoteListArgs {
    /// Show only quotes for the given year.
    #[arg(long)]
    pub year: Option<i32>,

    /// Show only quotes for the given client slug.
    #[arg(long)]
    pub client: Option<String>,

    /// Filter by status: draft, sent, expired, accepted, declined.
    #[arg(long)]
    pub status: Option<String>,
}

async fn list(args: QuoteListArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let filter = QuoteFilter {
        year: args.year,
        client: args.client.clone(),
        ..Default::default()
    };
    let quotes = store.list_quotes(&filter).await?;
    let index = BuildIndex::load(&root)?;

    println!(
        "{:<16} {:<12} {:<12} {:<12} {:<14} {:<16} {}",
        "ID", "CLIENT", "DATE", "EXPIRES", "TOTAL", "STATUS", "PDF"
    );
    println!("{}", "-".repeat(95));

    for quote in &quotes {
        let client = store.get_client(&quote.client).await?;
        let computed = compute_quote(quote, &client, &config);

        if let Some(ref sf) = args.status {
            if computed.status.to_string() != *sf {
                continue;
            }
        }

        let template_html =
            get_template_html(&computed.template, &store.templates_dir()).unwrap_or_default();
        let quote_toml = toml::to_string(quote).unwrap_or_default();
        let client_path = store.clients_dir().join(format!("{}.toml", client.slug));
        let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
        let hash = compute_source_hash(&quote_toml, &client_toml, &template_html, &config);

        let pdf_indicator = match check_pdf_state(&index, &quote.id, &hash) {
            PdfState::Fresh => "✓",
            PdfState::Stale => "~",
            PdfState::NeverBuilt => "—",
        };

        let total_str = format!("{} {:.2}", computed.currency, computed.total);
        let row = format!(
            "{:<16} {:<12} {:<12} {:<12} {:<14} {:<16} {}",
            quote.id,
            client.slug,
            computed.date.format("%Y-%m-%d"),
            computed.expires.format("%Y-%m-%d"),
            total_str,
            computed.status.to_string(),
            pdf_indicator,
        );

        let colored_row = match computed.status {
            QuoteStatus::Expired => row.red().to_string(),
            QuoteStatus::Sent => row.yellow().to_string(),
            QuoteStatus::Accepted => row.green().to_string(),
            QuoteStatus::Draft | QuoteStatus::Declined => row.dimmed().to_string(),
        };
        println!("{}", colored_row);
    }
    Ok(())
}

// ─── build ─────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct QuoteBuildArgs {
    /// Quote ID to build (e.g. `QUO-2026-001`).
    pub id: String,

    /// Rebuild even if the source hash is unchanged.
    #[arg(long)]
    pub force: bool,

    /// Open the PDF after building.
    #[arg(long)]
    pub open: bool,

    /// Override the template for this build only.
    #[arg(long)]
    pub template: Option<String>,
}

async fn build(args: QuoteBuildArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());
    let mut index = BuildIndex::load(&root)?;

    build_quote_one(
        &config,
        &store,
        &mut index,
        &args.id,
        args.force,
        args.open,
        args.template.as_deref(),
    )
    .await?;
    index.save(&root)?;
    Ok(())
}

/// Build a single quote to PDF and update the build index entry.
///
/// Exported so that `folio quote send` can call it when the PDF is missing.
/// Returns the path of the written PDF.
pub async fn build_quote_one(
    config: &FolioConfig,
    store: &FilesystemStore,
    index: &mut BuildIndex,
    id: &str,
    force: bool,
    open: bool,
    template_override: Option<&str>,
) -> Result<PathBuf> {
    let quote = store.get_quote(id).await?;
    let client = store.get_client(&quote.client).await?;
    let computed = compute_quote(&quote, &client, config);

    let template_name = template_override.unwrap_or(&computed.template);
    let template_html = get_template_html(template_name, &store.templates_dir())
        .map_err(|e| eyre::eyre!("{}", e))?;

    let quote_toml = toml::to_string(&quote)?;
    let client_path = store.clients_dir().join(format!("{}.toml", client.slug));
    let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
    let source_hash = compute_source_hash(&quote_toml, &client_toml, &template_html, config);

    let output_path = store.output_dir().join(format!("{}.pdf", id));

    if !force {
        let state = check_pdf_state(index, id, &source_hash);
        if state == PdfState::Fresh && output_path.exists() {
            println!("  {} already up to date (use --force to rebuild)", id);
            return Ok(output_path);
        }
    }

    let client_json = serde_json::to_value(&client)?;
    let html = render_quote_html(&template_html, &computed, &client_json, &config.me, config)
        .map_err(|e| eyre::eyre!("{}", e))?;

    fs::create_dir_all(output_path.parent().unwrap())?;
    html_to_pdf(&html, &output_path).map_err(|e| eyre::eyre!("{}", e))?;

    index.record(id, &source_hash);
    println!("✓ Built {}", output_path.display());

    if open {
        let _ = open::that(&output_path);
    }

    Ok(output_path)
}

// ─── send ──────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct QuoteSendArgs {
    /// Quote ID to send.
    pub id: String,

    /// Override the recipient email address.
    #[arg(long)]
    pub to: Option<String>,

    /// Preview without actually sending.
    #[arg(long)]
    pub dry_run: bool,

    /// Resend even if already sent.
    #[arg(long)]
    pub force: bool,

    /// Rebuild the PDF before sending.
    #[arg(long)]
    pub rebuild: bool,

    /// Record as sent without emailing (method = "manual").
    #[arg(long)]
    pub manual: bool,
}

async fn send(args: QuoteSendArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let mut quote = store
        .get_quote(&args.id)
        .await
        .map_err(|e| eyre::eyre!(e).wrap_err(format!("could not load quote {}", args.id)))?;

    if quote.sent.is_some() && !args.force {
        eyre::bail!(
            "Quote {} is already marked as sent — use --force to overwrite",
            args.id
        );
    }

    let client = store.get_client(&quote.client).await?;

    let pdf_path = store.output_dir().join(format!("{}.pdf", args.id));
    if !pdf_path.exists() || args.rebuild {
        println!("Building PDF...");
        let mut index = BuildIndex::load(&root)?;
        build_quote_one(
            &config,
            &store,
            &mut index,
            &args.id,
            args.rebuild,
            false,
            None,
        )
        .await?;
        index.save(&root)?;
    }

    let to = args.to.clone().unwrap_or_else(|| client.email.clone());

    if args.manual {
        quote.sent = Some(SentInfo {
            at: Utc::now(),
            method: "manual".to_string(),
            to,
            cc: None,
        });
        store.save_quote(&quote).await?;
        println!("✓ Quote {} marked as sent (manual)", args.id);
        return Ok(());
    }

    // Build JSON context for the quote, mapping expires→due so email templates
    // can use {{ invoice.due }} just like invoice templates do.
    let computed = compute_quote(&quote, &client, &config);
    let mut inv_ctx = serde_json::to_value(&computed).map_err(|e| eyre::eyre!("{}", e))?;
    if let Some(obj) = inv_ctx.as_object_mut() {
        if let Some(expires) = obj.remove("expires") {
            obj.insert("due".to_string(), expires);
        }
        obj.insert("paid".to_string(), serde_json::Value::Null);
        obj.insert("voided".to_string(), serde_json::Value::Null);
        let total = obj.get("total").cloned().unwrap_or(serde_json::Value::Null);
        obj.insert("outstanding".to_string(), total);
    }

    let default_subject = "{{ document_type }} {{ invoice.id }} from {{ me.company }}";
    let default_body = "Hi {{ client.contact }},\n\nPlease find attached {{ document_type | lower }} {{ invoice.id }}.\n\n{{ me.name }}\n";

    let email_config = config.email.as_ref();
    let subject_tpl = email_config
        .and_then(|e| e.templates.as_ref())
        .and_then(|t| t.subject.as_deref())
        .unwrap_or(default_subject);

    let template_email = get_email_template(&computed.template, &store.templates_dir());
    let body_tpl_str;
    let body_tpl = if let Some(ref tpl) = template_email {
        tpl.as_str()
    } else {
        body_tpl_str = email_config
            .and_then(|e| e.templates.as_ref())
            .and_then(|t| t.body.clone())
            .unwrap_or_else(|| default_body.to_string());
        &body_tpl_str
    };

    let client_json = serde_json::to_value(&client)?;
    let subject = render_email_subject(subject_tpl, &inv_ctx, &config.me, "Quote")
        .map_err(|e| eyre::eyre!("{}", e))?;
    let body = render_email_body(body_tpl, &inv_ctx, &client_json, &config.me, "Quote")
        .map_err(|e| eyre::eyre!("{}", e))?;

    let cc = client
        .email_opts
        .as_ref()
        .and_then(|e| e.cc.clone())
        .unwrap_or_default();

    if args.dry_run {
        println!("--- DRY RUN ---");
        println!("To: {}", to);
        if !cc.is_empty() {
            println!("CC: {}", cc.join(", "));
        }
        println!("Subject: {}", subject);
        println!("---\n{}", body);
        println!("Attachment: {}", pdf_path.display());
        return Ok(());
    }

    let msg = EmailMessage {
        to: to.clone(),
        cc: cc.clone(),
        bcc: Vec::new(),
        subject,
        body,
        attachment_path: Some(pdf_path.to_string_lossy().to_string()),
        attachment_name: Some(format!("{}.pdf", args.id)),
    };
    send_email(&config, msg)
        .await
        .map_err(|e| eyre::eyre!("{}", e))?;

    quote.sent = Some(SentInfo {
        at: Utc::now(),
        method: "email".to_string(),
        to,
        cc: if cc.is_empty() { None } else { Some(cc) },
    });
    store.save_quote(&quote).await?;
    println!("✓ Quote {} sent", args.id);
    Ok(())
}

// ─── accept ────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct QuoteAcceptArgs {
    /// Quote ID to accept.
    pub id: String,

    /// Convert to an invoice after accepting.
    #[arg(long)]
    pub convert: bool,
}

async fn accept(args: QuoteAcceptArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let mut quote = store
        .get_quote(&args.id)
        .await
        .map_err(|e| eyre::eyre!(e).wrap_err(format!("could not load quote {}", args.id)))?;

    if quote.accepted.is_some() {
        eyre::bail!("Quote {} is already accepted", args.id);
    }

    let today = Local::now().date_naive();
    let mut invoice_id: Option<String> = None;

    if args.convert {
        let id_format = config
            .defaults
            .id_format
            .as_deref()
            .unwrap_or("INV-{year}-{seq:03}");
        let year = today.year();
        let next_seq = crate::cmd::new::next_invoice_seq(&store, year).await?;
        let inv_id = crate::cmd::new::format_id(id_format, year, next_seq);

        let invoice = Invoice {
            id: inv_id.clone(),
            client: quote.client.clone(),
            date: today,
            due: None,
            currency: quote.currency.clone(),
            template: quote.template.clone(),
            primary_color: quote.primary_color.clone(),
            tax_rate: quote.tax_rate,
            notes: quote.notes.clone(),
            items: quote.items.clone(),
            sent: None,
            paid: None,
            voided: None,
        };
        store.save(&invoice).await?;
        println!("✓ Created invoice {}", inv_id);
        invoice_id = Some(inv_id);
    }

    quote.accepted = Some(AcceptedInfo {
        at: today,
        invoice_id,
    });
    store.save_quote(&quote).await?;
    println!("✓ Quote {} accepted", args.id);
    Ok(())
}

// ─── decline ───────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct QuoteDeclineArgs {
    /// Quote ID to decline.
    pub id: String,

    /// Reason for declining.
    #[arg(long)]
    pub reason: Option<String>,
}

async fn decline(args: QuoteDeclineArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let mut quote = store
        .get_quote(&args.id)
        .await
        .map_err(|e| eyre::eyre!(e).wrap_err(format!("could not load quote {}", args.id)))?;

    if quote.declined.is_some() {
        eyre::bail!("Quote {} is already declined", args.id);
    }

    quote.declined = Some(DeclinedInfo {
        at: Local::now().date_naive(),
        reason: args.reason,
    });
    store.save_quote(&quote).await?;
    println!("✓ Quote {} declined", args.id);
    Ok(())
}

// ─── preview ───────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct QuotePreviewArgs {
    /// Quote ID to preview.
    pub id: String,

    /// Override the template for this preview only.
    #[arg(long)]
    pub template: Option<String>,
}

async fn preview(args: QuotePreviewArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let quote = store.get_quote(&args.id).await?;
    let client = store.get_client(&quote.client).await?;
    let computed = compute_quote(&quote, &client, &config);

    let template_name = args.template.as_deref().unwrap_or(&computed.template);
    let template_html = get_template_html(template_name, &store.templates_dir())
        .map_err(|e| eyre::eyre!("{}", e))?;

    let client_json = serde_json::to_value(&client)?;
    let html = render_quote_html(&template_html, &computed, &client_json, &config.me, &config)
        .map_err(|e| eyre::eyre!("{}", e))?;

    let mut tmp = tempfile::Builder::new().suffix(".html").tempfile()?;
    tmp.write_all(html.as_bytes())?;
    let tmp_path = tmp.into_temp_path();
    open::that(tmp_path.to_str().unwrap())?;
    // Give the browser a moment to load before the temp file is cleaned up.
    std::thread::sleep(std::time::Duration::from_secs(3));
    Ok(())
}
