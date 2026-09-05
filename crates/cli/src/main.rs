mod commands;
mod config;
mod output;
mod tui;
mod ui;
mod version_check;

use clap::{
    ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use url::Url;

const BUILD_HASH: &str = env!("GRAT_BUILD_HASH");

#[derive(Parser)]
#[command(name = "grat", version = env!("CARGO_PACKAGE_VERSION"), about, long_about = None)]
#[command(propagate_version = true)]
#[command(before_help = ui::logo::GRAT_LOGO)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(
        long,
        default_value = "human",
        value_parser = ["human", "json", "compact", "short"],
        global = true
    )]
    output: String,

    #[arg(long, short, default_value = "testnet", global = true)]
    network: String,

    #[arg(long, short, action = ArgAction::Count, global = true)]
    verbose: u8,

    #[arg(long, global = true, value_parser = validate_url)]
    rpc_url: Option<String>,

    #[arg(long, global = true, value_name = "PATH")]
    save: Option<String>,

    #[arg(long, short, global = true)]
    quiet: bool,

    #[arg(long, global = true)]
    no_color: bool,

    #[arg(long, global = true, help = "Disable network requests for updates")]
    offline: bool,

    #[arg(long, global = true, help = "Bypass local cache and query network providers")]
    no_cache: bool,
}

#[derive(Subcommand)]
enum Commands {
    #[command(next_help_heading = "Analysis Commands")]
    Decode(commands::decode::DecodeArgs),

    Batch(commands::batch::BatchArgs),

    Inspect(commands::inspect::InspectArgs),

    Trace(commands::trace::TraceArgs),

    Profile(commands::profile::ProfileArgs),

    Diff(commands::diff::DiffArgs),

    #[command(next_help_heading = "Debug & TUI Commands")]
    Replay(commands::replay::ReplayArgs),

    Whatif(commands::whatif::WhatifArgs),

    Export(commands::export::ExportArgs),

    #[command(next_help_heading = "System & Data Commands")]
    Db(commands::db::DbArgs),

    Clean(commands::clean::CleanArgs),

    Auth(commands::auth::AuthArgs),

    Diagnostic(commands::diagnostic::DiagnosticArgs),

    #[command(next_help_heading = "System & Data Commands")]
    History,

    Completions {
        shell: clap_complete::Shell,
    },

    Serve(commands::serve::ServeArgs),

    SearchError(commands::search_error::SearchErrorArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _update_check_handle = tokio::spawn(version_check::check_for_updates());

    let version: &'static str = Box::leak(build_version().into_boxed_str());
    let matches = Cli::command().version(version).get_matches();
    let cli = Cli::from_arg_matches(&matches)?;

    let _taxonomy_update_handle =
        tokio::spawn(grat_core::taxonomy::updater::check_and_update(cli.offline));
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
        no_color = cli.no_color,
        no_cache = cli.no_cache,
        config_loaded = loaded_config.is_some(),
        "CLI arguments parsed"
    );

    output::theme::set_color_enabled(!cli.no_color);

    let mut network = grat_core::network::config::resolve_network(&cli.network);
    if let Some(ref rpc_url) = cli.rpc_url {
        network.rpc_url = rpc_url.clone();
    }
    network.no_cache = cli.no_cache;

    tracing::debug!(
        resolved_network = ?network.network,
        rpc_url = %network.rpc_url,
        archive_url_count = network.archive_urls.len(),
        no_cache = network.no_cache,
        "Resolved network configuration"
    );

    let save = cli.save.as_deref();

    match cli.command {
        Commands::Decode(args) => commands::decode::run(args, &network, &cli.output, save).await?,
        Commands::Batch(args) => commands::batch::run(args, &network).await?,
        Commands::Inspect(args) => {
            commands::inspect::run(args, &network, &cli.output, save).await?;
        }
        Commands::Trace(args) => commands::trace::run(args, &network, &cli.output, save).await?,
        Commands::Profile(args) => {
            commands::profile::run(args, &network, &cli.output, save).await?;
        }
        Commands::Diff(args) => commands::diff::run(args, &network, &cli.output, save).await?,
        Commands::Replay(args) => {
            commands::replay::run(args, &network, &cli.output, &cli.quiet).await?;
        }
        Commands::Whatif(args) => commands::whatif::run(args, &network, &cli.output, save).await?,
        Commands::Export(args) => {
            commands::export::run(args, &network, &cli.output, &cli.quiet).await?;
        }
        Commands::Clean(args) => commands::clean::run(args, &cli.output).await?,
        Commands::Db(args) => commands::db::run(args, &cli.output).await?,
        Commands::Auth(args) => commands::auth::run(args, &cli.output).await?,
        Commands::Diagnostic(args) => commands::diagnostic::run(args).await?,
        Commands::History => commands::history::run(&cli.output).await?,
        Commands::Serve(args) => commands::serve::run(args, &network).await?,
        Commands::SearchError(args) => commands::search_error::run(args).await?,
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        }
    }

    Ok(())
}

