use crate::{
    error::{FolioError, Result},
    types::{Client, Invoice, InvoiceFilter},
};
use async_trait::async_trait;
use std::{fs, path::PathBuf};

#[async_trait]
pub trait InvoiceStore: Send + Sync {
    async fn list(&self, filter: &InvoiceFilter) -> Result<Vec<Invoice>>;
    async fn get(&self, id: &str) -> Result<Invoice>;
    async fn save(&self, invoice: &Invoice) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;

    async fn list_clients(&self) -> Result<Vec<Client>>;
    async fn get_client(&self, slug: &str) -> Result<Client>;
    async fn save_client(&self, client: &Client) -> Result<()>;
}

pub struct FilesystemStore {
    pub root: PathBuf,
}

impl FilesystemStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn invoice_path(&self, id: &str) -> PathBuf {
        // Extract year from ID like INV-2026-001
        let parts: Vec<&str> = id.split('-').collect();
        let year = if parts.len() >= 2 {
            parts[1]
        } else {
            "unknown"
        };
        self.root
            .join("invoices")
            .join(year)
            .join(format!("{}.toml", id))
    }
}

#[async_trait]
impl InvoiceStore for FilesystemStore {
    async fn list(&self, filter: &InvoiceFilter) -> Result<Vec<Invoice>> {
        let invoices_dir = self.root.join("invoices");
        let mut invoices = Vec::new();

        if !invoices_dir.exists() {
            return Ok(invoices);
        }

        let mut year_entries: Vec<_> = fs::read_dir(&invoices_dir)?
            .filter_map(|e| e.ok())
            .collect();
        year_entries.sort_by_key(|e| e.file_name());

        for year_entry in year_entries {
            if !year_entry.file_type()?.is_dir() {
                continue;
            }

            if let Some(year_str) = year_entry.file_name().to_str().map(|s| s.to_string()) {
                if let Ok(year) = year_str.parse::<i32>() {
                    if let Some(filter_year) = filter.year {
                        if year != filter_year {
                            continue;
                        }
                    }
                }
            }

            let mut inv_entries: Vec<_> = fs::read_dir(year_entry.path())?
                .filter_map(|e| e.ok())
                .collect();
            inv_entries.sort_by_key(|e| e.file_name());

            for inv_entry in inv_entries {
                let path = inv_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                let contents = fs::read_to_string(&path)?;
                let invoice: Invoice = toml::from_str(&contents)?;

                if let Some(ref client_filter) = filter.client {
                    if &invoice.client != client_filter {
                        continue;
                    }
                }

                invoices.push(invoice);
            }
        }

        Ok(invoices)
    }

    async fn get(&self, id: &str) -> Result<Invoice> {
        let path = self.invoice_path(id);
        if !path.exists() {
            return Err(FolioError::InvoiceNotFound { id: id.to_string() });
        }
        let contents = fs::read_to_string(&path)?;
        let invoice: Invoice = toml::from_str(&contents)?;
        Ok(invoice)
    }

    async fn save(&self, invoice: &Invoice) -> Result<()> {
        let path = self.invoice_path(&invoice.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(invoice)?;
        fs::write(&path, contents)?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let path = self.invoice_path(id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    async fn list_clients(&self) -> Result<Vec<Client>> {
        let clients_dir = self.root.join("clients");
        let mut clients = Vec::new();

        if !clients_dir.exists() {
            return Ok(clients);
        }

        let mut entries: Vec<_> = fs::read_dir(&clients_dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let slug = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let contents = fs::read_to_string(&path)?;
            let mut client: Client = toml::from_str(&contents)?;
            client.slug = slug;
            clients.push(client);
        }

        Ok(clients)
    }

    async fn get_client(&self, slug: &str) -> Result<Client> {
        let path = self.root.join("clients").join(format!("{}.toml", slug));
        if !path.exists() {
            return Err(FolioError::ClientNotFound {
                slug: slug.to_string(),
                path,
            });
        }
        let contents = fs::read_to_string(&path)?;
        let mut client: Client = toml::from_str(&contents)?;
        client.slug = slug.to_string();
        Ok(client)
    }

    async fn save_client(&self, client: &Client) -> Result<()> {
        let clients_dir = self.root.join("clients");
        fs::create_dir_all(&clients_dir)?;
        let path = clients_dir.join(format!("{}.toml", client.slug));
        let contents = toml::to_string_pretty(client)?;
        fs::write(path, contents)?;
        Ok(())
    }
}
