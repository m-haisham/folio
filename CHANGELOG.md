# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Client management
- `folio client new` — create a client interactively (same wizard previously only available inline during `folio new`).
- `folio client list` — table of all clients: slug, name, email, contact.
- `folio client show <slug>` — detailed view of a single client.

#### Template system
- Bundled document templates renamed from `invoice.html` → `document.html`. Custom templates using `invoice.html` continue to work as a legacy fallback.
- Each bundled template now ships with `email.html` — a Tera plain-text template for the email body, styled to match the template's personality. Resolution order: custom `email.html` → bundled `email.html` → `[email.templates].body` in config → hardcoded default.
- `folio templates export` now exports `document.html`, `email.html`, and `template.toml`.
- `render_email_body` and `render_email_subject` now receive `document_type` in their Tera context so email templates can reference `{{ document_type }}` ("Invoice" or "Quote").
- Default email subject updated to `{{ document_type }} {{ invoice.id }} from {{ me.company }}`.
- `[email.templates].body` removed from the `folio init` generated config; body is now driven by the template's `email.html`.

#### First-class invoice and quote parity
- `folio invoice` subcommand group — mirrors `folio quote` exactly: `new`, `list`, `build`, `send`, `paid`, `void`, `preview`, `summary`. Invoices and quotes are now equal citizens at the CLI level.
- `folio new --type invoice|quote` — global create command now prompts for document type when `--type` is not given.
- `folio build <id>` — auto-detects invoice vs quote from the filesystem; no flag needed.
- `folio send <id>` — auto-detects invoice vs quote from the filesystem.
- `folio preview <id>` — auto-detects invoice vs quote from the filesystem.
- `folio list` now shows **both** invoices and quotes by default with a `TYPE` column. Use `--type invoice` or `--type quote` to filter to one kind.
- `folio summary` now includes a **Quote Pipeline** section (total quoted, accepted, pending, by-status breakdown) after the invoice financials.
- `[invoice]` section in `folio.toml` — type-specific defaults for `template`, `id_format`, `due_days`; takes priority over shared `[defaults]`.
- `[quote]` section in `folio.toml` — type-specific defaults for `template`, `id_format`, `expires_days`; takes priority over shared `[defaults]`.
- `[defaults]` is now the truly shared fallback layer (currency, tax_rate, template, primary_color, notes); type-specific settings (`id_format`, `due_days`, `expires_days`) live in `[invoice]`/`[quote]` only.
- `folio init` no longer creates empty directories — they are created on demand when the first document is saved.
- `folio init` now writes a detailed `README.md` to the new repo with a full quick-start guide, client/document examples, and layout reference.

#### Quotations (first introduced this release)
- New `Quote` document type stored as `quotes/{year}/{id}.toml` — same line-item model as invoices but with an `expires` date and its own status lifecycle.
- `folio quote new` — interactive wizard to create a quote with auto-generated `QUO-{year}-{seq:03}` IDs.
- `folio quote list` — colour-coded table (expired=red, sent=yellow, accepted=green, draft/declined=dim) with PDF freshness indicator.
- `folio quote build` — render a quote to PDF using any bundled or custom template.
- `folio quote send` — email a quote as a PDF attachment and record the `[sent]` block.
- `folio quote accept` — mark a quote as accepted; `--convert` automatically creates an invoice from the quote's line items and records the resulting invoice ID in `[accepted].invoice_id`.
- `folio quote decline` — mark a quote as declined with an optional `--reason`.
- `folio quote preview` — open the rendered quote HTML in the default browser.
- Quote status lifecycle: `draft` → `sent` → `accepted` / `declined` / `expired` (auto, when `expires < today`).
- `folio init` now creates the `quotes/` directory as part of the standard scaffold.
- All six bundled templates updated to use `document_type` and `due_label` Tera variables; quotes render with `Quote` heading and `Expires` date label; existing invoices are unaffected.



## [0.1.1] - 2026-05-25

### Added
- `[defaults].notes` in `folio.toml` — a global fallback note rendered on every invoice that does not define its own `notes`.
- `[defaults].notes` in `clients/{slug}.toml` — a per-client fallback (e.g. payment details) that takes priority over the global default but loses to a note written directly on the invoice.
- Resolution order: `invoice.notes` → `client [defaults].notes` → `folio.toml [defaults].notes`.

## [0.1.0] - 2026-05-25

### Added

