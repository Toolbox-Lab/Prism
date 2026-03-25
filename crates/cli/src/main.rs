//! Prism CLI — Soroban Transaction Debugger
//!
//! Usage:
//!   prism decode <tx-hash>       — Decode a transaction error
//!   prism inspect <tx-hash>      — Full transaction context
//!   prism trace <tx-hash>        — Replay and trace execution
//!   prism profile <tx-hash>      — Resource consumption profile
//!   prism diff <tx-hash>         — State diff (before/after)
//!   prism replay <tx-hash> -i    — Interactive TUI debugger
//!   prism whatif <tx-hash>       — Re-simulate with modifications
//!   prism export <tx-hash>       — Export as regression test
//!   prism db update              — Update taxonomy database

mod commands;
mod output;
mod tui;

use clap::{Parser, Subcommand};

/// Prism — From cryptic error to root cause in one command.
#[derive(Parser)]
#[command(name = "prism", version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Commands,

    /// Output format: human, json, compact.
    #[arg(long, default_value = "human", global = true)]
    output: String,

    /// Network: mainnet, testnet, futurenet, or a custom RPC URL.
    #[arg(long, short, default_value = "testnet", global = true)]
    network: String,

    /// Enable verbose logging.
    #[arg(long, short, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Decode a transaction error into plain English.
    Decode(commands::decode::DecodeArgs),
    /// Inspect full transaction context.
    Inspect(commands::inspect::InspectArgs),
    /// Replay transaction and output execution trace.
    Trace(commands::trace::TraceArgs),
    /// Generate resource consumption profile.
    Profile(commands::profile::ProfileArgs),
    /// Show state diff (before/after) for a transaction.
    Diff(commands::diff::DiffArgs),
    /// Launch interactive TUI debugger.
    Replay(commands::replay::ReplayArgs),
    /// Re-simulate with modified inputs.
    Whatif(commands::whatif::WhatifArgs),
    /// Export debug session as a regression test.
    Export(commands::export::ExportArgs),
    /// Manage the error taxonomy database.
    Db(commands::db::DbArgs),
    /// Manage API credentials for hosted services.
    Auth(commands::auth::AuthArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    // Resolve network configuration
    let network = prism_core::network::config::resolve_network(&cli.network);

    // Dispatch to command handler
    match cli.command {
        Commands::Decode(args) => commands::decode::run(args, &network, &cli.output).await?,
        Commands::Inspect(args) => commands::inspect::run(args, &network, &cli.output).await?,
        Commands::Trace(args) => commands::trace::run(args, &network, &cli.output).await?,
        Commands::Profile(args) => commands::profile::run(args, &network, &cli.output).await?,
        Commands::Diff(args) => commands::diff::run(args, &network, &cli.output).await?,
        Commands::Replay(args) => commands::replay::run(args, &network).await?,
        Commands::Whatif(args) => commands::whatif::run(args, &network, &cli.output).await?,
        Commands::Export(args) => commands::export::run(args, &network).await?,
        Commands::Db(args) => commands::db::run(args).await?,
        Commands::Auth(args) => commands::auth::run(args).await?,
    }

    Ok(())
}
