//! Filesystem helpers for client-scoped Markdown documents.
//!
//! Documents live at `<clients_dir>/<slug>/documents/**/*.md`.
//! Their PDFs are mirrored under `<output_dir>/clients/<slug>/documents/**/*.pdf`.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Recursively collect every `.md` file under `<clients_dir>/<slug>/documents/`.
///
/// Returns absolute paths, sorted for deterministic ordering.
/// Returns an empty vec if the directory doesn't exist.
pub fn list_docs(clients_dir: &Path, slug: &str) -> Vec<PathBuf> {
    let docs_dir = documents_dir(clients_dir, slug);
    let mut out = Vec::new();
    collect_md(&docs_dir, &mut out);
    out.sort();
    out
}

/// Recursively collect `.md` files under `<clients_dir>/` across **all** client slugs.
///
/// Returns `(slug, absolute_path)` pairs, sorted by slug then path.
pub fn list_all_docs(clients_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();

    let entries = match fs::read_dir(clients_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };

    let mut slugs: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    slugs.sort();

    for slug in slugs {
        for path in list_docs(clients_dir, &slug) {
            out.push((slug.clone(), path));
        }
    }

    out
}

/// Return the `documents/` directory for a client.
pub fn documents_dir(clients_dir: &Path, slug: &str) -> PathBuf {
    clients_dir.join(slug).join("documents")
}

/// Compute the mirrored output PDF path for a source `.md` file.
///
/// The relative path of `source` under `clients_dir` is reproduced under
/// `output_dir/clients/`, with the extension changed to `.pdf`.
///
/// Example:
/// - `clients_dir` = `<root>/clients`
/// - `source`      = `<root>/clients/acme/documents/proposals/q3.md`
/// - `output_dir`  = `<root>/output`
/// - result        = `<root>/output/clients/acme/documents/proposals/q3.pdf`
pub fn doc_output_path(source: &Path, clients_dir: &Path, output_dir: &Path) -> PathBuf {
    let rel = source.strip_prefix(clients_dir).unwrap_or(source);
    let pdf_rel = rel.with_extension("pdf");
    output_dir.join("clients").join(pdf_rel)
}

/// Return the path of a source `.md` relative to a client's `documents/` dir.
///
/// Used for display purposes. Falls back to the full path if stripping fails.
pub fn doc_rel_path<'a>(source: &'a Path, clients_dir: &Path, slug: &str) -> &'a Path {
    let docs_dir = documents_dir(clients_dir, slug);
    source.strip_prefix(&docs_dir).unwrap_or(source)
}

// ── Internal ──────────────────────────────────────────────────────────────────

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_md(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
}
