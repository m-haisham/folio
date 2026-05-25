use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum FolioError {
    #[error("client {slug:?} not found (expected {path:?})")]
    ClientNotFound { slug: String, path: PathBuf },

    #[error("invoice {id:?} not found")]
    InvoiceNotFound { id: String },

    #[error("quote {id:?} not found")]
    QuoteNotFound { id: String },

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

    #[error("toml parse error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, FolioError>;
