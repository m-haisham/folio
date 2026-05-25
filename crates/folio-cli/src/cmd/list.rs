use clap::Args;
use colored::Colorize;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    index::{check_pdf_state, compute_source_hash, BuildIndex, PdfState},
    store::{FilesystemStore, InvoiceStore},
    templates::get_template_html,
    types::{InvoiceFilter, InvoiceStatus},
};
use std::fs;

#[derive(Args)]
pub struct ListArgs {
    #[arg(long)]
    pub year: Option<i32>,
    #[arg(long)]
    pub client: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
}

pub async fn run(args: ListArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::new(&root);

    let filter = InvoiceFilter {
        year: args.year,
        client: args.client.clone(),
        ..Default::default()
    };

    let invoices = store.list(&filter).await?;
    let index = BuildIndex::load(&root)?;

    println!(
        "{:<16} {:<12} {:<12} {:<12} {:<14} {:<16} {}",
        "ID", "CLIENT", "DATE", "DUE", "TOTAL", "STATUS", "PDF"
    );
    println!("{}", "-".repeat(95));

    for invoice in &invoices {
        let client = store.get_client(&invoice.client).await?;
        let computed = compute_invoice(invoice, &client, &config);

        // Filter by status if requested
        if let Some(ref status_filter) = args.status {
            let matches = match status_filter.as_str() {
                "unpaid" => matches!(
                    computed.status,
                    InvoiceStatus::Draft | InvoiceStatus::Sent | InvoiceStatus::Overdue
                ),
                s => computed.status.to_string() == s,
            };
            if !matches {
                continue;
            }
        }

        // Compute PDF state
        let template_html = get_template_html(&computed.template, &root).unwrap_or_default();
        let invoice_toml = toml::to_string(invoice).unwrap_or_default();
        let client_path = root.join("clients").join(format!("{}.toml", client.slug));
        let client_toml = fs::read_to_string(&client_path).unwrap_or_default();
        let me_toml = toml::to_string(&config.me).unwrap_or_default();
        let hash = compute_source_hash(&invoice_toml, &client_toml, &template_html, &me_toml);

        let pdf_indicator = match check_pdf_state(&index, &invoice.id, &hash) {
            PdfState::Fresh => "✓",
            PdfState::Stale => "~",
            PdfState::NeverBuilt => "—",
        };

        let total_str = format!("{} {:.2}", computed.currency, computed.total);
        let status_str = computed.status.to_string();

        let row = format!(
            "{:<16} {:<12} {:<12} {:<12} {:<14} {:<16} {}",
            invoice.id,
            client.slug,
            computed.date.format("%Y-%m-%d"),
            computed.due.format("%Y-%m-%d"),
            total_str,
            status_str,
            pdf_indicator,
        );

        let colored_row = match computed.status {
            InvoiceStatus::Overdue => row.red().to_string(),
            InvoiceStatus::Sent => row.yellow().to_string(),
            InvoiceStatus::Paid => row.green().to_string(),
            InvoiceStatus::Draft => row.dimmed().to_string(),
            _ => row,
        };

        println!("{}", colored_row);
    }

    Ok(())
}
