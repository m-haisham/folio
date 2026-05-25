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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildConfig {
    pub chrome_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FolioConfig {
    pub me: MeConfig,
    pub defaults: Defaults,
    pub email: Option<EmailConfig>,
    pub build: Option<BuildConfig>,
}

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
    /// Not stored in file — set after loading
    #[serde(skip)]
    pub slug: String,
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

#[derive(Debug, Clone, Default)]
pub struct InvoiceFilter {
    pub year: Option<i32>,
    pub client: Option<String>,
    pub status: Option<String>,
}
