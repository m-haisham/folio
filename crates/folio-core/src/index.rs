use crate::{error::Result, types::FolioConfig};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildEntry {
    pub built_at: DateTime<Utc>,
    pub source_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildIndex {
    #[serde(default)]
    pub builds: HashMap<String, BuildEntry>,
}

impl BuildIndex {
    pub fn load(root: &Path) -> Result<Self> {
        let path = index_path(root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(&path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let dir = root.join(".folio");
        fs::create_dir_all(&dir)?;
        let path = dir.join("index.toml");
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn record(&mut self, id: &str, hash: &str) {
        self.builds.insert(
            id.to_string(),
            BuildEntry {
                built_at: Utc::now(),
                source_hash: hash.to_string(),
            },
        );
    }
}

fn index_path(root: &Path) -> PathBuf {
    root.join(".folio").join("index.toml")
}

pub fn compute_source_hash(
    document_toml: &str,
    client_toml: &str,
    template_html: &str,
    config: &FolioConfig,
) -> String {
    // Serialize only the sections that affect rendered output.
    // [email], [build], and [paths] are intentionally excluded.
    let me_toml = toml::to_string(&config.me).unwrap_or_default();
    let defaults_toml = toml::to_string(&config.defaults).unwrap_or_default();
    let invoice_defaults_toml = toml::to_string(&config.invoice).unwrap_or_default();
    let quote_defaults_toml = toml::to_string(&config.quote).unwrap_or_default();

    let mut hasher = Sha256::new();
    hasher.update(document_toml.as_bytes());
    hasher.update(client_toml.as_bytes());
    hasher.update(template_html.as_bytes());
    hasher.update(me_toml.as_bytes());
    hasher.update(defaults_toml.as_bytes());
    hasher.update(invoice_defaults_toml.as_bytes());
    hasher.update(quote_defaults_toml.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..4])
}

#[derive(Debug, Clone, PartialEq)]
pub enum PdfState {
    Fresh,
    Stale,
    NeverBuilt,
}

pub fn check_pdf_state(index: &BuildIndex, id: &str, current_hash: &str) -> PdfState {
    match index.builds.get(id) {
        None => PdfState::NeverBuilt,
        Some(entry) if entry.source_hash == current_hash => PdfState::Fresh,
        Some(_) => PdfState::Stale,
    }
}
