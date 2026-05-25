/// Integration tests for email sending via SMTP.
///
/// Prerequisites: run `docker-compose up -d` from the workspace root to start
/// Mailpit before executing these tests.
///
/// Run with:
///   cargo test --test email_integration -- --ignored
///
/// The tests are marked `#[ignore]` so they are skipped in normal `cargo test`
/// runs and only execute when you explicitly opt-in with `--ignored`.
use folio_core::{
    email::{EmailMessage, send_email},
    types::{EmailConfig, FolioConfig, MeConfig, SmtpConfig},
};
use serde::Deserialize;
use std::sync::OnceLock;
use tokio::sync::Mutex;

/// Global mutex so the integration tests that share Mailpit run serially.
static MAILPIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn mailpit_mutex() -> &'static Mutex<()> {
    MAILPIT_LOCK.get_or_init(|| Mutex::new(()))
}

// ─── Mailpit HTTP API types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MailpitMessages {
    messages: Vec<MailpitMessage>,
    total: u32,
}

#[derive(Debug, Deserialize)]
struct MailpitMessage {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Subject")]
    subject: String,
    #[serde(rename = "To")]
    to: Vec<MailpitAddress>,
    #[serde(rename = "From")]
    from: MailpitAddress,
    #[serde(rename = "Cc")]
    cc: Option<Vec<MailpitAddress>>,
}

#[derive(Debug, Deserialize)]
struct MailpitAddress {
    #[serde(rename = "Address")]
    address: String,
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Debug, Deserialize)]
struct MailpitMessageDetail {
    #[serde(rename = "Text")]
    text: String,
}

// ─── helpers ────────────────────────────────────────────────────────────────

const MAILPIT_API: &str = "http://localhost:8025/api/v1";
const SMTP_HOST: &str = "localhost";
const SMTP_PORT: u16 = 1025;

/// Build a minimal `FolioConfig` pointing at the local Mailpit SMTP server.
fn mailpit_config() -> FolioConfig {
    FolioConfig {
        me: MeConfig {
            name: "Alice Sender".into(),
            company: Some("Acme Inc".into()),
            email: "alice@example.com".into(),
            address: vec![],
            logo: None,
        },
        defaults: Default::default(),
        invoice: None,
        quote: None,
        email: Some(EmailConfig {
            provider: Some("smtp".into()),
            from: Some("alice@example.com".into()),
            from_name: Some("Alice Sender".into()),
            smtp: Some(SmtpConfig {
                host: SMTP_HOST.into(),
                port: SMTP_PORT,
                username: "test".into(),
                tls: Some(false), // Mailpit uses plain SMTP
            }),
            ..Default::default()
        }),
        build: None,
        paths: None,
    }
}

/// Delete all messages from Mailpit so each test starts with an empty inbox.
async fn clear_mailpit(client: &reqwest::Client) {
    client
        .delete(format!("{}/messages", MAILPIT_API))
        .send()
        .await
        .expect("Failed to clear Mailpit inbox");
}

/// Fetch the current message list from Mailpit.
async fn fetch_messages(client: &reqwest::Client) -> MailpitMessages {
    client
        .get(format!("{}/messages", MAILPIT_API))
        .send()
        .await
        .expect("Failed to query Mailpit")
        .json::<MailpitMessages>()
        .await
        .expect("Failed to parse Mailpit response")
}

/// Fetch the plain-text body of a single message by ID.
async fn fetch_message_detail(client: &reqwest::Client, id: &str) -> MailpitMessageDetail {
    client
        .get(format!("{}/message/{}", MAILPIT_API, id))
        .send()
        .await
        .expect("Failed to fetch message detail")
        .json::<MailpitMessageDetail>()
        .await
        .expect("Failed to parse message detail")
}

// ─── tests ──────────────────────────────────────────────────────────────────

