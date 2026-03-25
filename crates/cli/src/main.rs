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
//!   prism serve                  — Start web server for Prism Web UI
//!   prism clean                  — Clear local cache data
//!   prism serve                  — Launch Web UI dashboard

mod commands;
mod config;
mod output;
mod tui;

use clap::{ ArgAction, Parser, Subcommand };
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

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

    /// Enable verbose logging. Repeat for more detail.
    #[arg(long, short, action = ArgAction::Count, global = true)]
    verbose: u8,

    /// Override RPC URL (e.g. http://localhost:8000)
    #[arg(long, global = true, value_parser = validate_url)]
    rpc_url: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Decode a transaction error into plain English.
    #[command(subcommand_help_heading = "Analysis Commands")]
    Decode(commands::decode::DecodeArgs),

    /// Inspect full transaction context.
    #[command(subcommand_help_heading = "Analysis Commands")]
    Inspect(commands::inspect::InspectArgs),

    /// Replay transaction and output execution trace.
    #[command(subcommand_help_heading = "Analysis Commands")]
    Trace(commands::trace::TraceArgs),

    /// Generate resource consumption profile.
    #[command(subcommand_help_heading = "Analysis Commands")]
    Profile(commands::profile::ProfileArgs),

    /// Show state diff (before/after) for a transaction.
    #[command(subcommand_help_heading = "State & Simulation")]
    Diff(commands::diff::DiffArgs),

    /// Re-simulate with modified inputs.
    #[command(subcommand_help_heading = "State & Simulation")]
    Whatif(commands::whatif::WhatifArgs),

    /// Launch interactive TUI debugger.
    #[command(subcommand_help_heading = "Development Tools")]
    Replay(commands::replay::ReplayArgs),

    /// Export debug session as a regression test.
    #[command(subcommand_help_heading = "Development Tools")]
    Export(commands::export::ExportArgs),

    /// Launch Web UI dashboard.
    #[command(subcommand_help_heading = "Development Tools")]
    Serve(commands::serve::ServeArgs),

    /// Clear local cache data.
    #[command(subcommand_help_heading = "Configuration & Maintenance")]
    Clean(commands::clean::CleanArgs),

    /// Manage the error taxonomy database.
    #[command(subcommand_help_heading = "Configuration & Maintenance")]
    Db(commands::db::DbArgs),
    /// Start a local web server to host the Prism Web UI.
    Serve(commands::serve::ServeArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging before resolving the network or dispatching commands.
    tracing_subscriber
        ::fmt()
        .with_env_filter(build_log_filter(cli.verbose))
        .with_writer(std::io::stderr)
        .with_file(cli.verbose > 1)
        .with_line_number(cli.verbose > 1)
        .with_thread_ids(cli.verbose > 1)
        .init();

    tracing::debug!(
        output = %cli.output,
        network_arg = %cli.network,
        verbose = cli.verbose,
        "CLI arguments parsed"
    );

    // Resolve network configuration
    let mut network = prism_core::network::config::resolve_network(&cli.network);

    // Override RPC URL if provided
    if let Some(ref rpc_url) = cli.rpc_url {
        network.rpc_url = rpc_url.clone();
    }

    tracing::debug!(
        resolved_network = ?network.network,
        rpc_url = %network.rpc_url,
        archive_url_count = network.archive_urls.len(),
        "Resolved network configuration"
    );

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
        Commands::Clean(args) => commands::clean::run(args).await?,
        Commands::Db(args) => commands::db::run(args).await?,
        Commands::Serve(args) => commands::serve::run(args).await?,
    }

    Ok(())
}

fn build_log_filter(verbose: u8) -> EnvFilter {
    let prism_level = match verbose {
        0 => LevelFilter::WARN,
        1 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };

    EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .parse_lossy("")
        .add_directive(format!("prism={prism_level}").parse().expect("valid directive"))
        .add_directive(format!("prism_core={prism_level}").parse().expect("valid directive"))
}

fn validate_url(value: &str) -> Result<String, String> {
    url::Url::parse(value)
        .map(|_| value.to_string())
        .map_err(|_| format!("Invalid URL: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_verbose_flag() {
        let cli = Cli::try_parse_from(["prism", "-v", "db", "update"]).expect("cli should parse");
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn parses_repeated_verbose_flags_as_trace() {
        let cli = Cli::try_parse_from(["prism", "-vv", "db", "update"]).expect("cli should parse");
        assert_eq!(cli.verbose, 2);
        assert!(build_log_filter(cli.verbose).to_string().contains("prism=trace"));
    }

    #[test]
    fn parses_long_verbose_flag_after_subcommand() {
        let cli = Cli::try_parse_from(["prism", "decode", "--verbose", "abc123"]).expect(
            "cli should parse"
        );
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn defaults_to_warn_without_verbose() {
        let warn = build_log_filter(0).to_string();
        let debug = build_log_filter(1).to_string();
        let trace = build_log_filter(2).to_string();

        assert!(warn.contains("prism=warn"));
        assert!(debug.contains("prism=debug"));
        assert!(trace.contains("prism=trace"));
    }
}
