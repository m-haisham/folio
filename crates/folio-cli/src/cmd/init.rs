use clap::Args;
use dialoguer::Input;
use eyre::Result;
use std::fs;

#[derive(Args)]
pub struct InitArgs {
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub company: Option<String>,
}

pub async fn run(args: InitArgs) -> Result<()> {
    let root = std::env::current_dir()?;

    if root.join("folio.toml").exists() {
        eyre::bail!("folio.toml already exists in current directory");
    }

    println!("Initializing folio repository...\n");

    let name: String = if let Some(n) = args.name {
        n
    } else {
        Input::new().with_prompt("Your name").interact_text()?
    };

    let company: String = if let Some(c) = args.company {
        c
    } else {
        Input::new()
            .with_prompt("Company / domain")
            .interact_text()?
    };

    let email: String = Input::new().with_prompt("Email").interact_text()?;
    let address_line1: String = Input::new().with_prompt("Address line 1").interact_text()?;
    let address_line2: String = Input::new()
        .with_prompt("Address line 2 (city, country)")
        .interact_text()?;

    // Create directory structure
    for dir in &["clients", "invoices", "templates", "output"] {
        fs::create_dir_all(root.join(dir))?;
    }

    // Write folio.toml
    let config_content = format!(
        r#"[me]
name    = "{name}"
company = "{company}"
email   = "{email}"
address = ["{address_line1}", "{address_line2}"]

[defaults]
currency  = "USD"
tax_rate  = 0.0
due_days  = 30
template  = "basic"
id_format = "INV-{{year}}-{{seq:03}}"

[email]
provider  = "smtp"
from      = "{email}"
from_name = "{name}"

[email.smtp]
host     = "smtp.gmail.com"
port     = 587
username = "{email}"
# password via env var: FOLIO_SMTP_PASSWORD

[email.templates]
subject = "Invoice {{{{ invoice.id }}}} from {{{{ me.company }}}}"
body    = """
Hi {{{{ client.contact }}}},

Please find attached invoice {{{{ invoice.id }}}}.

{{{{ me.name }}}}
"""
"#
    );

    fs::write(root.join("folio.toml"), config_content)?;

    // Write .gitignore
    fs::write(
        root.join(".gitignore"),
        "# folio generated files\noutput/\n.folio/\n",
    )?;

    // Write README
    let readme = r#"# Invoices

Managed with [folio](https://github.com/your/folio).

## Layout

- `folio.toml` — global config
- `clients/` — one TOML per client
- `invoices/` — one TOML per invoice, grouped by year
- `templates/` — custom HTML templates
- `output/` — generated PDFs (gitignored)
"#;
    fs::write(root.join("README.md"), readme)?;

    // Run git init if not already a git repo
    if !root.join(".git").exists() {
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(&root)
            .status();
        println!("✓ Initialized git repository");
    }

    println!("✓ Created directory structure");
    println!("✓ Wrote folio.toml");
    println!("✓ Wrote .gitignore");
    println!("✓ Wrote README.md");
    println!("\nDone! Run `folio new` to create your first invoice.");

    Ok(())
}
