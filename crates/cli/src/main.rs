mod commands;
mod config;
mod output;
mod tui;
mod ui;
mod version_check;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};
use std::path::PathBuf;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use url::Url;

const BUILD_HASH: &str = env!("GRAT_BUILD_HASH");

const COMPLETIONS_HELP: &str = "\
Examples:
  grat completions bash > ~/.local/share/bash-completion/completions/grat
  grat completions zsh > ~/.zfunc/_grat
  grat completions fish > ~/.config/fish/completions/grat.fish
  grat completions powershell | Out-String | Invoke-Expression
  grat completions elvish > ~/.config/elvish/lib/grat.elv";

#[derive(Parser)]
#[command(name = "grat", version = env!("CARGO_PKG_VERSION"), about, long_about = None)]
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

    /// Network to use (defaults to the config file's default network, then testnet)
    #[arg(
        long,
        short,
        value_parser = ["testnet", "mainnet", "futurenet", "standalone"],
        global = true
    )]
    network: Option<String>,

    #[arg(long, short, action = ArgAction::Count, global = true)]
    verbose: u8,

    #[arg(long, global = true, value_parser = validate_url)]
    rpc_url: Option<String>,

    #[arg(long, global = true, value_name = "PATH")]
    save: Option<String>,

    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Path to a custom config file (overrides ~/.grat/config.toml)"
    )]
    config_path: Option<PathBuf>,

    #[arg(long, short, global = true)]
    quiet: bool,

    #[arg(long, global = true)]
    no_color: bool,

    #[arg(long, global = true, help = "Disable network requests for updates")]
    offline: bool,

    #[arg(
        long,
        global = true,
        help = "Bypass local cache and always fetch live network data"
    )]
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

    /// Generate shell completion scripts and print them to stdout.
    #[command(after_help = COMPLETIONS_HELP)]
    Completions {
        /// Shell to generate completions for.
        shell: clap_complete::Shell,
    },

    Serve(commands::serve::ServeArgs),

    SearchError(commands::search_error::SearchErrorArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let version: &'static str = Box::leak(build_version().into_boxed_str());
    let matches = Cli::command().version(version).get_matches();
    let cli = Cli::from_arg_matches(&matches)?;

    // Completion scripts are consumed by shells (`source <(grat completions bash)`),
    // so emit them before any background network work, config loading or logging.
    if let Commands::Completions { shell } = cli.command {
        return write_completions(shell, &mut std::io::stdout().lock());
    }

    let _update_check_handle = tokio::spawn(version_check::check_for_updates());
    let _taxonomy_update_handle =
        tokio::spawn(grat_core::taxonomy::updater::check_and_update(cli.offline));
    let loaded_config = config::load_config(cli.config_path.as_deref())?;

    tracing_subscriber::fmt()
        .with_env_filter(build_log_filter(cli.verbose))
        .with_writer(std::io::stderr)
        .with_file(cli.verbose > 1)
        .with_line_number(cli.verbose > 1)
        .with_thread_ids(cli.verbose > 1)
        .init();

    tracing::debug!(
        output = %cli.output,
        network_arg = ?cli.network,
        verbose = cli.verbose,
        no_color = cli.no_color,
        config_default_network = %loaded_config.default_network.as_key(),
        "CLI arguments parsed"
    );

    output::theme::set_color_enabled(output::theme::resolve_color_enabled(cli.no_color));

    let network_name = config::effective_network_name(cli.network.as_deref(), &loaded_config);
    let mut network = config::resolve_network_config(&network_name, &loaded_config);
    if let Some(ref rpc_url) = cli.rpc_url {
        network.rpc_url = rpc_url.clone();
    }
    network.no_cache = cli.no_cache;

    tracing::debug!(
        resolved_network = ?network.network,
        rpc_url = %network.rpc_url,
        archive_url_count = network.archive_urls.len(),
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
        Commands::Db(args) => commands::db::run(args, &cli.output, cli.offline).await?,
        Commands::Auth(args) => commands::auth::run(args, &cli.output).await?,
        Commands::Diagnostic(args) => commands::diagnostic::run(args).await?,
        Commands::History => commands::history::run(&cli.output).await?,
        Commands::Serve(args) => commands::serve::run(args, &network).await?,
        Commands::SearchError(args) => commands::search_error::run(args, &cli.output).await?,
        Commands::Completions { .. } => unreachable!("handled before runtime setup"),
    }

    Ok(())
}

