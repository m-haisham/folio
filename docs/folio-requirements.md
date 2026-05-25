# folio — Tool Requirements

## Overview

`folio` is a Rust CLI tool for managing freelance/consulting invoices entirely in plaintext.
All data lives in TOML files, all templates are HTML/CSS, and the entire workflow is designed
to be managed in a git repository. The tool generates PDFs by rendering HTML templates through
a headless browser.

---

## Goals

- **Plaintext-first**: All invoice data, client data, and config are TOML files. No database, no binary formats.
- **Git-native**: The repo layout and file design should make git history a natural audit trail.
- **Hackable templates**: Templates are plain HTML/CSS (with Tailwind CDN). Users can open them in a browser, edit them, and see changes without running any Rust.
- **Multi-client**: First-class support for managing invoices across multiple contracting clients.
- **Email integration**: The tool should be able to send invoices via email directly, populating the `[sent]` block automatically.
- **Minimal magic**: Derived state (overdue, etc.) not stored — always computed at runtime.

---

## Repository Layout

```
my-invoices/                        # user's git repo
│
├── folio.toml                      # global config (identity, defaults, email)
├── .gitignore                      # output/ and .folio/ excluded
│
├── clients/
│   ├── acme.toml
│   └── stripe.toml
│
├── invoices/
│   ├── 2025/
│   │   ├── INV-2025-001.toml
│   │   └── INV-2025-002.toml
│   └── 2026/
│       └── INV-2026-001.toml
│
├── templates/
│   ├── default/
│   │   ├── invoice.html            # Tera template
│   │   └── template.toml           # template metadata
│   └── minimal/
│       ├── invoice.html
│       └── template.toml
│
├── .folio/                         # generated — gitignored
│   └── index.toml                  # build state cache
│
└── output/                         # generated — gitignored
    └── INV-2026-001.pdf
```

---

## Configuration Files

### `folio.toml` — global config

```toml
[me]
name    = "Your Name"
company = "your-company.dev"
email   = "you@yourcompany.dev"
address = ["123 Main St", "City, Country"]

# Optional: logo path relative to repo root
logo = "assets/logo.png"

[defaults]
currency  = "USD"
tax_rate  = 0.0            # percentage, e.g. 15.0 for 15%
due_days  = 30             # used when `due` is omitted from invoice
template  = "basic"        # bundled: basic | classic | modern | floral | slate
id_format = "INV-{year}-{seq:03}"   # invoice ID format

[email]
provider = "smtp"          # smtp | sendgrid | resend
from     = "you@yourcompany.dev"
from_name = "Your Name"

# Provider-specific config
[email.smtp]
host     = "smtp.gmail.com"
port     = 587
username = "you@gmail.com"
# password via env var: FOLIO_SMTP_PASSWORD

[email.sendgrid]
# api_key via env var: FOLIO_SENDGRID_API_KEY

[email.resend]
# api_key via env var: FOLIO_RESEND_API_KEY
```

### `clients/{slug}.toml` — one file per client

```toml
name    = "Acme Corp"
contact = "Jane Doe"
email   = "jane@acme.com"
address = ["123 Business St", "New York, NY 10001", "USA"]

# Optional overrides
currency = "USD"
due_days = 14
template = "floral"        # optional; overrides folio.toml default for this client

# Optional email overrides for this client
[email]
cc  = ["finance@acme.com"]
bcc = ["you@yourcompany.dev"]

# Free-form notes (not shown on invoice)
notes = "Net-14. Contact AP dept for payment queries."
```

### `invoices/{year}/INV-{year}-{seq}.toml` — one file per invoice

```toml
id       = "INV-2026-001"
client   = "acme"              # references clients/acme.toml (without extension)
date     = "2026-05-01"
due      = "2026-05-31"        # optional; if omitted, computed from client/global due_days

# Optional invoice-level overrides
currency = "USD"
template = "modern"        # optional; overrides client/global default for this invoice
tax_rate = 0.0

notes = "Thank you for the work!"

[[items]]
description = "API architecture consulting"
quantity    = 8.0
unit        = "hours"
rate        = 150.00

[[items]]
description = "Rust auth library — Phase 1"
quantity    = 1.0
unit        = "project"
rate        = 1200.00

# Absent = not yet sent
[sent]
at     = "2026-05-03T10:00:00Z"
method = "email"
to     = "jane@acme.com"
cc     = ["finance@acme.com"]

# Absent = not yet paid
[paid]
at     = "2026-05-15"
amount = 2400.00
method = "bank_transfer"
ref    = "TXN-88821"

# Optional; if present, invoice is voided
[voided]
at     = "2026-05-20"
reason = "Duplicate invoice"
```

