use clap::{Args, Subcommand};
use eyre::Result;
use folio_core::{
    config::find_root,
    templates::{export_template, list_bundled, list_custom},
};
use std::path::PathBuf;

#[derive(Args)]
pub struct TemplatesArgs {
    #[command(subcommand)]
    pub command: Option<TemplatesCommands>,
}

#[derive(Subcommand)]
pub enum TemplatesCommands {
    Export {
        name: String,
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
                let custom = list_custom(root);
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
