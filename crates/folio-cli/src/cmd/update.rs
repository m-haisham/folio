//! `folio update` — self-update folio to the latest release.

use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType, Version};
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

    // is_update_needed() returns false without a receipt (can't verify the
    // install prefix), so track whether one was found to handle that below.
    let has_receipt = updater.load_receipt().is_ok();

    if !has_receipt {
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

    let current_str = env!("CARGO_PKG_VERSION");
    let current_version: Version = current_str
        .parse()
        .map_err(|e| eyre::eyre!("could not parse current version '{}': {}", current_str, e))?;

    // query_new_version() returns the latest release regardless of whether
    // it's newer — clone immediately to free the borrow on `updater`.
    let latest: Option<Version> = updater
        .query_new_version()
        .await
        .map_err(|e| eyre::eyre!("could not check for updates: {}", e))?
        .cloned();

    let update_available = latest.as_ref().map_or(false, |v| v > &current_version);

    if args.check {
        if update_available {
            println!(
                "update available: {} → {}",
                current_str,
                latest.as_ref().unwrap()
            );
            println!(
                "run {} to install it.",
                colored::Colorize::bold("`folio update`")
            );
        } else {
            println!(
                "{} folio {} is up to date.",
                colored::Colorize::green("✓"),
                current_str
            );
        }
        return Ok(());
    }

    println!("checking for updates…");

    if !update_available {
        println!(
            "{} folio {} is already up to date.",
            colored::Colorize::green("✓"),
            current_str
        );
        return Ok(());
    }

    // Without a receipt, bypass the install-prefix check since we've already
    // confirmed a newer version exists.
    if !has_receipt {
        updater.always_update(true);
    }

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
                current_str
            );
        }
        Err(e) => {
            return Err(eyre::eyre!("update failed: {}", e));
        }
    }

    Ok(())
}
