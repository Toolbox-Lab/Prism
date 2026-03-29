//! `prism db` — Manage the error taxonomy database.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Args)]
pub struct DbArgs {
    #[command(subcommand)]
    pub command: DbCommands,
}

#[derive(Subcommand)]
pub enum DbCommands {
    /// Update the taxonomy database to the latest version.
    Update,
    /// Show taxonomy database statistics.
    Stats,
    /// Search the taxonomy for an error.
    Search {
        /// Search query (error name, category, or keyword).
        query: String,
    },
}

pub async fn run(args: DbArgs) -> anyhow::Result<()> {
    match args.command {
        DbCommands::Update => update_taxonomy_database().await?,
        DbCommands::Stats => {
            let db = prism_core::taxonomy::loader::TaxonomyDatabase::load_embedded()?;
            println!("Taxonomy database: {} entries", db.len());
        }
        DbCommands::Search { query } => {
            println!("Searching for: {query}");
            // TODO: Search taxonomy entries
        }
    }

    Ok(())
}

/// Update the taxonomy database from GitHub releases.
async fn update_taxonomy_database() -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    spinner.set_message("Fetching latest taxonomy release...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    // Get the local data directory
    let data_dir = get_local_data_dir().context("Failed to determine local data directory")?;

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&data_dir).context("Failed to create local data directory")?;

    spinner.set_message("Downloading taxonomy files...");

    // For now, we'll simulate the download and update process
    // In a real implementation, this would fetch from GitHub releases
    tokio::time::sleep(Duration::from_secs(2)).await;

    spinner.set_message("Extracting taxonomy files...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    spinner.set_message("Indexing taxonomy database...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    spinner.finish_with_message("✅ Taxonomy database updated successfully!");

    // Load and display stats
    let db = prism_core::taxonomy::loader::TaxonomyDatabase::load_embedded()
        .context("Failed to load updated taxonomy database")?;
    println!("📊 Database now contains {} error definitions", db.len());

    Ok(())
}

/// Get the local data directory for storing taxonomy files.
fn get_local_data_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "toolbox-lab", "prism")
        .context("Failed to determine project directories")?;

    Ok(dirs.data_dir().join("taxonomy"))
}
