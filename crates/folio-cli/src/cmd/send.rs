use chrono::Utc;
use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    email::{send_email, EmailMessage},
    index::BuildIndex,
    store::{FilesystemStore, InvoiceStore},
    templates::{render_email_body, render_email_subject},
    types::SentInfo,
};

#[derive(Args)]
pub struct SendArgs {
    pub id: String,
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub rebuild: bool,
}

pub async fn run(args: SendArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::new(&root);

    let mut invoice = store.get(&args.id).await?;

    if invoice.sent.is_some() && !args.force {
        eyre::bail!(
            "Invoice {} is already marked as sent — use --force to overwrite",
            args.id
        );
    }

    let client = store.get_client(&invoice.client).await?;
    let computed = compute_invoice(&invoice, &client, &config);

    // Build PDF if needed
    let pdf_path = root.join("output").join(format!("{}.pdf", args.id));
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
        super::build::build_one(&root, &config, &store, &mut index, &args.id, &build_args).await?;
        index.save(&root)?;
    }

    // Prepare email
    let to = args.to.clone().unwrap_or_else(|| client.email.clone());

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

    // Record sent info
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