### `templates/{name}/template.toml` — template metadata

```toml
name        = "default"
description = "Clean, professional invoice template"
author      = "folio"
version     = "1.0.0"
```

---

## Invoice Status

Status is **always derived at runtime** — never stored explicitly.

| Condition | Derived Status |
|---|---|
| `[voided]` block present | `voided` |
| `[paid]` block present | `paid` |
| No `[sent]` block | `draft` |
| `[sent]` present, no `[paid]`, `due` ≥ today | `sent` |
| `[sent]` present, no `[paid]`, `due` < today | `overdue` |

Partial payment: if `paid.amount` < computed invoice total, status is `partially_paid` (takes precedence over `overdue`).

---

## Computed Invoice Fields

All monetary computations use `rust_decimal` — never `f64`.

| Field | Formula |
|---|---|
| `item.total` | `quantity × rate` |
| `subtotal` | `Σ item.total` |
| `tax_amount` | `subtotal × (tax_rate / 100)` |
| `total` | `subtotal + tax_amount` |
| `due` (when omitted) | `date + due_days` |
| `outstanding` | `total − paid.amount` (0 if unpaid) |

---

## CLI Commands

### `folio new`

Interactive wizard to create a new invoice TOML.

```
folio new
folio new --client acme           # pre-fills client
folio new --client acme --date 2026-05-01
```

- Prompts for client (with autocomplete from `clients/`), date, line items.
- Auto-generates the next ID based on `id_format` in config.
- Writes the file to `invoices/{year}/{id}.toml`.
- Does **not** build or send — creation is a separate step.

---

### `folio build`

Render invoice(s) to PDF.

```
folio build INV-2026-001
folio build --all
folio build --year 2026
folio build --client acme
folio build --status draft
```

- Loads invoice TOML + client TOML + global config.
- Merges into a Tera template context.
- Renders HTML via Tera.
- Converts HTML → PDF via headless Chrome.
- Writes to `output/{id}.pdf` (path configurable via `--output`).

Flags:
- `--open` — open the PDF after building
- `--preview` — open rendered HTML in browser instead of generating PDF
- `--template minimal` — override template for this build

---

### `folio send`

Send an invoice by email and record it.

```
folio send INV-2026-001
folio send INV-2026-001 --to jane@acme.com    # override recipient
folio send INV-2026-001 --dry-run             # preview email, don't send
```

- Builds the PDF if not already built (or if `--rebuild` flag is passed).
- Sends email using configured provider.
- On success: writes `[sent]` block to the invoice TOML with `at`, `method = "email"`, `to`.
- Subject line and body are Tera templates (configurable in `folio.toml`).
- Fails with a clear error if `[sent]` already exists (use `--force` to resend).

---

### `folio paid`

Mark an invoice as paid.

```
folio paid INV-2026-001
folio paid INV-2026-001 --amount 2400.00
folio paid INV-2026-001 --amount 2400.00 --method bank_transfer --ref TXN-88821
folio paid INV-2026-001 --date 2026-05-15
```

- Writes `[paid]` block to the invoice TOML.
- `--amount` defaults to the invoice total if omitted.
- `--date` defaults to today if omitted.

---

### `folio void`

Void an invoice.

```
folio void INV-2026-001
folio void INV-2026-001 --reason "Duplicate invoice"
```

- Writes `[voided]` block with `at` (today) and optional `reason`.

---

### `folio list`

Print a table of invoices.

```
folio list
folio list --year 2026
folio list --client acme
folio list --status unpaid          # draft + sent + overdue
folio list --status overdue
folio list --status paid
```

Output columns: `ID | Client | Date | Due | Total | Status`

