use crate::error::{FolioError, Result};
use std::path::Path;

// A4 dimensions in inches (210mm × 297mm)
const A4_WIDTH_IN: f64 = 8.27;
const A4_HEIGHT_IN: f64 = 11.69;

/// Page margins (in inches) passed to Chrome's PrintToPdf API.
///
/// Use `PdfMargins::none()` for templates that manage their own layout via
/// internal CSS padding (invoices, quotes). Use `PdfMargins::document()` for
/// Markdown documents where Chrome should provide standard whitespace margins.
#[derive(Debug, Clone, Copy)]
pub struct PdfMargins {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

impl PdfMargins {
    /// No margins — template owns all spacing (invoices, quotes).
    pub fn none() -> Self {
        Self {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
        }
    }

    /// Standard document margins: 0.75" top/bottom, 0.9" left/right.
    pub fn document() -> Self {
        Self {
            top: 0.7,
            bottom: 0.2,
            left: 0.8,
            right: 0.8,
        }
    }
}

pub fn html_to_pdf(html: &str, output_path: &Path, margins: PdfMargins) -> Result<()> {
    use headless_chrome::{Browser, LaunchOptions, types::PrintToPdfOptions};
    use std::fs;

    // Write HTML to a temp file so Chrome can load it via file://
    let tmp = tempfile::Builder::new()
        .suffix(".html")
        .tempfile()
        .map_err(|e| FolioError::Io(e.into()))?;
    fs::write(tmp.path(), html)?;

    let chrome_path = std::env::var("FOLIO_CHROME_PATH").ok();

    let mut builder = LaunchOptions::default_builder();
    if let Some(ref path) = chrome_path {
        builder.path(Some(std::path::PathBuf::from(path)));
    }
    let options = builder
        .build()
        .map_err(|e| FolioError::Other(e.to_string()))?;

    let browser = Browser::new(options).map_err(|e| FolioError::Other(e.to_string()))?;
    let tab = browser
        .new_tab()
        .map_err(|e| FolioError::Other(e.to_string()))?;

    let url = format!("file://{}", tmp.path().display());
    tab.navigate_to(&url)
        .map_err(|e| FolioError::Other(e.to_string()))?;
    tab.wait_until_navigated()
        .map_err(|e| FolioError::Other(e.to_string()))?;

    let pdf_options = PrintToPdfOptions {
        paper_width: Some(A4_WIDTH_IN),
        paper_height: Some(A4_HEIGHT_IN),
        print_background: Some(true),
        margin_top: Some(margins.top),
        margin_bottom: Some(margins.bottom),
        margin_left: Some(margins.left),
        margin_right: Some(margins.right),
        ..Default::default()
    };

    let pdf_bytes = tab
        .print_to_pdf(Some(pdf_options))
        .map_err(|e| FolioError::Other(e.to_string()))?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, pdf_bytes)?;

    Ok(())
}
