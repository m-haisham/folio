use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MeConfig {
    pub name: String,
    pub company: Option<String>,
    pub email: String,
    pub address: Vec<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Defaults {
    pub currency: Option<String>,
    pub tax_rate: Option<Decimal>,
    pub due_days: Option<u32>,
    pub template: Option<String>,
    pub id_format: Option<String>,
    /// Primary/accent color as a CSS hex string, e.g. "#7c3aed".
    /// Overrides the template's built-in default when set.
    pub primary_color: Option<String>,
    /// Default notes appended to every invoice when the invoice itself has no notes.
    pub notes: Option<String>,
    /// Default ID format for quotes, e.g. "QUO-{year}-{seq:03}".
    pub quote_id_format: Option<String>,
    /// Number of days a quote is valid (used to compute `expires` when omitted).
    pub expires_days: Option<u32>,
}

/// Invoice-specific defaults. These take priority over the shared `[defaults]` section.
/// Settable under `[invoice]` in `folio.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InvoiceDefaults {
    /// Override `[defaults].template` for invoices only.
    pub template: Option<String>,
    /// Override `[defaults].id_format` for invoices only.
    pub id_format: Option<String>,
    /// Override `[defaults].due_days` for invoices only.
    pub due_days: Option<u32>,
}

/// Quote-specific defaults. These take priority over the shared `[defaults]` section.
/// Settable under `[quote]` in `folio.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuoteDefaults {
    /// Override `[defaults].template` for quotes only.
    pub template: Option<String>,
    /// Override `[defaults].quote_id_format` for quotes only.
    pub id_format: Option<String>,
    /// Override `[defaults].expires_days` for quotes only.
    pub expires_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailConfig {
    pub provider: Option<String>,
    pub from: Option<String>,
    pub from_name: Option<String>,
    pub smtp: Option<SmtpConfig>,
    pub sendgrid: Option<SendgridConfig>,
    pub resend: Option<ResendConfig>,
    pub templates: Option<EmailTemplates>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Set to `false` to use an unencrypted connection (e.g. local Mailpit).
    /// Defaults to `true`.
    pub tls: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SendgridConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResendConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailTemplates {
    pub subject: Option<String>,
    pub body: Option<String>,
}

/// Directory layout configuration. All values are relative to the repo root.
/// Omitting `[paths]` in `folio.toml` is equivalent to the defaults shown below.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    /// Directory containing `{slug}.toml` client files. Default: `"clients"`.
    pub clients: Option<String>,
    /// Directory containing `{year}/{id}.toml` invoice files. Default: `"invoices"`.
    pub invoices: Option<String>,
    /// Directory searched for custom Tera HTML templates. Default: `"templates"`.
    pub templates: Option<String>,
    /// Directory where rendered PDFs are written. Default: `"output"`.
    pub output: Option<String>,
    /// Directory containing `{year}/{id}.toml` quote files. Default: `"quotes"`.
    pub quotes: Option<String>,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            clients: None,
            invoices: None,
            templates: None,
            output: None,
            quotes: None,
        }
    }
}

