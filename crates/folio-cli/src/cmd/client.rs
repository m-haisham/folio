//! `folio client` — manage clients.
//!
//! Subcommands: new, list, show.

use clap::{Args, Subcommand};
use colored::Colorize;
use dialoguer::{Input, theme::ColorfulTheme};
use eyre::Result;
use folio_core::{
    config::{find_root, load_config},
    store::{ClientStore, FilesystemStore},
    types::Client,
};

/// Manage clients.
///
/// Examples:
///
/// ```sh
/// folio client new
/// folio client list
/// folio client show acme
/// ```
#[derive(Args)]
pub struct ClientArgs {
    #[command(subcommand)]
    pub command: ClientCommand,
}

#[derive(Subcommand)]
pub enum ClientCommand {
    /// Create a new client interactively.
    New(ClientNewArgs),
    /// List all clients.
    List(ClientListArgs),
    /// Show details for a single client.
    Show(ClientShowArgs),
}

pub async fn run(args: ClientArgs) -> Result<()> {
    match args.command {
        ClientCommand::New(a) => new(a).await,
        ClientCommand::List(a) => list(a).await,
        ClientCommand::Show(a) => show(a).await,
    }
}

// ─── new ───────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct ClientNewArgs {}

async fn new(_args: ClientNewArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());
    create_client_interactive(&store).await?;
    Ok(())
}

// ─── list ──────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct ClientListArgs {}

async fn list(_args: ClientListArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let clients = store.list_clients().await?;

    if clients.is_empty() {
        println!("No clients found in {}.", store.clients_dir().display());
        println!("Run `folio client new` to add one.");
        return Ok(());
    }

    println!(
        "{:<20} {:<25} {:<30} {}",
        "SLUG", "NAME", "EMAIL", "CONTACT"
    );
    println!("{}", "-".repeat(85));

    for client in &clients {
        let contact = client.contact.as_deref().unwrap_or("—");
        println!(
            "{:<20} {:<25} {:<30} {}",
            client.slug, client.name, client.email, contact
        );
    }

    Ok(())
}

// ─── show ──────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct ClientShowArgs {
    /// Client slug (filename without `.toml`).
    pub slug: String,
}

async fn show(args: ClientShowArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd).ok_or_else(|| eyre::eyre!("No folio.toml found"))?;
    let config = load_config(&root)?;
    let store = FilesystemStore::with_paths(&root, config.paths().clone());

    let client = store
        .get_client(&args.slug)
        .await
        .map_err(|e| eyre::eyre!(e).wrap_err(format!("client {:?} not found", args.slug)))?;

    let rule = "─".repeat(40);
    println!("{}", rule.dimmed());
    println!("  {}", client.name.bold());
    println!("{}", rule.dimmed());
    println!("  {:<12} {}", "Slug:".dimmed(), client.slug);
    if let Some(ref contact) = client.contact {
        println!("  {:<12} {}", "Contact:".dimmed(), contact);
    }
    println!("  {:<12} {}", "Email:".dimmed(), client.email);

    if !client.address.is_empty() {
        for (i, line) in client.address.iter().enumerate() {
            if i == 0 {
                println!("  {:<12} {}", "Address:".dimmed(), line);
            } else {
                println!("  {:<12} {}", "", line);
            }
        }
    }

    if let Some(ref currency) = client.currency {
        println!("  {:<12} {}", "Currency:".dimmed(), currency);
    }
    if let Some(due_days) = client.due_days {
        println!("  {:<12} {} days", "Due days:".dimmed(), due_days);
    }
    if let Some(ref template) = client.template {
        println!("  {:<12} {}", "Template:".dimmed(), template);
    }

    if let Some(ref email_opts) = client.email_opts {
        if let Some(ref cc) = email_opts.cc {
            println!("  {:<12} {}", "CC:".dimmed(), cc.join(", "));
        }
    }

    if let Some(ref notes) = client.notes {
        println!("  {:<12} {}", "Notes:".dimmed(), notes);
    }

    if let Some(ref defaults) = client.defaults {
        if let Some(ref notes) = defaults.notes {
            let preview: String = notes
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect();
            println!("  {:<12} {}", "Def.notes:".dimmed(), preview);
        }
    }

    println!("{}", rule.dimmed());

    Ok(())
}

// ─── shared helper ─────────────────────────────────────────────────────────

/// Interactively prompt for client details and write `clients/{slug}.toml`.
///
/// Returns the slug of the newly created client.
pub(crate) async fn create_client_interactive(store: &FilesystemStore) -> Result<String> {
    println!();
    let theme = crate::theme::default_theme();

    let name: String = Input::with_theme(&theme)
        .with_prompt("Client name")
        .interact_text()?;

    let default_slug = name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");

    let slug: String = Input::with_theme(&theme)
        .with_prompt("Client slug (used as filename)")
        .default(default_slug)
        .interact_text()?;

    let contact: String = Input::with_theme(&theme)
        .with_prompt("Contact name")
        .allow_empty(true)
        .interact_text()?;

    let email: String = Input::with_theme(&theme)
        .with_prompt("Billing email")
        .interact_text()?;

    let addr1: String = Input::with_theme(&theme)
        .with_prompt("Address line 1")
        .allow_empty(true)
        .interact_text()?;
    let addr2: String = Input::with_theme(&theme)
        .with_prompt("Address line 2")
        .allow_empty(true)
        .interact_text()?;

    let mut address = Vec::new();
    if !addr1.is_empty() {
        address.push(addr1);
    }
    if !addr2.is_empty() {
        address.push(addr2);
    }

    let client = Client {
        name,
        contact: if contact.is_empty() {
            None
        } else {
            Some(contact)
        },
        email,
        address,
        currency: None,
        due_days: None,
        template: None,
        email_opts: None,
        notes: None,
        defaults: None,
        slug: slug.clone(),
    };

    store.save_client(&client).await?;
    println!(
        "✓ Created {}/{}.toml\n",
        store.clients_dir().display(),
        slug
    );

    Ok(slug)
}