/// Renders the completion script for `shell` and writes it to `out`.
///
/// The script is generated into memory first because `clap_complete::generate`
/// panics on write errors; a closed pipe (`grat completions bash | head`) is
/// treated as a normal early exit instead.
fn write_completions(
    shell: clap_complete::Shell,
    out: &mut dyn std::io::Write,
) -> anyhow::Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();

    let mut script = Vec::new();
    clap_complete::generate(shell, &mut cmd, name, &mut script);

    match out.write_all(&script).and_then(|()| out.flush()) {
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
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
        let cli = Cli::try_parse_from(["grat", "trace", "abc123", "--output-file", "trace.json"])
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
    fn parses_config_path_flag() {
        let cli = Cli::try_parse_from([
            "grat",
            "--config-path",
            "/tmp/custom_config.toml",
            "decode",
            "abc123",
        ])
        .expect("cli should parse with --config-path");
        assert_eq!(
            cli.config_path,
            Some(PathBuf::from("/tmp/custom_config.toml"))
        );
    }

    #[test]
    fn config_path_absent_by_default() {
        let cli = Cli::try_parse_from(["grat", "db", "update"]).expect("cli should parse");
        assert!(cli.config_path.is_none());
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

    #[test]
    fn parses_no_cache_flag_on_decode() {
        let cli = Cli::try_parse_from(["grat", "decode", "abc123", "--no-cache"])
            .expect("cli should parse with --no-cache");
        assert!(cli.no_cache);
    }

    #[test]
    fn network_defaults_to_none_so_config_can_supply_it() {
        let cli = Cli::try_parse_from(["grat", "db", "update"]).expect("cli should parse");
        assert!(cli.network.is_none());
    }

    #[test]
    fn accepts_all_documented_networks() {
        for name in ["testnet", "mainnet", "futurenet", "standalone"] {
            let cli = Cli::try_parse_from(["grat", "--network", name, "db", "update"])
                .expect("network should parse");
            assert_eq!(cli.network.as_deref(), Some(name));
        }
    }

    #[test]
    fn rejects_unknown_network() {
        assert!(Cli::try_parse_from(["grat", "--network", "bogus", "db", "update"]).is_err());
    }

    #[test]
    fn rejects_unknown_output_format() {
        assert!(Cli::try_parse_from(["grat", "--output", "xml", "db", "update"]).is_err());
    }

    #[test]
    fn parses_rpc_url_and_rejects_invalid_url() {
        let cli =
            Cli::try_parse_from(["grat", "--rpc-url", "http://localhost:9000", "db", "update"])
                .expect("cli should parse");
        assert_eq!(cli.rpc_url.as_deref(), Some("http://localhost:9000"));
        assert!(Cli::try_parse_from(["grat", "--rpc-url", "not a url", "db", "update"]).is_err());
    }

    #[test]
    fn parses_no_color_and_offline_flags() {
        let cli = Cli::try_parse_from(["grat", "--no-color", "--offline", "db", "update"])
            .expect("cli should parse");
        assert!(cli.no_color);
        assert!(cli.offline);
    }

    #[test]
    fn no_cache_absent_by_default() {
        let cli = Cli::try_parse_from(["grat", "decode", "abc123"]).expect("cli should parse");
        assert!(!cli.no_cache);
    }

    fn completion_script(shell: clap_complete::Shell) -> String {
        let mut out = Vec::new();
        write_completions(shell, &mut out).expect("completions should be generated");
        String::from_utf8(out).expect("completion script should be valid UTF-8")
    }

    #[test]
    fn parses_completions_shell_argument() {
        for (name, expected) in [
            ("bash", clap_complete::Shell::Bash),
            ("elvish", clap_complete::Shell::Elvish),
            ("fish", clap_complete::Shell::Fish),
            ("powershell", clap_complete::Shell::PowerShell),
            ("zsh", clap_complete::Shell::Zsh),
        ] {
            let cli = Cli::try_parse_from(["grat", "completions", name]).expect("should parse");
            match cli.command {
                Commands::Completions { shell } => assert_eq!(shell, expected),
                _ => panic!("expected completions command"),
            }
        }
    }

    #[test]
    fn completions_rejects_unknown_shell() {
        assert!(Cli::try_parse_from(["grat", "completions", "tcsh"]).is_err());
        assert!(Cli::try_parse_from(["grat", "completions"]).is_err());
    }

    #[test]
    fn completions_are_generated_for_every_supported_shell() {
        use clap::ValueEnum;

        for shell in clap_complete::Shell::value_variants() {
            let script = completion_script(*shell);
            assert!(!script.trim().is_empty(), "{shell} script is empty");
            assert!(
                script.contains("grat"),
                "{shell} script never mentions grat"
            );
        }
    }

    #[test]
    fn completions_cover_every_subcommand_and_global_flag() {
        use clap::ValueEnum;

        let cmd = Cli::command();
        let subcommands: Vec<String> = cmd
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set())
            .map(|sub| sub.get_name().to_string())
            .collect();
        let global_flags: Vec<String> = cmd
            .get_arguments()
            .filter(|arg| arg.is_global_set())
            .filter_map(|arg| arg.get_long().map(str::to_string))
            .collect();

        assert!(subcommands.iter().any(|name| name == "completions"));
        assert!(global_flags.iter().any(|flag| flag == "output"));
        assert!(global_flags.iter().any(|flag| flag == "network"));

        for shell in clap_complete::Shell::value_variants() {
            let script = completion_script(*shell);
            for name in &subcommands {
                assert!(
                    script.contains(name.as_str()),
                    "{shell}: missing subcommand `{name}`"
                );
            }
            for flag in &global_flags {
                assert!(
                    script.contains(flag.as_str()),
                    "{shell}: missing global flag `--{flag}`"
                );
            }
        }
    }

    #[test]
    fn completions_tolerate_a_closed_pipe() {
        struct ClosedPipe;

        impl std::io::Write for ClosedPipe {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        write_completions(clap_complete::Shell::Bash, &mut ClosedPipe)
            .expect("a closed pipe should not be an error");
    }
}
