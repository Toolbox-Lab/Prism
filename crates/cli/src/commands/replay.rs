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

pub async fn run(args: ReplayArgs, network: &NetworkConfig) -> anyhow::Result<()> {
    if args.interactive {
        println!("Launching interactive TUI debugger for {}...", args.tx_hash);
        // TODO: Launch the ratatui TUI application
        crate::tui::app::launch(&args.tx_hash, network).await?;
    } else {
        println!("Use --interactive / -i to launch the TUI debugger.");
        println!(
            "Or use `prism trace {}` for non-interactive trace output.",
            args.tx_hash
        );
    }

    Ok(())
}
