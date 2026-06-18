use crate::{
    error::{FolioError, Result},
    markdown::RenderedDoc,
    types::{ComputedInvoice, ComputedQuote, FolioConfig, MeConfig},
};
use rust_embed::RustEmbed;
use std::{fs, path::Path};
use tera::{Context, Tera};

#[derive(RustEmbed)]
#[folder = "templates/"]
struct BundledTemplates;

/// A minimal colour palette derived from a single primary hex colour.
/// All values are ready-to-use CSS hex strings ("#rrggbb").
#[derive(serde::Serialize)]
struct ThemeContext {
    /// The primary/accent colour itself, e.g. "#7c3aed".
    primary: String,
    /// A very light tint (~90% white mixed), for card backgrounds.
    primary_light: String,
    /// A slightly lighter variant (~15% lighter), for hover or borders.
    primary_mid: String,
    /// A darkened variant (~20% darker), for footer bands etc.
    primary_dark: String,
    /// The primary colour at low opacity for transparent overlays (rgba string).
    primary_alpha_low: String,
    primary_alpha_very_low: String,
}

/// Parse a CSS hex color (#rrggbb or #rgb) into (r, g, b) u8 components.
fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim_start_matches('#');
    match h.len() {
        6 => {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
            Some((r, g, b))
        }
        3 => {
            let r = u8::from_str_radix(&h[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&h[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&h[2..3].repeat(2), 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Blend a colour toward white by `amount` (0.0 = original, 1.0 = white).
fn lighten(r: u8, g: u8, b: u8, amount: f32) -> (u8, u8, u8) {
    let blend = |c: u8| -> u8 { (c as f32 + (255.0 - c as f32) * amount).round() as u8 };
    (blend(r), blend(g), blend(b))
}

/// Blend a colour toward black by `amount` (0.0 = original, 1.0 = black).
fn darken(r: u8, g: u8, b: u8, amount: f32) -> (u8, u8, u8) {
    let blend = |c: u8| -> u8 { (c as f32 * (1.0 - amount)).round() as u8 };
    (blend(r), blend(g), blend(b))
}

fn build_theme(primary_hex: &str) -> Option<ThemeContext> {
    let (r, g, b) = parse_hex(primary_hex)?;
    let (lr, lg, lb) = lighten(r, g, b, 0.88);
    let (mr, mg, mb) = lighten(r, g, b, 0.20);
    let (dr, dg, db) = darken(r, g, b, 0.22);
    Some(ThemeContext {
        primary: to_hex(r, g, b),
        primary_light: to_hex(lr, lg, lb),
        primary_mid: to_hex(mr, mg, mb),
        primary_dark: to_hex(dr, dg, db),
        primary_alpha_low: format!("rgba({},{},{},0.06)", r, g, b),
        primary_alpha_very_low: format!("rgba({},{},{},0.04)", r, g, b),
    })
}

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
        TemplateInfo {
            name: "signature".into(),
            description: "Editorial serif design. Cream paper, forest-green ink, italic accents."
                .into(),
            is_bundled: true,
            path: None,
        },
    ]
}

/// List custom templates found under `templates_dir`.
///
/// `templates_dir` should be the resolved path (i.e. `root.join(paths.templates())`).
pub fn list_custom(templates_dir: &Path) -> Vec<TemplateInfo> {
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(&templates_dir) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let doc_path = entry.path().join("document.html");
            let inv_path = entry.path().join("invoice.html");
            let html_path = if doc_path.exists() {
                doc_path
            } else {
                inv_path.clone()
            };
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

/// Return the HTML source for a named template.
///
/// Custom templates (from `templates_dir`) take priority over bundled ones.
/// `templates_dir` should be the resolved path (i.e. `root.join(paths.templates())`).
pub fn get_template_html(name: &str, templates_dir: &Path) -> Result<String> {
    // Custom templates: document.html takes priority, invoice.html for legacy compat
    let custom_doc = templates_dir.join(name).join("document.html");
    if custom_doc.exists() {
        return Ok(fs::read_to_string(&custom_doc)?);
    }
    let custom_inv = templates_dir.join(name).join("invoice.html");
    if custom_inv.exists() {
        return Ok(fs::read_to_string(&custom_inv)?);
    }

    // Bundled: document.html, then invoice.html for legacy
    let key = format!("{}/document.html", name);
    if let Some(file) = BundledTemplates::get(&key) {
        return Ok(String::from_utf8_lossy(file.data.as_ref()).to_string());
    }
    let key_legacy = format!("{}/invoice.html", name);
    if let Some(file) = BundledTemplates::get(&key_legacy) {
        return Ok(String::from_utf8_lossy(file.data.as_ref()).to_string());
    }

    Err(FolioError::TemplateNotFound {
        name: name.to_string(),
    })
}

/// Return the email body template for a named template, or `None` if not found.
///
/// Checks custom templates first, then bundled ones. Returns `None` when absent
/// so callers can fall back to `[email.templates].body` from config.
pub fn get_email_template(name: &str, templates_dir: &Path) -> Option<String> {
    // Custom template takes priority
    let custom_path = templates_dir.join(name).join("email.html");
    if custom_path.exists() {
        return fs::read_to_string(&custom_path).ok();
    }

    // Try bundled
    let key = format!("{}/email.html", name);
    BundledTemplates::get(&key).map(|f| String::from_utf8_lossy(f.data.as_ref()).to_string())
}

/// Return the `doc.html` source for a named template, used for Markdown document rendering.
///
/// Falls back to `basic/doc.html` if the requested template has no `doc.html`.
/// Custom templates (from `templates_dir`) take priority over bundled ones.
pub fn get_doc_template_html(name: &str, templates_dir: &Path) -> Result<String> {
    // Custom template first
    let custom = templates_dir.join(name).join("doc.html");
    if custom.exists() {
        return Ok(fs::read_to_string(&custom)?);
    }

    // Bundled
    let key = format!("{}/doc.html", name);
    if let Some(file) = BundledTemplates::get(&key) {
        return Ok(String::from_utf8_lossy(file.data.as_ref()).to_string());
    }

    // Fallback to basic
    if name != "basic" {
        return get_doc_template_html("basic", templates_dir);
    }

    Err(FolioError::TemplateNotFound {
        name: format!("{}/doc.html", name),
    })
}

pub fn export_template(name: &str, output: &Path) -> Result<()> {
    fs::create_dir_all(output)?;

    // Export document.html
    let key_html = format!("{}/document.html", name);
    if let Some(file) = BundledTemplates::get(&key_html) {
        fs::write(output.join("document.html"), file.data.as_ref())?;
    } else {
        return Err(FolioError::TemplateNotFound {
            name: name.to_string(),
        });
    }

    // Export email.html if present
    let key_email = format!("{}/email.html", name);
    if let Some(file) = BundledTemplates::get(&key_email) {
        fs::write(output.join("email.html"), file.data.as_ref())?;
    }

    // Export template.toml if present
    let key_toml = format!("{}/template.toml", name);
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
    ctx.insert("document_type", "Invoice");
    ctx.insert("due_label", "Due");

    if let Some(ref color) = invoice.primary_color {
        if let Some(theme) = build_theme(color) {
            ctx.insert("theme", &theme);
        }
    }

    Ok(tera.render("invoice.html", &ctx)?)
}

pub fn render_quote_html(
    template_html: &str,
    quote: &ComputedQuote,
    client: &serde_json::Value,
    me: &MeConfig,
    _config: &FolioConfig,
) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template("invoice.html", template_html)?;

    // Build an invoice-compatible JSON value from the quote so existing
    // templates work unchanged (they use {{ invoice.* }} everywhere).
    let mut inv_ctx = serde_json::to_value(quote).map_err(|e| FolioError::Other(e.to_string()))?;

    if let Some(obj) = inv_ctx.as_object_mut() {
        // expires → due (templates use invoice.due)
        if let Some(expires) = obj.remove("expires") {
            obj.insert("due".to_string(), expires);
        }
        // Null out invoice-only fields that quotes don't have
        obj.insert("paid".to_string(), serde_json::Value::Null);
        obj.insert("voided".to_string(), serde_json::Value::Null);
        // outstanding = total (no partial payments on quotes)
        let total = obj.get("total").cloned().unwrap_or(serde_json::Value::Null);
        obj.insert("outstanding".to_string(), total);
    }

    let mut ctx = Context::new();
    ctx.insert("invoice", &inv_ctx);
    ctx.insert("client", client);
    ctx.insert("me", me);
    ctx.insert("document_type", "Quote");
    ctx.insert("due_label", "Expires");

    if let Some(ref color) = quote.primary_color {
        if let Some(theme) = build_theme(color) {
            ctx.insert("theme", &theme);
        }
    }

    Ok(tera.render("invoice.html", &ctx)?)
}

pub fn render_email_subject(
    template: &str,
    invoice: &serde_json::Value,
    me: &MeConfig,
    document_type: &str,
) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template("subject", template)?;
    let mut ctx = Context::new();
    ctx.insert("invoice", invoice);
    ctx.insert("me", me);
    ctx.insert("document_type", document_type);
    Ok(tera.render("subject", &ctx)?)
}

pub fn render_email_body(
    template: &str,
    invoice: &serde_json::Value,
    client: &serde_json::Value,
    me: &MeConfig,
    document_type: &str,
) -> Result<String> {
    let mut tera = Tera::default();
    // Use a .html extension so Tera applies HTML auto-escaping.
    tera.add_raw_template("body.html", template)?;
    let mut ctx = Context::new();
    ctx.insert("invoice", invoice);
    ctx.insert("client", client);
    ctx.insert("me", me);
    ctx.insert("document_type", document_type);
    // Inject theme palette if primary_color is set, matching render_invoice_html.
    if let Some(color) = invoice.get("primary_color").and_then(|v| v.as_str()) {
        if let Some(theme) = build_theme(color) {
            ctx.insert("theme", &theme);
        }
    }
    Ok(tera.render("body.html", &ctx)?)
}

/// Render a Markdown-sourced document to HTML using a `doc.html` template.
///
/// The Tera context exposes: `title`, `body`, `date`, `author`, `me`, `theme`.
pub fn render_doc_html(
    template_html: &str,
    doc: &RenderedDoc,
    me: &MeConfig,
    primary_color: Option<&str>,
) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template("doc.html", template_html)?;

    let mut ctx = Context::new();
    ctx.insert("title", &doc.title);
    ctx.insert("body", &doc.body_html);
    if let Some(ref date) = doc.date {
        ctx.insert("date", date);
    }
    if let Some(ref author) = doc.author {
        ctx.insert("author", author);
    }
    ctx.insert("me", me);

    // Resolve primary color: doc frontmatter > caller override > nothing
    let color = doc.primary_color.as_deref().or(primary_color);
    if let Some(color) = color {
        if let Some(theme) = build_theme(color) {
            ctx.insert("theme", &theme);
        }
    }

    Ok(tera.render("doc.html", &ctx)?)
}