Colour coding in terminal:
- `overdue` → red
- `sent` → yellow
- `paid` → green
- `draft` → dim

---

### `folio summary`

Aggregate financial report.

```
folio summary
folio summary --year 2026
folio summary --client acme
```

Output:
- Total billed / total paid / total outstanding
- Breakdown by client
- Breakdown by currency (if multiple)
- Count by status

---

### `folio init`

Initialise a new folio repo in the current directory.

```
folio init
folio init --name "Your Name" --company "yourcompany.dev"   # skip prompts
```

- Runs an interactive prompt for `[me]` fields (name, company, email, address).
- Creates the full directory structure: `clients/`, `invoices/`, `templates/`, `output/`.
- Writes a populated `folio.toml` from prompt answers.
- Copies the built-in `default` and `minimal` templates into `templates/`.
- Writes a `.gitignore`:

```
# folio generated files
output/
.folio/
```

- Runs `git init` if the directory is not already a git repo.
- Writes an initial `README.md` explaining the repo layout.
- Does **not** create `.folio/` — that is created on first `folio build`.

---

### `folio preview`

Open the rendered HTML in the default browser without generating a PDF.

```
folio preview INV-2026-001
folio preview INV-2026-001 --template minimal
```

Useful for iterating on template design.

---

## Build Index

`folio` tracks whether generated PDFs are up to date via a `.folio/index.toml` file in
the repo root. This directory is gitignored — it is fully derived from source files.

### `.folio/index.toml`

```toml
[builds]
"INV-2026-001" = { built_at = "2026-05-03T10:00:00Z", source_hash = "a3f9c1d2" }
"INV-2026-002" = { built_at = "2026-05-10T09:15:00Z", source_hash = "b82e4f77" }
```

### Source Hash

The `source_hash` for an invoice is a SHA-256 (truncated to 8 hex chars) of the
concatenation of:

1. The invoice TOML file contents
2. The resolved client TOML file contents
3. The template HTML file contents
4. The `folio.toml` `[me]` and `[defaults]` sections

On every `folio build`, folio recomputes the hash. If it matches the index entry, the
PDF is considered **fresh** and the build is skipped. Use `--force` to rebuild anyway.

If `.folio/` does not exist, folio creates it on first build.

### Staleness in `folio list`

`folio list` reads the index and adds a staleness indicator to the output:

| PDF state | Indicator |
|---|---|
| Fresh (hash matches) | ✓ |
| Stale (hash mismatch) | `~` |
| Never built (no index entry) | `—` |

```
ID              CLIENT   DATE        DUE         TOTAL      STATUS    PDF
INV-2026-001    acme     2026-05-01  2026-05-31  $2,400.00  paid      ✓
INV-2026-002    stripe   2026-05-10  2026-06-10  $800.00    sent      ~
INV-2026-003    acme     2026-05-20  2026-06-20  $1,200.00  draft     —
```

---

## Template System

Templates are **Tera** (Jinja2-style) HTML files. They use the Tailwind CDN so users need no build step — just edit HTML and refresh.

### Resolution Order

When resolving which template to use, folio walks this chain and uses the first value found:

```
invoice.toml [template]
  ← clients/{slug}.toml [template]
    ← folio.toml [defaults.template]
      ← "basic" (hardcoded fallback)
```

Most specific wins. All levels are optional — omitting `[template]` everywhere is valid and
will always resolve to `basic`.

### Bundled Templates

Bundled templates are embedded directly into the `folio` binary via `rust-embed`. They are
always available by name without any files in the user's repo. `folio init` does **not**
copy them — the user's `templates/` directory is reserved for custom templates only.

| Name | Description |
|---|---|
| `basic` | Clean, minimal layout. Black and white, no decoration. Default fallback. |
| `classic` | Traditional invoice style with a ruled header and footer. |
| `modern` | Bold accent colour, sans-serif, left-aligned logo block. |
| `floral` | Decorative botanical accents in the header and footer. Warm tones. |
| `slate` | Dark header band, light body. Professional and high-contrast. |

List available templates at any time:

```
folio templates
```

Output:

