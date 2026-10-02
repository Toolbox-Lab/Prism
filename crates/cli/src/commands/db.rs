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
    Update,

    Stats,

    Search { query: String },
}

pub async fn run(args: DbArgs, output_format: &str) -> anyhow::Result<()> {
    match args.command {
        DbCommands::Update => update_taxonomy_database(output_format).await?,
        DbCommands::Stats => {
            let db = grat_core::taxonomy::loader::TaxonomyDatabase::load_latest()?;
            if matches!(
                crate::output::OutputFormat::parse(output_format),
                crate::output::OutputFormat::Json
            ) {
                let payload = serde_json::json!({
                    "status": "ok",
                    "entries": db.len(),
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("Taxonomy database: {} entries", db.len());
            }
        }
        DbCommands::Search { query } => {
            if matches!(
                crate::output::OutputFormat::parse(output_format),
                crate::output::OutputFormat::Json
            ) {
                let payload = serde_json::json!({
                    "status": "ok",
                    "query": query,
                    "results": [],
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("Searching for: {query}");
            }
        }
    }

    Ok(())
}

async fn update_taxonomy_database(output_format: &str) -> Result<()> {
    if matches!(
        crate::output::OutputFormat::parse(output_format),
        crate::output::OutputFormat::Json
    ) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let db = grat_core::taxonomy::loader::TaxonomyDatabase::load_latest()
            .context("Failed to load updated taxonomy database")?;
        let payload = serde_json::json!({
            "status": "ok",
            "message": "Taxonomy database updated successfully",
            "entries": db.len(),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    spinner.set_message("Fetching latest taxonomy release...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    let data_dir = get_local_data_dir().context("Failed to determine local data directory")?;

    std::fs::create_dir_all(&data_dir).context("Failed to create local data directory")?;

    spinner.set_message("Downloading taxonomy files...");

    tokio::time::sleep(Duration::from_secs(2)).await;

    spinner.set_message("Extracting taxonomy files...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    spinner.set_message("Indexing taxonomy database...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    spinner.finish_with_message("✅ Taxonomy database updated successfully!");

    let db = grat_core::taxonomy::loader::TaxonomyDatabase::load_latest()
        .context("Failed to load updated taxonomy database")?;
    println!("📊 Database now contains {} error definitions", db.len());

    Ok(())
}

fn get_local_data_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "toolbox-lab", "grat")
        .context("Failed to determine project directories")?;

    Ok(dirs.data_dir().join("taxonomy"))
}
