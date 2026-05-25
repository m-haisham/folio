use crate::{
    error::{FolioError, Result},
    types::{ComputedInvoice, FolioConfig, MeConfig},
};
use rust_embed::RustEmbed;
use std::{fs, path::Path};
use tera::{Context, Tera};

#[derive(RustEmbed)]
#[folder = "templates/"]
struct BundledTemplates;

pub struct TemplateInfo {
    pub name: String,
    pub description: String,
    pub is_bundled: bool,
    pub path: Option<String>,
}

pub fn list_bundled() -> Vec<TemplateInfo> {
    vec![
        TemplateInfo {
            name: "basic".into(),
            description: "Clean, minimal layout. Black and white, no decoration. (default)".into(),
            is_bundled: true,
            path: None,
        },
        TemplateInfo {
            name: "classic".into(),
            description: "Traditional invoice style with a ruled header and footer.".into(),
            is_bundled: true,
            path: None,
        },
        TemplateInfo {
            name: "modern".into(),
            description: "Bold accent colour, sans-serif, left-aligned logo block.".into(),
            is_bundled: true,
            path: None,
        },
        TemplateInfo {
            name: "floral".into(),
            description: "Decorative botanical accents in the header and footer. Warm tones."
                .into(),
            is_bundled: true,
            path: None,
        },
        TemplateInfo {
            name: "slate".into(),
            description: "Dark header band, light body. Professional and high-contrast.".into(),
            is_bundled: true,
            path: None,
        },
    ]
}

pub fn list_custom(root: &Path) -> Vec<TemplateInfo> {
    let templates_dir = root.join("templates");
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(&templates_dir) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let html_path = entry.path().join("invoice.html");
            if html_path.exists() {
                result.push(TemplateInfo {
                    name: name.clone(),
                    description: String::new(),
                    is_bundled: false,
                    path: Some(html_path.to_string_lossy().to_string()),
                });
            }
        }
    }
    result
}

pub fn get_template_html(name: &str, root: &Path) -> Result<String> {
    // Custom templates take priority over bundled
    let custom_path = root.join("templates").join(name).join("invoice.html");
    if custom_path.exists() {
        return Ok(fs::read_to_string(&custom_path)?);
    }

    // Try bundled
    let key = format!("{}/invoice.html", name);
    if let Some(file) = BundledTemplates::get(&key) {
        return Ok(String::from_utf8_lossy(file.data.as_ref()).to_string());
    }

    Err(FolioError::TemplateNotFound {
        name: name.to_string(),
    })
}

pub fn export_template(name: &str, output: &Path) -> Result<()> {
    let key_html = format!("{}/invoice.html", name);
    let key_toml = format!("{}/template.toml", name);

    fs::create_dir_all(output)?;

    if let Some(file) = BundledTemplates::get(&key_html) {
        fs::write(output.join("invoice.html"), file.data.as_ref())?;
    } else {
        return Err(FolioError::TemplateNotFound {
            name: name.to_string(),
        });
    }

    if let Some(file) = BundledTemplates::get(&key_toml) {
        fs::write(output.join("template.toml"), file.data.as_ref())?;
    }

    Ok(())
}

pub fn render_invoice_html(
    template_html: &str,
    invoice: &ComputedInvoice,
    client: &serde_json::Value,
    me: &MeConfig,
    _config: &FolioConfig,
) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template("invoice.html", template_html)?;

    let mut ctx = Context::new();
    ctx.insert("invoice", invoice);
    ctx.insert("client", client);
    ctx.insert("me", me);

    Ok(tera.render("invoice.html", &ctx)?)
}

pub fn render_email_subject(
    template: &str,
    invoice: &ComputedInvoice,
    me: &MeConfig,
) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template("subject", template)?;
    let mut ctx = Context::new();
    ctx.insert("invoice", invoice);
    ctx.insert("me", me);
    Ok(tera.render("subject", &ctx)?)
}

pub fn render_email_body(
    template: &str,
    invoice: &ComputedInvoice,
    client: &serde_json::Value,
    me: &MeConfig,
) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template("body", template)?;
    let mut ctx = Context::new();
    ctx.insert("invoice", invoice);
    ctx.insert("client", client);
    ctx.insert("me", me);
    Ok(tera.render("body", &ctx)?)
}
