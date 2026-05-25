use chrono::Local;
use clap::Args;
use eyre::Result;
use folio_core::{
    compute::compute_invoice,
    config::{find_root, load_config},
    store::{FilesystemStore, InvoiceStore},
    types::PaidInfo,
};
use rust_decimal::Decimal;

#[derive(Args)]
pub struct PaidArgs {
    pub id: String,
    #[arg(long)]
    pub amount: Option<Decimal>,
    #[arg(long)]
    pub method: Option<String>,
    #[arg(long, name = "ref")]
    pub reference: Option<String>,
    #[arg(long)]
    pub date: Option<String>,
}

pub async fn run(args: PaidArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::new(&root);

    let mut invoice = store.get(&args.id).await?;

    if invoice.paid.is_some() {
        eyre::bail!("Invoice {} is already marked as paid", args.id);
    }

    let client = store.get_client(&invoice.client).await?;
    let computed = compute_invoice(&invoice, &client, &config);

    let amount = args.amount.unwrap_or(computed.total);

    let date = if let Some(d) = args.date {
        chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .map_err(|e| eyre::eyre!("Invalid date: {}", e))?
    } else {
        Local::now().date_naive()
    };

    invoice.paid = Some(PaidInfo {
        at: date,
        amount,
        method: args.method,
        reference: args.reference,
    });

    store.save(&invoice).await?;
    println!("✓ Invoice {} marked as paid ({})", args.id, amount);

    Ok(())
}
