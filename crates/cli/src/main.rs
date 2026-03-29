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
//!   prism clean                  — Clear local cache data

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
//!   prism serve                  — Start WebSocket server for streaming trace updates
//!   prism clean                  — Clear local cache data

mod commands;
mod config;
mod output;
mod tui;
mod version_check;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use url::Url;

const BUILD_HASH: &str = env!("PRISM_BUILD_HASH");

/// Prism — From cryptic error to root cause in one command.
#[derive(Parser)]
#[command(name = "prism", version = env!("CARGO_PKG_VERSION"), about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Commands,

    /// Output format: human, json, compact, or short.
    #[arg(
        long,
        default_value = "human",
        value_parser = ["human", "json", "compact", "short"],
        global = true
    )]
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

    /// Save analysis output as JSON to the specified file path.
    #[arg(long, global = true, value_name = "PATH")]
    save: Option<String>,

    /// Suppress non-essential output.
    #[arg(long, short, global = true)]
    quiet: bool,
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
    /// Clear local cache data.
    Clean(commands::clean::CleanArgs),
    /// Manage the error taxonomy database.
    Db(commands::db::DbArgs),
    /// Start WebSocket server for streaming trace updates.
    Serve(commands::serve::ServeArgs),
    /// Run binary, network, and cache self-checks.
    Diagnostic(commands::diagnostic::DiagnosticArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let update_check_handle = tokio::spawn(version_check::check_for_updates());

    let version: &'static str = Box::leak(build_version().into_boxed_str());
    let matches = Cli::command().version(version).get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    let loaded_config = config::ConfigManager::new()
        .and_then(|manager| manager.load())
        .ok();

    tracing_subscriber::fmt()
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
        config_loaded = loaded_config.is_some(),
        "CLI arguments parsed"
    );

    let mut network = prism_core::network::config::resolve_network(&cli.network);
    if let Some(ref rpc_url) = cli.rpc_url {
        network.rpc_url = rpc_url.clone();
    }

    tracing::debug!(
        resolved_network = ?network.network,
        rpc_url = %network.rpc_url,
        archive_url_count = network.archive_urls.len(),
        "Resolved network configuration"
    );

    let save = cli.save.as_deref();

    match cli.command {
        Commands::Decode(args) => commands::decode::run(args, &network, &cli.output, save).await?,
        Commands::Inspect(args) => {
            commands::inspect::run(args, &network, &cli.output, save).await?
        }
        Commands::Trace(args) => commands::trace::run(args, &network, &cli.output, save).await?,
        Commands::Profile(args) => {
            commands::profile::run(args, &network, &cli.output, save).await?
        }
        Commands::Diff(args) => commands::diff::run(args, &network, &cli.output, save).await?,
        Commands::Replay(args) => commands::replay::run(args, &network, &cli.quiet).await?,
        Commands::Whatif(args) => commands::whatif::run(args, &network, &cli.output, save).await?,
        Commands::Export(args) => commands::export::run(args, &network, &cli.quiet).await?,
        Commands::Clean(args) => commands::clean::run(args).await?,
        Commands::Db(args) => commands::db::run(args).await?,
        Commands::Serve(args) => commands::serve::run(args, &network).await?,
        Commands::Diagnostic(args) => commands::diagnostic::run(args).await?,
    }

    if let Ok(Ok(Some(newer_version))) =
        tokio::time::timeout(std::time::Duration::from_millis(50), update_check_handle).await
    {
        eprintln!(
            "\n{}",
            colored::Colorize::bright_yellow(
                format!(
                    "A newer version of Prism is available (v{}). Update to stay current!",
                    newer_version
                )
                .as_str()
            )
        );
    }

    Ok(())
}

fn build_version() -> String {
    format!(
        "prism {} (build: {}) | Soroban Protocol: {}",
        prism_core::VERSION,
        BUILD_HASH,
        prism_core::SOROBAN_PROTOCOL_VERSION
    )
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
        .add_directive(
            format!("prism={prism_level}")
                .parse()
                .expect("valid directive"),
        )
        .add_directive(
            format!("prism_core={prism_level}")
                .parse()
                .expect("valid directive"),
        )
}

fn validate_url(value: &str) -> Result<String, String> {
    Url::parse(value)
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
        assert!(build_log_filter(cli.verbose)
            .to_string()
            .contains("prism=trace"));
    }

    #[test]
    fn parses_long_verbose_flag_after_subcommand() {
        let cli = Cli::try_parse_from(["prism", "decode", "--verbose", &"a".repeat(64)])
            .expect("cli should parse");
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn parses_short_output_alias() {
        let cli = Cli::try_parse_from(["prism", "--output", "short", "decode", "abc123"])
            .expect("cli should parse");
        assert_eq!(cli.output, "short");
    }

    #[test]
    fn parses_trace_tx_hash_as_positional_argument() {
        let cli = Cli::try_parse_from(["prism", "trace", "abc123"]).expect("cli should parse");

        match cli.command {
            Commands::Trace(args) => {
                assert_eq!(args.tx_hash, "abc123");
                assert!(args.output_file.is_none());
            }
            _ => panic!("expected trace command"),
        }
    }

    #[test]
    fn parses_trace_output_file_flag_with_positional_tx_hash() {
        let cli = Cli::try_parse_from(["prism", "trace", "abc123", "--output-file", "trace.json"])
            .expect("cli should parse");

        match cli.command {
            Commands::Trace(args) => {
                assert_eq!(args.tx_hash, "abc123");
                assert_eq!(args.output_file.as_deref(), Some("trace.json"));
            }
            _ => panic!("expected trace command"),
        }
    }

    #[test]
    fn parses_diff_tx_hash_argument() {
        let cli = Cli::try_parse_from(["prism", "diff", "deadbeef"]).expect("cli should parse");

        match cli.command {
            Commands::Diff(args) => assert_eq!(args.tx_hash, "deadbeef"),
            _ => panic!("expected diff command"),
        }
    }

    #[test]
    fn parses_save_flag_for_trace() {
        let tx_hash = "a".repeat(64);
        let cli = Cli::try_parse_from(["prism", "--save", "report.json", "trace", &tx_hash])
            .expect("cli should parse with --save");
        assert_eq!(cli.save.as_deref(), Some("report.json"));
    }

    #[test]
    fn save_flag_absent_by_default() {
        let cli = Cli::try_parse_from(["prism", "db", "update"]).expect("cli should parse");
        assert!(cli.save.is_none());
    }

    #[test]
    fn save_flag_can_appear_after_subcommand() {
        let tx_hash = "a".repeat(64);
        let cli = Cli::try_parse_from(["prism", "trace", &tx_hash, "--save", "out.json"])
            .expect("--save after subcommand should parse");
        assert_eq!(cli.save.as_deref(), Some("out.json"));
    }

    #[test]
    fn defaults_to_warn_without_verbose() {
        let warn = build_log_filter(0).to_string();
        let debug = build_log_filter(1).to_string();
        let trace = build_log_filter(2).to_string();

        assert!(warn.contains("prism=warn"));
        assert!(debug.contains("prism=debug"));
        assert!(trace.contains("prism=trace"));
        assert!(trace.contains("prism_core=trace"));
    }

    #[test]
    fn version_string_includes_build_hash_and_protocol() {
        let version = build_version();

        assert!(version.contains(prism_core::VERSION));
        assert!(version.contains(BUILD_HASH));
        assert!(version.contains(&prism_core::SOROBAN_PROTOCOL_VERSION.to_string()));
    }
}