#### Core
- Plaintext invoice management stored entirely in TOML files, designed to live in a git repository.
- `folio init` — initialise a new folio repository with `folio.toml`, directory scaffold, `.gitignore`, and `git init`.
- `folio new` — interactive invoice creation with auto-generated sequential IDs; inline client creation if the client does not exist yet.
- `folio build` — render invoices to PDF via headless Chrome/Chromium, with a content-hash cache to skip unchanged invoices. Supports `--all`, `--year`, `--client`, `--status`, `--force`, and `--open` flags.
- `folio send` — email an invoice via SMTP, SendGrid, or Resend and append a `[sent]` block to the invoice file. Pass `--manual` to skip the email and record `method = "manual"` instead (for invoices delivered outside of folio).
- `folio paid` — mark an invoice as paid with optional `--amount`, `--method`, `--ref`, and `--date` flags.
- `folio void` — void an invoice with an optional `--reason`.
- `folio list` — colour-coded table of invoices showing ID, client, dates, total, status, and PDF freshness indicator.
- `folio summary` — aggregate financial report: total billed, paid, outstanding, per-client breakdown, and per-currency breakdown when multiple currencies are in use.
- `folio preview` — open the rendered invoice HTML in the default browser without producing a PDF; useful for iterating on template design.
- `folio templates` — list all available templates (bundled and custom), or export a bundled template to a local directory for customisation.
- `folio update` — self-update the binary to the latest GitHub release via axoupdater. `--check` flag reports availability without installing.

#### Invoice model
- Invoice status derived at runtime from file state: `draft`, `sent`, `overdue`, `paid`, `partially_paid`, `voided`.
- Per-invoice and per-client overrides for `currency`, `template`, `due_days`, `tax_rate`, and `primary_color`.
- `[sent]`, `[paid]`, and `[voided]` blocks appended in place by CLI commands, preserving all existing file content.
- Computed fields: `subtotal`, `tax_amount`, `total`, `outstanding`.

#### Templates
- Five bundled Tera/HTML invoice templates, all rendered with Tailwind CSS via CDN (no build step):
  - `basic` — clean, minimal, black and white.
  - `classic` — traditional ruled header and footer, serif typeface.
  - `modern` — bold accent colour, sans-serif, left-aligned logo block.
  - `floral` — decorative botanical header and footer, warm terracotta palette.
  - `slate` — dark header band, light body, sky-blue accent.
  - `signature` — editorial serif design; cream paper (`#fcfaf8`), forest-green ink, EB Garamond italic accents.
- Configurable primary/accent colour (`primary_color`) supported by all six templates. A single hex value is expanded into a full tonal palette (`primary`, `primary_mid`, `primary_light`, `primary_dark`, `primary_alpha_low`, `primary_alpha_very_low`) and injected as a `theme` context variable.
- Custom template support: any directory under `templates/` containing `invoice.html` is automatically discovered and takes priority over a bundled template of the same name.
- Template context includes `me`, `client`, `invoice` (with computed fields), and `theme` (when `primary_color` is set).

#### Configuration
- `folio.toml` — global config with `[me]`, `[defaults]`, `[email]`, `[build]`, and `[paths]` sections.
- Configurable directory layout via `[paths]`: `clients`, `invoices`, `templates`, `output` can each be redirected.
- Email provider config for SMTP (with `FOLIO_SMTP_PASSWORD` env var, optional for local testing), SendGrid (`FOLIO_SENDGRID_API_KEY`), and Resend (`FOLIO_RESEND_API_KEY`).
- `SmtpConfig` supports a `tls` field (`true` by default); set to `false` for plain/unencrypted SMTP connections such as local mail catchers.
- Customisable email subject and body templates (Tera syntax).
- `FOLIO_CONFIG` env var to override the path to `folio.toml`.
- `FOLIO_CHROME_PATH` env var to specify the Chrome/Chromium binary.

#### Testing
- Email integration tests against [Mailpit](https://github.com/axllent/mailpit) via `docker compose up -d`; covers plain send, CC recipients, PDF attachments, and missing-config error path.
- `docker-compose.yml` included in the repository root to spin up Mailpit on SMTP `:1025` and HTTP API `:8025`.
- Integration tests are `#[ignore]`d and opt-in: `cargo test --test email_integration -- --ignored`.

#### Distribution
- cargo-dist artifact renamed from `folio-cli-*` to `folio-*` via `[package.metadata.dist] name = "folio"` in `folio-cli/Cargo.toml`.

[0.1.0]: https://github.com/m-haisham/folio/releases/tag/v0.1.0
