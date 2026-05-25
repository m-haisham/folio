# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-05-25

### Added

#### Core
- Plaintext invoice management stored entirely in TOML files, designed to live in a git repository.
- `folio init` — initialise a new folio repository with `folio.toml`, directory scaffold, `.gitignore`, and `git init`.
- `folio new` — interactive invoice creation with auto-generated sequential IDs; inline client creation if the client does not exist yet.
- `folio build` — render invoices to PDF via headless Chrome/Chromium, with a content-hash cache to skip unchanged invoices. Supports `--all`, `--year`, `--client`, `--status`, `--force`, and `--open` flags.
- `folio send` — email an invoice via SMTP, SendGrid, or Resend and append a `[sent]` block to the invoice file.
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
- Email provider config for SMTP (with `FOLIO_SMTP_PASSWORD` env var), SendGrid (`FOLIO_SENDGRID_API_KEY`), and Resend (`FOLIO_RESEND_API_KEY`).
- Customisable email subject and body templates (Tera syntax).
- `FOLIO_CONFIG` env var to override the path to `folio.toml`.
- `FOLIO_CHROME_PATH` env var to specify the Chrome/Chromium binary.

[0.1.0]: https://github.com/m-haisham/folio/releases/tag/v0.1.0
