use crate::{error::Result, types::FolioConfig};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn find_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join("folio.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub fn load_config(root: &Path) -> Result<FolioConfig> {
    if let Ok(env_path) = std::env::var("FOLIO_CONFIG") {
        let contents = fs::read_to_string(&env_path)?;
        return Ok(toml::from_str(&contents)?);
    }
    let path = root.join("folio.toml");
    let contents = fs::read_to_string(&path)?;
    Ok(toml::from_str(&contents)?)
}
