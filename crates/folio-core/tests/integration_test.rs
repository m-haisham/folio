use chrono::NaiveDate;
use folio_core::{
    compute::{compute_invoice, derive_status},
    index::{BuildIndex, PdfState, check_pdf_state, compute_source_hash},
    store::{FilesystemStore, InvoiceStore},
    types::{
        Client, Defaults, FolioConfig, Invoice, InvoiceStatus, LineItem, MeConfig, PaidInfo,
        PathsConfig, SentInfo, VoidedInfo,
    },
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tempfile::TempDir;

// ─── helpers ───────────────────────────────────────────────────────────────

fn make_config() -> FolioConfig {
    FolioConfig {
        me: MeConfig {
            name: "Alice".into(),
            company: Some("Acme".into()),
            email: "alice@example.com".into(),
            address: vec!["1 Main St".into()],
            logo: None,
        },
        defaults: Defaults {
            currency: Some("USD".into()),
            tax_rate: Some(Decimal::from_str("10").unwrap()),
            due_days: Some(30),
            template: Some("basic".into()),
            id_format: None,
            primary_color: None,
            notes: None,
        },
        email: None,
        build: None,
        paths: None,
    }
}

fn make_client() -> Client {
    Client {
        name: "Bob Corp".into(),
        contact: Some("Bob".into()),
        email: "bob@example.com".into(),
        address: vec!["2 Client Ave".into()],
        currency: None,
        due_days: None,
        template: None,
        email_opts: None,
        notes: None,
        defaults: None,
        slug: "bob-corp".into(),
    }
}

fn make_invoice(id: &str, date: NaiveDate) -> Invoice {
    Invoice {
        id: id.into(),
        client: "bob-corp".into(),
        date,
        due: None,
        currency: None,
        template: None,
        primary_color: None,
        tax_rate: None,
        notes: None,
        items: vec![LineItem {
            description: "Dev work".into(),
            quantity: Decimal::from(10),
            unit: Some("hours".into()),
            rate: Decimal::from(100),
        }],
        sent: None,
        paid: None,
        voided: None,
    }
}

// ─── compute_invoice ───────────────────────────────────────────────────────

#[test]
fn test_compute_totals() {
    let config = make_config();
    let client = make_client();
    let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    let invoice = make_invoice("INV-2026-001", date);

    let computed = compute_invoice(&invoice, &client, &config);

    // 10 hours * $100 = $1000 subtotal
    assert_eq!(computed.subtotal, Decimal::from(1000));
    // 10% tax = $100
    assert_eq!(computed.tax_amount, Decimal::from(100));
    // total = $1100
    assert_eq!(computed.total, Decimal::from(1100));
    // no payment yet => outstanding == total
    assert_eq!(computed.outstanding, Decimal::from(1100));
    // due = date + 30 days
    assert_eq!(computed.due, NaiveDate::from_ymd_opt(2026, 2, 14).unwrap());
    assert_eq!(computed.currency, "USD");
    assert_eq!(computed.template, "basic");
}

#[test]
fn test_compute_zero_tax() {
    let mut config = make_config();
    config.defaults.tax_rate = Some(Decimal::ZERO);
    let client = make_client();
    let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let invoice = make_invoice("INV-2026-001", date);

    let computed = compute_invoice(&invoice, &client, &config);
    assert_eq!(computed.tax_amount, Decimal::ZERO);
    assert_eq!(computed.total, computed.subtotal);
}

// ─── status derivation ─────────────────────────────────────────────────────

#[test]
fn test_status_draft() {
    let invoice = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let due = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    let status = derive_status(&invoice, Decimal::from(1000), &due, &today);
    assert_eq!(status, InvoiceStatus::Draft);
}

#[test]
fn test_status_sent() {
    let mut invoice = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    invoice.sent = Some(SentInfo {
        at: chrono::Utc::now(),
        method: "email".into(),
        to: "bob@example.com".into(),
        cc: None,
    });
    let due = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    let status = derive_status(&invoice, Decimal::from(1000), &due, &today);
    assert_eq!(status, InvoiceStatus::Sent);
}

#[test]
fn test_status_overdue() {
    let mut invoice = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    invoice.sent = Some(SentInfo {
        at: chrono::Utc::now(),
        method: "email".into(),
        to: "bob@example.com".into(),
        cc: None,
    });
    let due = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    let status = derive_status(&invoice, Decimal::from(1000), &due, &today);
    assert_eq!(status, InvoiceStatus::Overdue);
}

#[test]
fn test_status_paid() {
    let mut invoice = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    invoice.paid = Some(PaidInfo {
        at: NaiveDate::from_ymd_opt(2026, 1, 20).unwrap(),
        amount: Decimal::from(1000),
        method: None,
        reference: None,
    });
    let due = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 1, 25).unwrap();
    let status = derive_status(&invoice, Decimal::from(1000), &due, &today);
    assert_eq!(status, InvoiceStatus::Paid);
}