```
BUNDLED
  basic    Clean, minimal layout. Black and white, no decoration. (default)
  classic  Traditional invoice style with a ruled header and footer.
  modern   Bold accent colour, sans-serif, left-aligned logo block.
  floral   Decorative botanical accents in the header and footer. Warm tones.
  slate    Dark header band, light body. Professional and high-contrast.

CUSTOM (templates/)
  studio   templates/studio/invoice.html
```

### Custom Templates

Users can add their own templates by creating a directory under `templates/`:

```
templates/
└── studio/
    ├── invoice.html       # Tera template
    └── template.toml      # metadata
```

Custom templates take the same name as their directory and are resolved by name identically
to bundled ones. If a custom template shares a name with a bundled one, the custom one wins.

To start from a bundled template:

```
folio templates export basic --output templates/studio
```

This writes the bundled template files into `templates/studio/` for editing.

### Template Context

Every template receives a single context object with these keys:

```
me          — from folio.toml [me]
client      — merged: global defaults ← client TOML
invoice     — the invoice TOML, with computed fields added
  .id
  .date
  .due
  .items[]
    .description
    .quantity
    .unit
    .rate
    .total         ← computed
  .subtotal        ← computed
  .tax_rate
  .tax_amount      ← computed
  .total           ← computed
  .notes
  .status          ← computed
  .sent            ← nil if absent
  .paid            ← nil if absent
```

### Email Templates

Subject and body are also Tera templates, defined in `folio.toml`:

```toml
[email.templates]
subject = "Invoice {{ invoice.id }} from {{ me.company }}"
body    = """
Hi {{ client.contact }},

Please find attached invoice {{ invoice.id }} for {{ invoice.total | currency(invoice.currency) }},
due {{ invoice.due | date(format="%B %d, %Y") }}.

{{ me.name }}
"""
```

---

## Rendering Pipeline

```
invoice.toml + client.toml + folio.toml
        ↓
    Merged context struct (Rust)
        ↓
    Tera → rendered HTML string
        ↓
    headless_chrome crate → PDF bytes
        ↓
    output/INV-2026-001.pdf
```

### PDF Engine

Default: **headless Chrome** via the `headless_chrome` crate. Requires Chrome/Chromium installed on the system.

Fallback: `--engine wkhtmltopdf` flag, for environments without Chrome. Lower CSS fidelity.

Chrome binary path configurable via `FOLIO_CHROME_PATH` env var or `[build] chrome_path` in `folio.toml`.

---

## Key Crates

| Crate | Purpose |
|---|---|
| `clap` (derive) | CLI argument parsing |
| `serde` + `toml` | TOML data model |
| `tera` | HTML templating |
| `headless_chrome` | HTML → PDF rendering |
| `rust_decimal` | Exact monetary arithmetic |
| `chrono` | Date handling and formatting |
| `tabled` | Terminal table output for `list` |
| `thiserror` | Typed domain errors in `folio-core` |
| `eyre` | Ergonomic error propagation in CLI and server |
| `lettre` | SMTP email sending |
| `reqwest` | HTTP for SendGrid/Resend providers |
| `dialoguer` | Interactive prompts for `new` wizard and `folio init` |
| `colored` | Terminal colour output |
| `sha2` | Source hash computation for build index |
| `rust-embed` | Embed bundled templates into the binary |

---

## Error Handling

`folio-core` uses `thiserror` for typed domain errors. `folio-cli` and `folio-server`
use `eyre` for ergonomic propagation and rich human-readable reporting.

### Boundary

```rust
// folio-core: typed errors callers can match on
#[derive(Debug, thiserror::Error)]
pub enum FolioError {
    #[error("client {slug:?} not found (expected {path:?})")]
    ClientNotFound { slug: String, path: PathBuf },

    #[error("invoice {id:?} not found")]
    InvoiceNotFound { id: String },

    #[error("invoice {id:?} is already marked as sent — use --force to overwrite")]
    AlreadySent { id: String },

    #[error("invoice {id:?} is already marked as paid")]
    AlreadyPaid { id: String },

    #[error("template {name:?} not found — run `folio templates` to list available templates")]
    TemplateNotFound { name: String },

    #[error("Chrome binary not found — set FOLIO_CHROME_PATH or install Chromium")]
    ChromeNotFound,

    #[error("render error: {0}")]
    Render(#[from] tera::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// folio-cli: eyre for context-rich propagation
fn cmd_send(id: &str) -> eyre::Result<()> {
    folio_core::send(id)
        .wrap_err_with(|| format!("failed to send invoice {id}"))?;
    Ok(())
}
```

