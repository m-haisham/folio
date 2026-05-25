use crate::error::{FolioError, Result};
use std::path::Path;

pub fn html_to_pdf(html: &str, output_path: &Path) -> Result<()> {
    use headless_chrome::{Browser, LaunchOptions};
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

    let pdf_bytes = tab
        .print_to_pdf(None)
        .map_err(|e| FolioError::Other(e.to_string()))?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, pdf_bytes)?;

    Ok(())
}
