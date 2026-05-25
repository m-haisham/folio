//! `folio templates` — list and export invoice templates.
//!
//! With no subcommand, lists all bundled templates and any custom templates
//! found in the `templates/` directory. The `export` subcommand copies a
//! bundled template's source files into a local directory for editing.

use clap::{Args, Subcommand};
use eyre::Result;
use folio_core::{
    config::find_root,
    templates::{export_template, list_bundled, list_custom},
};
use std::path::PathBuf;

/// List available templates or export a bundled one for customisation.
///
/// With no subcommand, prints all bundled and local custom templates.
/// Use the `export` subcommand to copy a bundled template into your
/// `templates/` directory so you can edit it.
///
/// Examples:
///
/// ```sh
/// folio templates
/// folio templates export basic --output templates/studio
/// ```
#[derive(Args)]
pub struct TemplatesArgs {
    #[command(subcommand)]
    pub command: Option<TemplatesCommands>,
}

#[derive(Subcommand)]
pub enum TemplatesCommands {
    /// Copy a bundled template into a local directory for editing.
    Export {
        /// Name of the bundled template to export (e.g. `basic`, `modern`).
        name: String,

        /// Directory to write the template files into.
        #[arg(long)]
        output: PathBuf,
    },
}

pub async fn run(args: TemplatesArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;

    match args.command {
        Some(TemplatesCommands::Export { name, output }) => {
            export_template(&name, &output).map_err(|e| eyre::eyre!("{}", e))?;
            println!("✓ Exported template '{}' to {}", name, output.display());
        }
        None => {
            let root = find_root(&cwd);

            println!("BUNDLED");
            for t in list_bundled() {
                println!("  {:<10} {}", t.name, t.description);
            }

            if let Some(ref root) = root {
                let config = folio_core::config::load_config(root).unwrap_or_default();
                let store =
                    folio_core::store::FilesystemStore::with_paths(root, config.paths().clone());
                let custom = list_custom(&store.templates_dir());
                if !custom.is_empty() {
                    println!("\nCUSTOM (templates/)");
                    for t in &custom {
                        println!("  {:<10} {}", t.name, t.path.as_deref().unwrap_or(""));
                    }
                }
            }
        }
    }

    Ok(())
}
