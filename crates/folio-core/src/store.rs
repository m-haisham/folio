use crate::{
    error::{FolioError, Result},
    types::{Client, Invoice, InvoiceFilter, PathsConfig, Quote, QuoteFilter},
};
use async_trait::async_trait;
use std::{fs, path::PathBuf};

#[async_trait]
pub trait ClientStore: Send + Sync {
    async fn list_clients(&self) -> Result<Vec<Client>>;
    async fn get_client(&self, slug: &str) -> Result<Client>;
    async fn save_client(&self, client: &Client) -> Result<()>;
}

#[async_trait]
pub trait InvoiceStore: Send + Sync {
    async fn list(&self, filter: &InvoiceFilter) -> Result<Vec<Invoice>>;
    async fn get(&self, id: &str) -> Result<Invoice>;
    async fn save(&self, invoice: &Invoice) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;
}

#[async_trait]
pub trait QuoteStore: Send + Sync {
    async fn list_quotes(&self, filter: &QuoteFilter) -> Result<Vec<Quote>>;
    async fn get_quote(&self, id: &str) -> Result<Quote>;
    async fn save_quote(&self, quote: &Quote) -> Result<()>;
    async fn delete_quote(&self, id: &str) -> Result<()>;
}

/// Filesystem-backed store that reads and writes TOML files under `root`.
///
/// Directory names are resolved through `paths`, which mirrors the `[paths]`
/// section of `folio.toml`. Pass `PathsConfig::default()` (or simply use
/// `FilesystemStore::new`) to get the conventional layout.
pub struct FilesystemStore {
    pub root: PathBuf,
    pub paths: PathsConfig,
}

fn id_year(id: &str) -> &str {
    id.split('-').nth(1).unwrap_or("unknown")
}

impl FilesystemStore {
    /// Create a store using the conventional directory layout
    /// (`clients/`, `invoices/`, `templates/`, `output/`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            paths: PathsConfig::default(),
        }
    }

    /// Create a store with custom directory paths taken from `folio.toml`.
    pub fn with_paths(root: impl Into<PathBuf>, paths: PathsConfig) -> Self {
        Self {
            root: root.into(),
            paths,
        }
    }

    /// Absolute path to an invoice TOML, e.g. `{invoices}/{year}/{id}.toml`.
    pub fn invoice_path(&self, id: &str) -> PathBuf {
        self.root
            .join(self.paths.invoices())
            .join(id_year(id))
            .join(format!("{}.toml", id))
    }

    /// Absolute path to the clients directory.
    pub fn clients_dir(&self) -> PathBuf {
        self.root.join(self.paths.clients())
    }

    /// Absolute path to the invoices directory.
    pub fn invoices_dir(&self) -> PathBuf {
        self.root.join(self.paths.invoices())
    }

    /// Absolute path to the output directory.
    pub fn output_dir(&self) -> PathBuf {
        self.root.join(self.paths.output())
    }

    /// Absolute path to the custom templates directory.
    pub fn templates_dir(&self) -> PathBuf {
        self.root.join(self.paths.templates())
    }

    /// Absolute path to a quote TOML, e.g. `{quotes}/{year}/{id}.toml`.
    pub fn quote_path(&self, id: &str) -> PathBuf {
        self.root
            .join(self.paths.quotes())
            .join(id_year(id))
            .join(format!("{}.toml", id))
    }

    /// Absolute path to the quotes directory.
    pub fn quotes_dir(&self) -> PathBuf {
        self.root.join(self.paths.quotes())
    }
}

#[async_trait]
impl InvoiceStore for FilesystemStore {
    async fn list(&self, filter: &InvoiceFilter) -> Result<Vec<Invoice>> {
        let invoices_dir = self.invoices_dir();
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
}

#[async_trait]
impl ClientStore for FilesystemStore {
    async fn list_clients(&self) -> Result<Vec<Client>> {
        let clients_dir = self.clients_dir();
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
        let path = self.clients_dir().join(format!("{}.toml", slug));
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
        let clients_dir = self.clients_dir();
        fs::create_dir_all(&clients_dir)?;
        let path = clients_dir.join(format!("{}.toml", client.slug));
        let contents = toml::to_string_pretty(client)?;
        fs::write(path, contents)?;
        Ok(())
    }
}

#[async_trait]
impl QuoteStore for FilesystemStore {
    async fn list_quotes(&self, filter: &QuoteFilter) -> Result<Vec<Quote>> {
        let quotes_dir = self.quotes_dir();
        let mut quotes = Vec::new();

        if !quotes_dir.exists() {
            return Ok(quotes);
        }

        let mut year_entries: Vec<_> = fs::read_dir(&quotes_dir)?.filter_map(|e| e.ok()).collect();
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

            let mut q_entries: Vec<_> = fs::read_dir(year_entry.path())?
                .filter_map(|e| e.ok())
                .collect();
            q_entries.sort_by_key(|e| e.file_name());

            for q_entry in q_entries {
                let path = q_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                let contents = fs::read_to_string(&path)?;
                let quote: Quote = toml::from_str(&contents)?;

                if let Some(ref client_filter) = filter.client {
                    if &quote.client != client_filter {
                        continue;
                    }
                }

                quotes.push(quote);
            }
        }

        Ok(quotes)
    }

    async fn get_quote(&self, id: &str) -> Result<Quote> {
        let path = self.quote_path(id);
        if !path.exists() {
            return Err(FolioError::QuoteNotFound { id: id.to_string() });
        }
        let contents = fs::read_to_string(&path)?;
        let quote: Quote = toml::from_str(&contents)?;
        Ok(quote)
    }

    async fn save_quote(&self, quote: &Quote) -> Result<()> {
        let path = self.quote_path(&quote.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(quote)?;
        fs::write(&path, contents)?;
        Ok(())
    }

    async fn delete_quote(&self, id: &str) -> Result<()> {
        let path = self.quote_path(id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
