use chrono::Local;
use clap::Args;
use eyre::Result;
use folio_core::{
    config::find_root,
    store::{FilesystemStore, InvoiceStore},
    types::VoidedInfo,
};

#[derive(Args)]
pub struct VoidArgs {
    pub id: String,
    #[arg(long)]
    pub reason: Option<String>,
}

pub async fn run(args: VoidArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let store = FilesystemStore::new(&root);

    let mut invoice = store.get(&args.id).await?;

    invoice.voided = Some(VoidedInfo {
        at: Local::now().date_naive(),
        reason: args.reason,
    });

    store.save(&invoice).await?;
    println!("✓ Invoice {} voided", args.id);

    Ok(())
}
