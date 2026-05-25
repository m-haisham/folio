//! `folio update` — self-update folio to the latest release.
//!
//! Checks GitHub releases for a newer version of folio and installs it in
//! place. The update is performed by axoupdater, which downloads and runs the
//! upstream installer script.
//!
//! If the binary was installed via a cargo-dist installer (which writes an
//! install receipt), the receipt is used automatically. Otherwise, the release
//! source is configured directly from the embedded repository metadata.

use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};
use clap::Args;
use eyre::Result;

/// Update folio to the latest release from GitHub.
///
/// Checks whether a newer version is available and, if so, downloads and
/// installs it in place. The current binary is replaced atomically.
///
/// Examples:
///
/// ```sh
/// folio update
/// folio update --check
/// ```
#[derive(Args)]
pub struct UpdateArgs {
    /// Only check whether an update is available; do not install it.
    #[arg(long)]
    pub check: bool,
}

pub async fn run(args: UpdateArgs) -> Result<()> {
    let mut updater = AxoUpdater::new_for("folio");

    // Try to load a cargo-dist install receipt first. If none exists (e.g. the
    // binary was installed with `cargo install`), fall back to configuring the
    // release source manually from the embedded repository metadata.
    if updater.load_receipt().is_err() {
        updater.set_release_source(ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner: "m-haisham".to_string(),
            name: "folio".to_string(),
            app_name: "folio".to_string(),
        });

        let current = env!("CARGO_PKG_VERSION");
        updater
            .set_current_version(
                current.parse().map_err(|e| {
                    eyre::eyre!("could not parse current version '{}': {}", current, e)
                })?,
            )
            .map_err(|e| eyre::eyre!("could not set current version: {}", e))?;
    }

    if args.check {
        // --check: report whether an update is available, then exit.
        match updater.query_new_version().await {
            Ok(Some(new_ver)) => {
                let current = env!("CARGO_PKG_VERSION");
                println!("update available: {} → {}", current, new_ver);
                println!(
                    "run {} to install it.",
                    colored::Colorize::bold("`folio update`")
                );
            }
            Ok(None) => {
                println!(
                    "{} folio {} is up to date.",
                    colored::Colorize::green("✓"),
                    env!("CARGO_PKG_VERSION")
                );
            }
            Err(e) => {
                return Err(eyre::eyre!("could not check for updates: {}", e));
            }
        }
        return Ok(());
    }

    // Full update: download and install.
    println!("checking for updates…");

    match updater.run().await {
        Ok(Some(_)) => {
            println!(
                "{} folio updated successfully.",
                colored::Colorize::green("✓")
            );
        }
        Ok(None) => {
            println!(
                "{} folio {} is already up to date.",
                colored::Colorize::green("✓"),
                env!("CARGO_PKG_VERSION")
            );
        }
        Err(e) => {
            return Err(eyre::eyre!("update failed: {}", e));
        }
    }

    Ok(())
}
