//! `prism db` — Manage the error taxonomy database.

use clap::{Args, Subcommand};

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

pub async fn run(args: DbArgs, quiet: &bool) -> anyhow::Result<()> {
    match args.command {
        DbCommands::Update => {
            if !*quiet {
                println!("Updating taxonomy database...");
            }
            // TODO: Download latest taxonomy from GitHub releases
            if !*quiet {
                println!("Database is up to date.");
            }
        }
        DbCommands::Stats => {
            let db = prism_core::taxonomy::loader::TaxonomyDatabase::load_embedded()?;
            if !*quiet {
                println!("Taxonomy database: {} entries", db.len());
            }
        }
        DbCommands::Search { query } => {
            if !*quiet {
                println!("Searching for: {query}");
            }
            // TODO: Search taxonomy entries
        }
    }

    Ok(())
}
