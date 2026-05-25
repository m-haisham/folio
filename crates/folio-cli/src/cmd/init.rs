//! `folio init` — initialise a new folio repository.
//!
//! Writes `folio.toml` from interactive prompts, adds a `.gitignore`, a
//! starter `README.md`, and runs `git init`. Directories are **not** created
//! up-front — they are created on demand when the first document is saved.

use clap::Args;
use dialoguer::Input;
use eyre::Result;
use std::fs;

/// Initialise a new folio repository in the current directory.
///
/// Runs an interactive prompt for your name, company, email, and address,
/// then writes `folio.toml`, `.gitignore`, and `README.md`.
///
/// Examples:
///
/// ```sh
/// folio init
/// folio init --name "Jane Doe" --company "janedoe.dev"
/// ```
#[derive(Args)]
pub struct InitArgs {
    /// Your full name (skips the interactive prompt).
    #[arg(long)]
    pub name: Option<String>,

    /// Your company name or domain (skips the interactive prompt).
    #[arg(long)]
    pub company: Option<String>,
}

pub async fn run(args: InitArgs) -> Result<()> {
    let root = std::env::current_dir()?;

    if root.join("folio.toml").exists() {
        eyre::bail!("folio.toml already exists in current directory");
    }

    println!("Initializing folio repository...\n");

    let theme = crate::theme::default_theme();

    let name: String = if let Some(n) = args.name {
        n
    } else {
        Input::with_theme(&theme)
            .with_prompt("Your name")
            .interact_text()?
    };

    let company: String = if let Some(c) = args.company {
        c
    } else {
        Input::with_theme(&theme)
            .with_prompt("Company / domain")
            .interact_text()?
    };

    let email: String = Input::with_theme(&theme)
        .with_prompt("Email")
        .interact_text()?;
    let address_line1: String = Input::with_theme(&theme)
        .with_prompt("Address line 1")
        .interact_text()?;
    let address_line2: String = Input::with_theme(&theme)
        .with_prompt("Address line 2 (city, country)")
        .interact_text()?;

    // ── folio.toml ────────────────────────────────────────────────────────
    //
    // [defaults]   — shared across invoices and quotes (currency, tax, template, colour, notes)
    // [invoice]    — invoice-specific (id format, payment terms, template override)
    // [quote]      — quote-specific   (id format, validity period, template override)
    //
    let config_content = format!(
        r##"[me]
name    = "{name}"
company = "{company}"
email   = "{email}"
address = ["{address_line1}", "{address_line2}"]

# Shared across both invoices and quotes
[defaults]
currency      = "USD"
tax_rate      = 0.0
template      = "basic"     # basic | classic | modern | floral | slate | signature
# primary_color = "#7c3aed" # optional accent colour for all documents
# notes = ""                # optional notes printed on every document (e.g. bank details)

# Invoice defaults
[invoice]
id_format = "INV-{{year}}-{{seq:03}}"
due_days  = 30
# template = "modern"       # optional; overrides [defaults].template for invoices only

# Quote defaults
[quote]
id_format    = "QUO-{{year}}-{{seq:03}}"
expires_days = 30
# template = "modern"       # optional; overrides [defaults].template for quotes only

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
subject = "{{{{document_type}}}} {{{{invoice.id}}}} from {{{{me.company}}}}"
# body is provided by templates/{{template_name}}/email.html
# uncomment and edit to override:
# body = """
# Hi {{{{client.contact}}}},
#
# Please find attached {{{{document_type | lower}}}} {{{{invoice.id}}}}.
#
# {{{{me.name}}}}
# """
"##
    );

    fs::write(root.join("folio.toml"), config_content)?;

    // ── .gitignore ────────────────────────────────────────────────────────
    fs::write(root.join(".gitignore"), "output/\n.folio/\n")?;

    // ── README.md ─────────────────────────────────────────────────────────
    let readme = format!(
        r#"# {company} — Invoices & Quotes

Managed with [folio](https://github.com/m-haisham/folio) — plaintext invoice
and quotation management that lives entirely in this git repository.

---

## Quick start

### 1. Add a client

Create `clients/<slug>.toml`, e.g. `clients/acme.toml`:

```toml
name    = "Acme Corp"
contact = "Jane Doe"
email   = "jane@acme.com"
address = ["123 Business St", "New York, NY 10001", "USA"]

# Optional payment details shown on every document for this client
[defaults]
notes = """
Payment via bank transfer:
Bank:    First National Bank
Account: 1234567890
"""
```

### 2. Send a quote first

```sh
folio quote new --client acme     # creates quotes/2026/QUO-2026-001.toml
folio quote build QUO-2026-001    # renders to output/QUO-2026-001.pdf
folio quote send  QUO-2026-001    # emails the PDF and records [sent]
```

### 3. Convert to an invoice when accepted

```sh
folio quote accept QUO-2026-001 --convert
# → marks quote accepted, creates invoices/2026/INV-2026-001.toml
```

### 4. Or create an invoice directly

```sh
folio invoice new --client acme
folio invoice build INV-2026-001
folio invoice send  INV-2026-001
```

### 5. Mark as paid

```sh
folio paid INV-2026-001 --amount 1200.00 --method bank_transfer --ref TXN-001
```

### 6. Review everything

```sh
folio list                  # all invoices and quotes
folio list --type invoice   # invoices only
folio list --type quote     # quotes only
folio summary               # financial totals + quote pipeline
```

---

## Repository layout

```
folio.toml          ← global config (you, defaults, email)
.gitignore          ← output/ and .folio/ are excluded

clients/
  acme.toml         ← one file per client (slug = filename)

invoices/
  2026/
    INV-2026-001.toml

quotes/
  2026/
    QUO-2026-001.toml

templates/          ← optional custom Tera/HTML templates
output/             ← generated PDFs (gitignored)
.folio/             ← build-state cache (gitignored)
```

> Directories are created automatically when the first document is saved —
> there is nothing to set up manually.

---

## Tips

- Edit `folio.toml` to change currency, tax rate, template, or accent colour.
- Set `[defaults].notes` (or `clients/<slug>.toml [defaults].notes`) for
  payment details that appear on every document.
- Run `folio templates` to list bundled templates, or export one to customise:
  `folio templates export modern --output templates/studio`
- All document files are plain TOML — commit freely, diff clearly.
"#
    );
    fs::write(root.join("README.md"), readme)?;

    // ── git init ──────────────────────────────────────────────────────────
    if !root.join(".git").exists() {
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(&root)
            .status();
        println!("✓ Initialized git repository");
    }

    println!("✓ Wrote folio.toml");
    println!("✓ Wrote .gitignore");
    println!("✓ Wrote README.md");
    println!();
    println!("Next: add a client, then run `folio quote new` or `folio invoice new`.");
    println!("Run `folio --help` for the full command reference.");

    Ok(())
}
