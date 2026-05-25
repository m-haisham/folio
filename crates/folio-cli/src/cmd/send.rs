//! `folio send` — email an invoice and record the `[sent]` block.
//!
//! Builds the PDF if it does not already exist, then sends it as an email
//! attachment using the provider configured in `folio.toml`. On success it
//! writes the `[sent]` block (timestamp, recipient, method) back to the
//! invoice TOML so the status advances from `draft` → `sent`.

use chrono::Utc;
use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    email::{EmailMessage, send_email},
    index::BuildIndex,
    store::{FilesystemStore, InvoiceStore},
    templates::{render_email_body, render_email_subject},
    types::SentInfo,
};

/// Send an invoice by email and record the `[sent]` block.
///
/// Builds the PDF first if it does not exist (pass `--rebuild` to force a
/// fresh render). The email subject and body are Tera templates defined in
/// `folio.toml` under `[email.templates]`. The `[sent]` block is written
/// back to the invoice TOML on success.
///
/// Fails if the invoice has already been sent — pass `--force` to resend.
///
/// Pass `--manual` to skip the email entirely and just record the `[sent]`
/// block with `method = "manual"`. Useful when an invoice was delivered
/// outside of folio (e.g. printed and handed over, or sent from another
/// mail client).
///
/// Examples:
///
/// ```sh
/// folio send INV-2026-001
/// folio send INV-2026-001 --to jane@acme.com
/// folio send INV-2026-001 --dry-run
/// folio send INV-2026-001 --force --rebuild
/// folio send INV-2026-001 --manual
/// folio send INV-2026-001 --manual --to jane@acme.com
/// ```
#[derive(Args)]
pub struct SendArgs {
    /// Invoice ID to send (e.g. `INV-2026-001`).
    pub id: String,

    /// Override the recipient email address.
    #[arg(long)]
    pub to: Option<String>,

    /// Preview the email without actually sending it.
    #[arg(long)]
    pub dry_run: bool,

    /// Resend even if a `[sent]` block already exists.
    #[arg(long)]
    pub force: bool,

    /// Rebuild the PDF before sending, even if it is already up to date.
    #[arg(long)]
    pub rebuild: bool,

    /// Mark the invoice as sent without emailing it (method = "manual").
    /// Use this when the invoice was delivered outside of folio.
    #[arg(long)]
    pub manual: bool,
}

pub async fn run(args: SendArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let mut invoice = store
        .get(&args.id)
        .await
        .map_err(|e| eyre::eyre!(e).wrap_err(format!("could not load invoice {}", args.id)))?;

    if invoice.sent.is_some() && !args.force {
        eyre::bail!(
            "Invoice {} is already marked as sent — use --force to overwrite",
            args.id
        );
    }

    let client = store.get_client(&invoice.client).await?;
    let computed = compute_invoice(&invoice, &client, &config);

    // Build PDF if it doesn't exist or an explicit rebuild was requested
    let pdf_path = store.output_dir().join(format!("{}.pdf", args.id));
    if !pdf_path.exists() || args.rebuild {
        println!("Building PDF...");
        let mut index = BuildIndex::load(&root)?;
        let build_args = super::build::BuildArgs {
            id: Some(args.id.clone()),
            all: false,
            year: None,
            client: None,
            status: None,
            force: args.rebuild,
            open: false,
            template: None,
            output: None,
        };
        super::build::build_one(&config, &store, &mut index, &args.id, &build_args).await?;
        index.save(&root)?;
    }

    // Determine recipient — flag > client email
    let to = args.to.clone().unwrap_or_else(|| client.email.clone());

    // ── Manual mode: skip email, just stamp the [sent] block ──────────────
    if args.manual {
        invoice.sent = Some(SentInfo {
            at: Utc::now(),
            method: "manual".to_string(),
            to,
            cc: None,
        });
        store.save(&invoice).await?;
        println!("✓ Invoice {} marked as sent (manual)", args.id);
        return Ok(());
    }

    // Resolve email templates with sensible defaults
    let default_subject = "Invoice {{ invoice.id }} from {{ me.company }}";
    let default_body = "Hi {{ client.contact }},\n\nPlease find attached invoice {{ invoice.id }}.\n\n{{ me.name }}\n";

    let email_config = config.email.as_ref();
    let subject_tpl = email_config
        .and_then(|e| e.templates.as_ref())
        .and_then(|t| t.subject.as_deref())
        .unwrap_or(default_subject);
    let body_tpl = email_config
        .and_then(|e| e.templates.as_ref())
        .and_then(|t| t.body.as_deref())
        .unwrap_or(default_body);

    let client_json = serde_json::to_value(&client)?;
    let subject = render_email_subject(subject_tpl, &computed, &config.me)
        .map_err(|e| eyre::eyre!("{}", e))?;
    let body = render_email_body(body_tpl, &computed, &client_json, &config.me)
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
        println!("---");
        println!("{}", body);
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

    // Persist the [sent] block so the invoice status becomes "sent"
    invoice.sent = Some(SentInfo {
        at: Utc::now(),
        method: "email".to_string(),
        to,
        cc: if cc.is_empty() { None } else { Some(cc) },
    });

    store.save(&invoice).await?;
    println!("✓ Invoice {} sent", args.id);

    Ok(())
}