fn build_version() -> String {
    format!(
        "{}\ngrat {} (build: {}) | Soroban Protocol: {}",
        ui::logo::GRAT_LOGO,
        grat_core::VERSION,
        BUILD_HASH,
        grat_core::SOROBAN_PROTOCOL_VERSION
    )
}

fn build_log_filter(verbose: u8) -> EnvFilter {
    let grat_level = match verbose {
        0 => LevelFilter::WARN,
        1 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };

    EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .parse_lossy("")
        .add_directive(
            format!("grat={grat_level}")
                .parse()
                .expect("valid directive"),
        )
        .add_directive(
            format!("grat_core={grat_level}")
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
        let cli = Cli::try_parse_from(["grat", "-v", "db", "update"]).expect("cli should parse");
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn parses_repeated_verbose_flags_as_trace() {
        let cli = Cli::try_parse_from(["grat", "-vv", "db", "update"]).expect("cli should parse");
        assert_eq!(cli.verbose, 2);
        assert!(build_log_filter(cli.verbose)
            .to_string()
            .contains("grat=trace"));
    }

    #[test]
    fn parses_long_verbose_flag_after_subcommand() {
        let cli = Cli::try_parse_from(["grat", "decode", "--verbose", &"a".repeat(64)])
            .expect("cli should parse");
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn parses_short_output_alias() {
        let cli = Cli::try_parse_from(["grat", "--output", "short", "decode", "abc123"])
            .expect("cli should parse");
        assert_eq!(cli.output, "short");
    }

    #[test]
    fn parses_trace_tx_hash_as_positional_argument() {
        let cli = Cli::try_parse_from(["grat", "trace", "abc123"]).expect("cli should parse");

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
        let cli = Cli::try_parse_from(["grat", "trace", "abc123", "--output-file", "trace.json"]).expect("cli should parse");

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
        let cli = Cli::try_parse_from(["grat", "diff", "deadbeef"]).expect("cli should parse");

        match cli.command {
            Commands::Diff(args) => assert_eq!(args.tx_hash, "deadbeef"),
            _ => panic!("expected diff command"),
        }
    }

    #[test]
    fn parses_save_flag_for_trace() {
        let tx_hash = "a".repeat(64);
        let cli = Cli::try_parse_from(["grat", "--save", "report.json", "trace", &tx_hash])
            .expect("cli should parse with --save");
        assert_eq!(cli.save.as_deref(), Some("report.json"));
    }

    #[test]
    fn save_flag_absent_by_default() {
        let cli = Cli::try_parse_from(["grat", "db", "update"]).expect("cli should parse");
        assert!(cli.save.is_none());
    }

    #[test]
    fn save_flag_can_appear_after_subcommand() {
        let tx_hash = "a".repeat(64);
        let cli = Cli::try_parse_from(["grat", "trace", &tx_hash, "--save", "out.json"])
            .expect("--save after subcommand should parse");
        assert_eq!(cli.save.as_deref(), Some("out.json"));
    }

    #[test]
    fn parses_global_no_cache_flag() {
        let cli = Cli::try_parse_from(["grat", "--no-cache", "decode", "abc123"])
            .expect("--no-cache before subcommand should parse");
        assert!(cli.no_cache);

        let cli = Cli::try_parse_from(["grat", "decode", "abc123", "--no-cache"])
            .expect("--no-cache after subcommand should parse");
        assert!(cli.no_cache);
    }

    #[test]
    fn defaults_to_warn_without_verbose() {
        let warn = build_log_filter(0).to_string();
        let debug = build_log_filter(1).to_string();
        let trace = build_log_filter(2).to_string();

        assert!(warn.contains("grat=warn"));
        assert!(debug.contains("grat=debug"));
        assert!(trace.contains("grat=trace"));
        assert!(trace.contains("grat_core=trace"));
    }

    #[test]
    fn version_string_includes_build_hash_and_protocol() {
        let version = build_version();

        assert!(version.contains(grat_core::VERSION));
        assert!(version.contains(BUILD_HASH));
        assert!(version.contains(&grat_core::SOROBAN_PROTOCOL_VERSION.to_string()));
    }
}