impl PathsConfig {
    /// Return the effective `clients` path, falling back to `"clients"`.
    pub fn clients(&self) -> &str {
        self.clients.as_deref().unwrap_or("clients")
    }
    /// Return the effective `invoices` path, falling back to `"invoices"`.
    pub fn invoices(&self) -> &str {
        self.invoices.as_deref().unwrap_or("invoices")
    }
    /// Return the effective `templates` path, falling back to `"templates"`.
    pub fn templates(&self) -> &str {
        self.templates.as_deref().unwrap_or("templates")
    }
    /// Return the effective `output` path, falling back to `"output"`.
    pub fn output(&self) -> &str {
        self.output.as_deref().unwrap_or("output")
    }
    /// Return the effective `quotes` path, falling back to `"quotes"`.
    pub fn quotes(&self) -> &str {
        self.quotes.as_deref().unwrap_or("quotes")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildConfig {
    pub chrome_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FolioConfig {
    pub me: MeConfig,
    pub defaults: Defaults,
    /// Invoice-specific defaults (`[invoice]` section). Overrides shared `[defaults]`.
    pub invoice: Option<InvoiceDefaults>,
    /// Quote-specific defaults (`[quote]` section). Overrides shared `[defaults]`.
    pub quote: Option<QuoteDefaults>,
    pub email: Option<EmailConfig>,
    pub build: Option<BuildConfig>,
    /// Optional directory layout overrides. Defaults to conventional names when absent.
    pub paths: Option<PathsConfig>,
}

impl FolioConfig {
    /// Return a reference to the resolved `PathsConfig`, using defaults if the
    /// `[paths]` section was omitted from `folio.toml`.
    pub fn paths(&self) -> &PathsConfig {
        self.paths.as_ref().map_or(&DEFAULT_PATHS, |p| p)
    }
}

static DEFAULT_PATHS: PathsConfig = PathsConfig {
    clients: None,
    invoices: None,
    templates: None,
    output: None,
    quotes: None,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub name: String,
    pub contact: Option<String>,
    pub email: String,
    pub address: Vec<String>,
    pub currency: Option<String>,
    pub due_days: Option<u32>,
    pub template: Option<String>,
    #[serde(rename = "email_opts", default)]
    pub email_opts: Option<ClientEmailOpts>,
    pub notes: Option<String>,
    /// Per-client invoice defaults.
    pub defaults: Option<ClientDefaults>,
    /// Not stored in file — set after loading
    #[serde(skip)]
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientDefaults {
    /// Default notes for invoices raised against this client (e.g. payment details).
    /// Takes priority over `[defaults].notes` in `folio.toml` but loses to a
    /// note written directly on the invoice.
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientEmailOpts {
    pub cc: Option<Vec<String>>,
    pub bcc: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItem {
    pub description: String,
    pub quantity: Decimal,
    pub unit: Option<String>,
    pub rate: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SentInfo {
    pub at: DateTime<Utc>,
    pub method: String,
    pub to: String,
    pub cc: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaidInfo {
    pub at: NaiveDate,
    pub amount: Decimal,
    pub method: Option<String>,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoidedInfo {
    pub at: NaiveDate,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: String,
    pub client: String,
    pub date: NaiveDate,
    pub due: Option<NaiveDate>,
    pub currency: Option<String>,
    pub template: Option<String>,
    /// Per-invoice primary color override (takes priority over defaults).
    pub primary_color: Option<String>,
    pub tax_rate: Option<Decimal>,
    pub notes: Option<String>,
    #[serde(rename = "items", default)]
    pub items: Vec<LineItem>,
    pub sent: Option<SentInfo>,
    pub paid: Option<PaidInfo>,
    pub voided: Option<VoidedInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum InvoiceStatus {
    Draft,
    Sent,
    Overdue,
    Paid,
    PartiallyPaid,
    Voided,
}

impl std::fmt::Display for InvoiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Sent => write!(f, "sent"),
            Self::Overdue => write!(f, "overdue"),
            Self::Paid => write!(f, "paid"),
            Self::PartiallyPaid => write!(f, "partially_paid"),
            Self::Voided => write!(f, "voided"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ComputedInvoice {
    pub id: String,
    pub client: String,
    pub date: NaiveDate,
    pub due: NaiveDate,
    pub currency: String,
    pub template: String,
    /// Resolved primary color (may be None if neither the invoice nor defaults set one).
    pub primary_color: Option<String>,
    pub tax_rate: Decimal,
    pub notes: Option<String>,
    pub items: Vec<ComputedLineItem>,
    pub subtotal: Decimal,
    pub tax_amount: Decimal,
    pub total: Decimal,
    pub outstanding: Decimal,
    pub status: InvoiceStatus,
    pub sent: Option<SentInfo>,
    pub paid: Option<PaidInfo>,
    pub voided: Option<VoidedInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComputedLineItem {
    pub description: String,
    pub quantity: Decimal,
    pub unit: Option<String>,
    pub rate: Decimal,
    pub total: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AcceptedInfo {
    pub at: NaiveDate,
    /// Invoice ID created from this quote via `folio quote accept --convert`.
    pub invoice_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeclinedInfo {
    pub at: NaiveDate,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub id: String,
    pub client: String,
    pub date: NaiveDate,
    /// Expiry date. Computed from `expires_days` in config if omitted.
    pub expires: Option<NaiveDate>,
    pub currency: Option<String>,
    pub template: Option<String>,
    pub primary_color: Option<String>,
    pub tax_rate: Option<Decimal>,
    pub notes: Option<String>,
    #[serde(rename = "items", default)]
    pub items: Vec<LineItem>,
    pub sent: Option<SentInfo>,
    pub accepted: Option<AcceptedInfo>,
    pub declined: Option<DeclinedInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QuoteStatus {
    Draft,
    Sent,
    Expired,
    Accepted,
    Declined,
}

impl std::fmt::Display for QuoteStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Sent => write!(f, "sent"),
            Self::Expired => write!(f, "expired"),
            Self::Accepted => write!(f, "accepted"),
            Self::Declined => write!(f, "declined"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ComputedQuote {
    pub id: String,
    pub client: String,
    pub date: NaiveDate,
    pub expires: NaiveDate,
    pub currency: String,
    pub template: String,
    pub primary_color: Option<String>,
    pub tax_rate: Decimal,
    pub notes: Option<String>,
    pub items: Vec<ComputedLineItem>,
    pub subtotal: Decimal,
    pub tax_amount: Decimal,
    pub total: Decimal,
    pub status: QuoteStatus,
    pub sent: Option<SentInfo>,
    pub accepted: Option<AcceptedInfo>,
    pub declined: Option<DeclinedInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct InvoiceFilter {
    pub year: Option<i32>,
    pub client: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QuoteFilter {
    pub year: Option<i32>,
    pub client: Option<String>,
    pub status: Option<String>,
}
