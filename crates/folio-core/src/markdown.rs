//! Markdown → HTML conversion with YAML frontmatter support.
//!
//! [`parse_doc`] strips a leading `---…---` frontmatter block (if present),
//! parses well-known keys (`title`, `date`, `author`, `template`,
//! `primary_color`), then renders the remaining Markdown to an HTML fragment
//! with `pulldown-cmark`.
//!
//! The `# Title` convention: if the Markdown body begins with an H1 heading
//! *and* no `title` key was found in frontmatter, the heading text is promoted
//! to `RenderedDoc::title` and stripped from `body_html`.

use pulldown_cmark::{Options, Parser, html};

/// The result of parsing a Markdown document.
#[derive(Debug, Clone, Default)]
pub struct RenderedDoc {
    /// Document title — from frontmatter `title:` or the first `# Heading`.
    pub title: String,
    /// Rendered HTML fragment (the body, without the title heading).
    pub body_html: String,
    /// Optional date string from frontmatter `date:`.
    pub date: Option<String>,
    /// Optional author from frontmatter `author:`.
    pub author: Option<String>,
    /// Optional template name from frontmatter `template:`.
    pub template: Option<String>,
    /// Optional accent colour (CSS hex) from frontmatter `primary_color:`.
    pub primary_color: Option<String>,
}

/// Parse a Markdown string (with optional YAML frontmatter) into a
/// [`RenderedDoc`].
pub fn parse_doc(source: &str) -> RenderedDoc {
    let (frontmatter, body) = split_frontmatter(source);
    let fm = parse_frontmatter(frontmatter.as_deref().unwrap_or(""));

    // Render the full body first so we can inspect the leading <h1>.
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION;

    let parser = Parser::new_ext(body, options);
    let mut full_html = String::new();
    html::push_html(&mut full_html, parser);

    // --- title extraction ---
    // Frontmatter title wins; otherwise lift the first `# Heading` from the body.
    let (title, body_html) = if fm.title.is_some() {
        (fm.title.unwrap_or_default(), full_html)
    } else {
        match lift_h1(&full_html) {
            Some((t, rest)) => (t, rest),
            None => (String::new(), full_html),
        }
    };

    RenderedDoc {
        title,
        body_html,
        date: fm.date,
        author: fm.author,
        template: fm.template,
        primary_color: fm.primary_color,
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Split off a leading `---\n…\n---\n` block.
/// Returns `(Some(frontmatter_str), rest_of_body)`.
fn split_frontmatter(source: &str) -> (Option<String>, &str) {
    let s = source.trim_start_matches('\u{feff}'); // strip BOM
    if !s.starts_with("---") {
        return (None, source);
    }
    // Must be `---` optionally followed by whitespace then a newline.
    let after_open = s.trim_start_matches("---");
    if !after_open.starts_with('\n') && !after_open.starts_with("\r\n") {
        return (None, source);
    }
    let rest = after_open
        .trim_start_matches("\r\n")
        .trim_start_matches('\n');
    // Find the closing `---`.
    if let Some(close_pos) = rest.find("\n---") {
        let fm = &rest[..close_pos];
        let after_close = &rest[close_pos + 4..]; // skip `\n---`
        let body = after_close
            .trim_start_matches("\r\n")
            .trim_start_matches('\n');
        (Some(fm.to_string()), body)
    } else {
        (None, source)
    }
}

#[derive(Default)]
struct Frontmatter {
    title: Option<String>,
    date: Option<String>,
    author: Option<String>,
    template: Option<String>,
    primary_color: Option<String>,
}

/// Parse the YAML frontmatter string into known fields.
/// Falls back gracefully on parse errors.
fn parse_frontmatter(yaml: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();
    if yaml.is_empty() {
        return fm;
    }
    // Use serde_yaml to parse into a generic Value.
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(yaml) else {
        return fm;
    };
    let Some(map) = value.as_mapping() else {
        return fm;
    };

    let get_str = |key: &str| -> Option<String> {
        map.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
    };

    fm.title = get_str("title");
    fm.date = get_str("date");
    fm.author = get_str("author");
    fm.template = get_str("template");
    fm.primary_color = get_str("primary_color").or_else(|| get_str("primaryColor"));
    fm
}

/// Find the first `<h1>…</h1>` in the HTML fragment, extract its inner text,
/// and return `(plain_title, html_without_that_h1)`.
fn lift_h1(html: &str) -> Option<(String, String)> {
    let open = html.find("<h1")?;
    let tag_end = html[open..].find('>')? + open + 1;
    let close = html[tag_end..].find("</h1>")? + tag_end;

    let inner_html = &html[tag_end..close];
    // Strip any inline HTML tags to get plain text.
    let plain = strip_tags(inner_html);

    let remaining = format!("{}{}", &html[..open], &html[close + 5..]);
    Some((plain.trim().to_string(), remaining))
}

/// Remove HTML tags from a string, returning plain text.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_frontmatter() {
        let md = "# Hello World\n\nSome text.";
        let doc = parse_doc(md);
        assert_eq!(doc.title, "Hello World");
        assert!(doc.body_html.contains("Some text."));
        assert!(!doc.body_html.contains("<h1"));
    }

    #[test]
    fn test_frontmatter_title() {
        let md = "---\ntitle: My Doc\ndate: 2026-01-01\n---\n\nBody text.";
        let doc = parse_doc(md);
        assert_eq!(doc.title, "My Doc");
        assert_eq!(doc.date.as_deref(), Some("2026-01-01"));
        assert!(doc.body_html.contains("Body text."));
    }

    #[test]
    fn test_frontmatter_overrides_h1() {
        let md = "---\ntitle: FM Title\n---\n\n# H1 Title\n\nBody.";
        let doc = parse_doc(md);
        assert_eq!(doc.title, "FM Title");
        // h1 stays in body when frontmatter title wins
        assert!(doc.body_html.contains("<h1"));
    }

    #[test]
    fn test_primary_color() {
        let md = "---\nprimary_color: \"#7c3aed\"\n---\n\nContent.";
        let doc = parse_doc(md);
        assert_eq!(doc.primary_color.as_deref(), Some("#7c3aed"));
    }
}
