use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::collections::HashSet;
use std::time::Duration;

const TAXONOMY_URL: &str = "https://raw.githubusercontent.com/grat-soroban/grat/main/taxonomy/enhanced_error_taxonomy.toml";

#[derive(Args)]
pub struct DbArgs {
    #[command(subcommand)]
    pub command: DbCommands,
}

#[derive(Subcommand)]
pub enum DbCommands {
    /// Download and install the latest taxonomy database.
    Update,
    /// Show the number of loaded entries and categories.
    Stats,
    /// Search taxonomy error definitions.
    Search { query: String },
}

#[derive(Serialize)]
struct DatabaseStats {
    entries: usize,
    categories: usize,
}

pub async fn run(args: DbArgs, output_format: &str) -> Result<()> {
    let json = matches!(
        crate::output::OutputFormat::parse(output_format),
        crate::output::OutputFormat::Json
    );

    match args.command {
        DbCommands::Update => update_taxonomy_database(json).await?,
        DbCommands::Stats => {
            let db = grat_core::taxonomy::loader::TaxonomyDatabase::load_latest()?;
            let stats = DatabaseStats {
                entries: db.len(),
                categories: db
                    .all_entries()
                    .iter()
                    .map(|entry| &entry.category)
                    .collect::<HashSet<_>>()
                    .len(),
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                println!(
                    "Taxonomy database: {} entries across {} categories",
                    stats.entries, stats.categories
                );
            }
        }
        DbCommands::Search { query } => {
            let db = grat_core::taxonomy::loader::TaxonomyDatabase::load_latest()?;
            let results = db.search(&query);
            if json {
                println!("{}", serde_json::to_string_pretty(&results)?);
            } else if results.is_empty() {
                println!("No matching entries found for '{query}'");
            } else {
                for entry in results {
                    println!(
                        "{} {} — {}\n  {}",
                        entry.category, entry.code, entry.name, entry.summary
                    );
                }
            }
        }
    }
    Ok(())
}

async fn update_taxonomy_database(json: bool) -> Result<()> {
    let progress = if json {
        ProgressBar::hidden()
    } else {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")?
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
        );
        bar.enable_steady_tick(Duration::from_millis(100));
        bar
    };
    progress.set_message("Downloading taxonomy database...");

    let client = reqwest::Client::builder()
        .user_agent("grat-taxonomy-updater")
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")?;
    let response = client
        .get(TAXONOMY_URL)
        .send()
        .await
        .context("Failed to download taxonomy database")?;
    if !response.status().is_success() {
        bail!(
            "Taxonomy download failed with HTTP status {}",
            response.status()
        );
    }
    let content = response
        .text()
        .await
        .context("Failed to read taxonomy response")?;
    progress.set_message("Validating taxonomy database...");
    let schema = grat_core::taxonomy::loader::TaxonomyParser::parse(&content)
        .map_err(anyhow::Error::msg)
        .context("Downloaded taxonomy database is invalid")?;
    if schema.errors.is_empty() {
        bail!("Downloaded taxonomy database contains no error definitions");
    }

    let path = grat_core::taxonomy::updater::db_file_path()
        .context("Failed to determine local taxonomy database path")?;
    let parent = path
        .parent()
        .context("Taxonomy database path has no parent")?;
    std::fs::create_dir_all(parent).context("Failed to create taxonomy data directory")?;
    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, &content).context("Failed to write taxonomy database")?;
    if let Err(error) = std::fs::rename(&temp_path, &path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error).context("Failed to install taxonomy database");
    }

    progress.finish_with_message("Taxonomy database updated");
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "ok",
                "entries": schema.errors.len(),
                "categories": 1
            }))?
        );
    } else {
        println!(
            "Updated taxonomy database with {} error definitions",
            schema.errors.len()
        );
    }
    Ok(())
}
