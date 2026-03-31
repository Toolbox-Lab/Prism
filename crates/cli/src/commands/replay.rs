//! `prism replay` — Launch interactive TUI debugger.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct ReplayArgs {
    /// Transaction hash to replay interactively.
    pub tx_hash: String,

    /// Enable interactive mode (TUI).
    #[arg(long, short)]
    pub interactive: bool,
}

pub async fn run(
    args: ReplayArgs,
    network: &NetworkConfig,
    output_format: &str,
    quiet: &bool,
) -> anyhow::Result<()> {
    if args.interactive {
        if matches!(
            crate::output::OutputFormat::parse(output_format),
            crate::output::OutputFormat::Json
        ) {
            let payload = serde_json::json!({
                "tx_hash": args.tx_hash,
                "interactive": true,
                "status": "launching_tui",
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            return Ok(());
        }

        if !*quiet {
            println!("Launching interactive TUI debugger for {}...", args.tx_hash);
        }
        crate::tui::app::launch(&args.tx_hash, network).await?;
    } else {
        if matches!(
            crate::output::OutputFormat::parse(output_format),
            crate::output::OutputFormat::Json
        ) {
            let payload = serde_json::json!({
                "tx_hash": args.tx_hash,
                "interactive": false,
                "status": "interactive_mode_required",
                "hint": "Use --interactive / -i to launch the TUI debugger.",
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            return Ok(());
        }

        if !*quiet {
            println!("Use --interactive / -i to launch the TUI debugger.");
            println!(
                "Or use `prism trace {}` for non-interactive trace output.",
                args.tx_hash
            );
        }
    }

    Ok(())
}