#[test]
fn test_status_partially_paid() {
    let mut invoice = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    invoice.paid = Some(PaidInfo {
        at: NaiveDate::from_ymd_opt(2026, 1, 20).unwrap(),
        amount: Decimal::from(500),
        method: None,
        reference: None,
    });
    let due = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 1, 25).unwrap();
    let status = derive_status(&invoice, Decimal::from(1000), &due, &today);
    assert_eq!(status, InvoiceStatus::PartiallyPaid);
}

#[test]
fn test_status_voided() {
    let mut invoice = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    invoice.voided = Some(VoidedInfo {
        at: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
        reason: Some("error".into()),
    });
    let due = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 1, 25).unwrap();
    let status = derive_status(&invoice, Decimal::from(1000), &due, &today);
    assert_eq!(status, InvoiceStatus::Voided);
}

// ─── FilesystemStore CRUD ──────────────────────────────────────────────────

#[tokio::test]
async fn test_store_invoice_crud() {
    let dir = TempDir::new().unwrap();
    let store = FilesystemStore::new(dir.path());

    let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
    let invoice = make_invoice("INV-2026-001", date);

    // Save
    store.save(&invoice).await.unwrap();

    // Get
    let fetched = store.get("INV-2026-001").await.unwrap();
    assert_eq!(fetched.id, "INV-2026-001");
    assert_eq!(fetched.client, "bob-corp");

    // List
    let filter = folio_core::types::InvoiceFilter::default();
    let list = store.list(&filter).await.unwrap();
    assert_eq!(list.len(), 1);

    // Delete
    store.delete("INV-2026-001").await.unwrap();
    let list_after = store.list(&filter).await.unwrap();
    assert!(list_after.is_empty());
}

#[tokio::test]
async fn test_store_invoice_not_found() {
    let dir = TempDir::new().unwrap();
    let store = FilesystemStore::new(dir.path());
    let result = store.get("INV-2026-999").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_store_client_crud() {
    let dir = TempDir::new().unwrap();
    let store = FilesystemStore::new(dir.path());
    let client = make_client();

    store.save_client(&client).await.unwrap();

    let fetched = store.get_client("bob-corp").await.unwrap();
    assert_eq!(fetched.name, "Bob Corp");
    assert_eq!(fetched.slug, "bob-corp");

    let clients = store.list_clients().await.unwrap();
    assert_eq!(clients.len(), 1);
}

#[tokio::test]
async fn test_store_filter_by_year() {
    let dir = TempDir::new().unwrap();
    let store = FilesystemStore::new(dir.path());

    let inv_2025 = make_invoice("INV-2025-001", NaiveDate::from_ymd_opt(2025, 6, 1).unwrap());
    let inv_2026 = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());

    store.save(&inv_2025).await.unwrap();
    store.save(&inv_2026).await.unwrap();

    let filter_2026 = folio_core::types::InvoiceFilter {
        year: Some(2026),
        ..Default::default()
    };
    let results = store.list(&filter_2026).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "INV-2026-001");
}

#[tokio::test]
async fn test_store_filter_by_client() {
    let dir = TempDir::new().unwrap();
    let store = FilesystemStore::new(dir.path());

    let mut inv1 = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let mut inv2 = make_invoice("INV-2026-002", NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
    inv2.client = "other-client".into();

    store.save(&inv1).await.unwrap();
    store.save(&inv2).await.unwrap();

    let filter = folio_core::types::InvoiceFilter {
        client: Some("bob-corp".into()),
        ..Default::default()
    };
    let results = store.list(&filter).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].client, "bob-corp");
}

// ─── source hash ───────────────────────────────────────────────────────────

#[test]
fn test_source_hash_deterministic() {
    let h1 = compute_source_hash("invoice", "client", "template", "me");
    let h2 = compute_source_hash("invoice", "client", "template", "me");
    assert_eq!(h1, h2);
}

#[test]
fn test_source_hash_changes_on_diff_input() {
    let h1 = compute_source_hash("invoice-v1", "client", "template", "me");
    let h2 = compute_source_hash("invoice-v2", "client", "template", "me");
    assert_ne!(h1, h2);
}

#[test]
fn test_source_hash_is_8_hex_chars() {
    let h = compute_source_hash("a", "b", "c", "d");
    assert_eq!(h.len(), 8);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
}

// ─── BuildIndex staleness ──────────────────────────────────────────────────

#[test]
fn test_pdf_state_never_built() {
    let index = BuildIndex::default();
    assert_eq!(
        check_pdf_state(&index, "INV-2026-001", "abc12345"),
        PdfState::NeverBuilt
    );
}