/// Sending a plain-text email lands in Mailpit with the correct metadata.
#[tokio::test]
#[ignore]
async fn test_send_plain_email_arrives_in_mailpit() {
    let _guard = mailpit_mutex().lock().await;
    let http = reqwest::Client::new();
    clear_mailpit(&http).await;

    let config = mailpit_config();
    send_email(
        &config,
        EmailMessage {
            to: "bob@example.com".into(),
            cc: vec![],
            bcc: vec![],
            subject: "Test invoice INV-2026-001".into(),
            body: "Please find your invoice attached.".into(),
            attachment_path: None,
            attachment_name: None,
        },
    )
    .await
    .expect("send_email should succeed");

    let msgs = fetch_messages(&http).await;
    assert_eq!(msgs.total, 1, "Expected exactly one message in Mailpit");

    let msg = &msgs.messages[0];
    assert_eq!(msg.subject, "Test invoice INV-2026-001");
    assert_eq!(msg.to[0].address, "bob@example.com");
    assert_eq!(msg.from.address, "alice@example.com");
    assert_eq!(msg.from.name, "Alice Sender");

    let detail = fetch_message_detail(&http, &msg.id).await;
    assert!(
        detail.text.contains("Please find your invoice attached."),
        "Email body mismatch: {}",
        detail.text
    );
}

/// CC recipients are forwarded correctly.
#[tokio::test]
#[ignore]
async fn test_send_email_with_cc() {
    let _guard = mailpit_mutex().lock().await;
    let http = reqwest::Client::new();
    clear_mailpit(&http).await;

    let config = mailpit_config();
    send_email(
        &config,
        EmailMessage {
            to: "bob@example.com".into(),
            cc: vec!["carol@example.com".into()],
            bcc: vec![],
            subject: "CC test".into(),
            body: "Hello with CC.".into(),
            attachment_path: None,
            attachment_name: None,
        },
    )
    .await
    .expect("send_email with CC should succeed");

    let msgs = fetch_messages(&http).await;
    assert_eq!(msgs.total, 1);

    let msg = &msgs.messages[0];
    let cc = msg.cc.as_deref().unwrap_or(&[]);
    assert!(
        cc.iter().any(|a| a.address == "carol@example.com"),
        "Expected carol@example.com in CC, got {:?}",
        cc
    );
}

/// Sending an email with a PDF attachment is accepted by Mailpit.
#[tokio::test]
#[ignore]
async fn test_send_email_with_pdf_attachment() {
    let _guard = mailpit_mutex().lock().await;
    let http = reqwest::Client::new();
    clear_mailpit(&http).await;

    // Write a minimal (non-valid) PDF placeholder so we don't need headless_chrome.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(tmp.path(), b"%PDF-1.4 fake content for testing").unwrap();

    let config = mailpit_config();
    send_email(
        &config,
        EmailMessage {
            to: "bob@example.com".into(),
            cc: vec![],
            bcc: vec![],
            subject: "Invoice with attachment".into(),
            body: "See attached PDF.".into(),
            attachment_path: Some(tmp.path().to_string_lossy().into_owned()),
            attachment_name: Some("INV-2026-001.pdf".into()),
        },
    )
    .await
    .expect("send_email with attachment should succeed");

    let msgs = fetch_messages(&http).await;
    assert_eq!(msgs.total, 1, "Expected one message");
    assert_eq!(msgs.messages[0].subject, "Invoice with attachment");
}

/// Missing SMTP config returns an error immediately (no network call needed).
#[tokio::test]
#[ignore]
async fn test_send_email_missing_smtp_config_returns_error() {
    use folio_core::types::{Defaults, MeConfig};

    let config = FolioConfig {
        me: MeConfig {
            name: "Alice".into(),
            company: None,
            email: "alice@example.com".into(),
            address: vec![],
            logo: None,
        },
        defaults: Defaults::default(),
        invoice: None,
        quote: None,
        email: Some(EmailConfig {
            provider: Some("smtp".into()),
            smtp: None, // deliberately missing
            ..Default::default()
        }),
        build: None,
        paths: None,
    };

    let result = send_email(
        &config,
        EmailMessage {
            to: "bob@example.com".into(),
            cc: vec![],
            bcc: vec![],
            subject: "Should fail".into(),
            body: "This should not be sent.".into(),
            attachment_path: None,
            attachment_name: None,
        },
    )
    .await;

    assert!(
        result.is_err(),
        "Expected error when SMTP config is missing"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("SMTP config missing"),
        "Unexpected error message: {err}"
    );
}
