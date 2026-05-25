use chrono::{Datelike, Local};
use clap::Args;
use dialoguer::{Input, Select};
use eyre::Result;
use folio_core::{
    config::{find_root, load_config},
    store::{FilesystemStore, InvoiceStore},
    types::{Invoice, InvoiceFilter, LineItem},
};
use rust_decimal::Decimal;
use std::str::FromStr;

#[derive(Args)]
pub struct NewArgs {
    #[arg(long)]
    pub client: Option<String>,
    #[arg(long)]
    pub date: Option<String>,
}

pub async fn run(args: NewArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::new(&root);

    // Client selection
    let client_slug = if let Some(c) = args.client {
        c
    } else {
        let clients = store.list_clients().await?;
        if clients.is_empty() {
            eyre::bail!("No clients found. Create one in clients/ first.");
        }
        let names: Vec<&str> = clients.iter().map(|c| c.slug.as_str()).collect();
        let idx = Select::new()
            .with_prompt("Client")
            .items(&names)
            .interact()?;
        clients[idx].slug.clone()
    };

    // Verify client exists
    let _client = store.get_client(&client_slug).await?;

    // Date
    let date_str = if let Some(d) = args.date {
        d
    } else {
        let today = Local::now().format("%Y-%m-%d").to_string();
        Input::new()
            .with_prompt("Invoice date")
            .default(today)
            .interact_text()?
    };

    let date = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|e| eyre::eyre!("Invalid date: {}", e))?;

    let year = date.format("%Y").to_string();

    // Auto-generate ID
    let id_format = config
        .defaults
        .id_format
        .as_deref()
        .unwrap_or("INV-{year}-{seq:03}");

    let next_seq = next_invoice_seq(&store, date.year()).await?;
    let id = format_id(id_format, date.year(), next_seq);

    println!("Creating invoice {}", id);

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
        let quantity: Decimal =
            Decimal::from_str(&qty_str).map_err(|e| eyre::eyre!("Invalid quantity: {}", e))?;

        let unit: String = Input::new()
            .with_prompt("Unit (e.g. hours, project)")
            .default("hours".to_string())
            .interact_text()?;

        let rate_str: String = Input::new().with_prompt("Rate").interact_text()?;
        let rate: Decimal =
            Decimal::from_str(&rate_str).map_err(|e| eyre::eyre!("Invalid rate: {}", e))?;

        items.push(LineItem {
            description: desc,
            quantity,
            unit: Some(unit),
            rate,
        });
    }

    if items.is_empty() {
        eyre::bail!("No items added. Aborting.");
    }

    let invoice = Invoice {
        id: id.clone(),
        client: client_slug,
        date,
        due: None,
        currency: None,
        template: None,
        tax_rate: None,
        notes: None,
        items,
        sent: None,
        paid: None,
        voided: None,
    };

    store.save(&invoice).await?;
    println!("✓ Created invoices/{}/{}.toml", year, id);

    Ok(())
}

pub async fn next_invoice_seq(store: &FilesystemStore, year: i32) -> Result<u32> {
    let filter = InvoiceFilter {
        year: Some(year),
        ..Default::default()
    };
    let invoices = store.list(&filter).await?;

    let max_seq = invoices
        .iter()
        .filter_map(|inv| inv.id.split('-').last()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);

    Ok(max_seq + 1)
}

pub fn format_id(format: &str, year: i32, seq: u32) -> String {
    let mut result = format.to_string();
    result = result.replace("{year}", &year.to_string());

    // Handle {seq:NNN} with zero-padding
    if let Some(start) = result.find("{seq:") {
        if let Some(end_rel) = result[start..].find('}') {
            let spec = &result[start + 5..start + end_rel];
            // Count leading zeros to determine width
            let width: usize = spec.len().max(1);
            let padded = format!("{:0>width$}", seq, width = width);
            result = format!(
                "{}{}{}",
                &result[..start],
                padded,
                &result[start + end_rel + 1..]
            );
        }
    } else {
        result = result.replace("{seq}", &seq.to_string());
    }

    result
}