#[test]
fn test_pdf_state_fresh() {
    let mut index = BuildIndex::default();
    index.record("INV-2026-001", "abc12345");
    assert_eq!(
        check_pdf_state(&index, "INV-2026-001", "abc12345"),
        PdfState::Fresh
    );
}

#[test]
fn test_pdf_state_stale() {
    let mut index = BuildIndex::default();
    index.record("INV-2026-001", "abc12345");
    assert_eq!(
        check_pdf_state(&index, "INV-2026-001", "deadbeef"),
        PdfState::Stale
    );
}

#[test]
fn test_build_index_save_load() {
    let dir = TempDir::new().unwrap();
    let mut index = BuildIndex::default();
    index.record("INV-2026-001", "abc12345");
    index.save(dir.path()).unwrap();

    let loaded = BuildIndex::load(dir.path()).unwrap();
    assert_eq!(
        check_pdf_state(&loaded, "INV-2026-001", "abc12345"),
        PdfState::Fresh
    );
}

// ─── PathsConfig ───────────────────────────────────────────────────────────

#[test]
fn test_paths_config_defaults() {
    let p = PathsConfig::default();
    assert_eq!(p.clients(), "clients");
    assert_eq!(p.invoices(), "invoices");
    assert_eq!(p.templates(), "templates");
    assert_eq!(p.output(), "output");
}

#[test]
fn test_paths_config_overrides() {
    let p = PathsConfig {
        clients: Some("billing/clients".into()),
        invoices: Some("billing/invoices".into()),
        templates: Some("billing/templates".into()),
        output: Some("billing/output".into()),
    };
    assert_eq!(p.clients(), "billing/clients");
    assert_eq!(p.invoices(), "billing/invoices");
    assert_eq!(p.templates(), "billing/templates");
    assert_eq!(p.output(), "billing/output");
}

#[test]
fn test_paths_config_partial_override() {
    // Only some paths are overridden; the rest fall back to defaults.
    let p = PathsConfig {
        clients: Some("billing/clients".into()),
        invoices: None,
        templates: None,
        output: Some("dist".into()),
    };
    assert_eq!(p.clients(), "billing/clients");
    assert_eq!(p.invoices(), "invoices"); // default
    assert_eq!(p.templates(), "templates"); // default
    assert_eq!(p.output(), "dist");
}

/// Store created with `with_paths` reads and writes to the configured directories.
#[tokio::test]
async fn test_store_with_custom_paths() {
    let dir = TempDir::new().unwrap();

    let paths = PathsConfig {
        clients: Some("billing/clients".into()),
        invoices: Some("billing/invoices".into()),
        templates: None,
        output: Some("billing/output".into()),
    };
    let store = FilesystemStore::with_paths(dir.path(), paths);

    // Clients dir should be billing/clients, not clients/
    let client = Client {
        name: "Test Co".into(),
        contact: None,
        email: "test@test.com".into(),
        address: vec![],
        currency: None,
        due_days: None,
        template: None,
        email_opts: None,
        notes: None,
        defaults: None,
        slug: "test-co".into(),
    };
    store.save_client(&client).await.unwrap();

    // File should exist at the custom path
    assert!(dir.path().join("billing/clients/test-co.toml").exists());
    // And NOT at the default path
    assert!(!dir.path().join("clients/test-co.toml").exists());

    let fetched = store.get_client("test-co").await.unwrap();
    assert_eq!(fetched.name, "Test Co");

    // Invoice dir should be billing/invoices
    let inv = make_invoice("INV-2026-001", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    store.save(&inv).await.unwrap();
    assert!(
        dir.path()
            .join("billing/invoices/2026/INV-2026-001.toml")
            .exists()
    );
    assert!(!dir.path().join("invoices/2026/INV-2026-001.toml").exists());

    let loaded = store.get("INV-2026-001").await.unwrap();
    assert_eq!(loaded.id, "INV-2026-001");
}

/// `store.output_dir()` and `store.templates_dir()` reflect the configured paths.
#[test]
fn test_store_path_helpers() {
    let paths = PathsConfig {
        clients: None,
        invoices: None,
        templates: Some("my/templates".into()),
        output: Some("dist".into()),
    };
    let store = FilesystemStore::with_paths("/repo", paths);
    assert_eq!(store.output_dir(), std::path::Path::new("/repo/dist"));
    assert_eq!(
        store.templates_dir(),
        std::path::Path::new("/repo/my/templates")
    );
}

/// `FolioConfig::paths()` returns sensible defaults when `[paths]` is absent.
#[test]
fn test_folio_config_paths_absent() {
    let config = make_config(); // paths: None
    assert_eq!(config.paths().clients(), "clients");
    assert_eq!(config.paths().output(), "output");
}