### Output

All errors should be human-readable and actionable at the terminal:

```
error: client "acme" not found
  → expected file at clients/acme.toml

error: invoice INV-2026-001 is already marked as sent
  → use --force to overwrite the [sent] block

error: Chrome not found
  → set FOLIO_CHROME_PATH or install Chromium
  → alternatively, use --engine wkhtmltopdf

error: missing required field `date` in INV-2026-001.toml
```

---

## Environment Variables

| Variable | Purpose |
|---|---|
| `FOLIO_CHROME_PATH` | Path to Chrome/Chromium binary |
| `FOLIO_SMTP_PASSWORD` | SMTP password |
| `FOLIO_SENDGRID_API_KEY` | SendGrid API key |
| `FOLIO_RESEND_API_KEY` | Resend API key |
| `FOLIO_CONFIG` | Override path to `folio.toml` |

---

## Non-Goals (v1)

- Multi-currency conversion (currencies stored as-is, no exchange rates)
- Recurring invoices (can be scripted by copying a TOML)
- Invoice approval workflows
- Client portal / web UI (v1 only — server is planned for v2)
- Stripe / payment link integration
- Time tracking integration

---

## Workspace Structure

`folio` is a Cargo workspace with three crates from the start, designed so the server is a future consumer — not a rewrite.

```
folio/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── folio-core/             # all business logic, types, rendering, email
│   ├── folio-cli/              # clap CLI — thin shell over folio-core
│   └── folio-server/           # axum server + web UI (future)
```

### `folio-core`

Contains everything that has no opinion about how it's invoked:

- Data model structs (`Invoice`, `Client`, `Config`, `LineItem`, etc.)
- TOML loading and serialisation
- Status derivation and monetary computation
- Tera template rendering
- PDF generation
- Email sending
- The `InvoiceStore` trait (see below)

### `folio-cli`

Depends only on `folio-core`. Each subcommand (`new`, `build`, `send`, etc.) is a thin
handler that parses args, calls `folio-core`, and formats output for the terminal.

### `folio-server` (future)

Depends only on `folio-core`. Axum HTTP handlers replace CLI handlers. The store
implementation can be swapped to a database without touching `folio-core`.

---

## Storage Abstraction

`folio-core` exposes an `InvoiceStore` trait from day one. In v1 only the filesystem
implementation exists, but the boundary is in place so the server can introduce a
Postgres backend later without touching business logic.

```rust
// folio-core/src/store.rs

#[async_trait]
pub trait InvoiceStore: Send + Sync {
    async fn list(&self, filter: &InvoiceFilter) -> Result<Vec<Invoice>>;
    async fn get(&self, id: &str) -> Result<Invoice>;
    async fn save(&self, invoice: &Invoice) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;

    async fn list_clients(&self) -> Result<Vec<Client>>;
    async fn get_client(&self, slug: &str) -> Result<Client>;
    async fn save_client(&self, client: &Client) -> Result<()>;
}

// v1 implementation — reads/writes TOML files on disk
pub struct FilesystemStore {
    pub root: PathBuf,
}

// future implementation — Postgres via sqlx
// pub struct PostgresStore { ... }
```

The CLI instantiates `FilesystemStore`. The future server can instantiate either,
depending on configuration.

---

## Future: `folio-server`

When the hosted server is built, the additions are:

- `folio-server` crate with Axum routes mirroring CLI commands (`POST /invoices/:id/send`, etc.)
- A `PostgresStore` impl of `InvoiceStore` (optional — `FilesystemStore` works for single-user hosted instances too)
- Auth layer (single-user token or multi-user with sessions)
- Web UI consuming the same REST API

The TOML repo format remains valid as an import/export format even after migrating to a database.

Environment variables are renamed for the server context (e.g. `FOLIO_DATABASE_URL`,
`FOLIO_CHROME_PATH`) but the same pattern applies.
