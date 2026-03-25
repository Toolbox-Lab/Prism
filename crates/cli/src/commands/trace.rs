//! `prism trace` — Replay transaction and output execution trace.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct TraceArgs {
    /// Transaction hash to trace.
    pub tx_hash: String,

    /// Output trace to a file instead of stdout.
    #[arg(long, short)]
    pub output_file: Option<String>,
}

pub async fn run(
    args: TraceArgs,
    network: &NetworkConfig,
    output_format: &str,
) -> anyhow::Result<()> {
    let progress = indicatif::ProgressBar::new_spinner();
    progress.set_message("Reconstructing state and replaying transaction...");
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

    let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;

    progress.finish_and_clear();

    let output = crate::output::format_trace(&trace, output_format)?;

    if let Some(path) = args.output_file {
        std::fs::write(&path, &output)?;
        println!("Trace written to {path}");
    } else {
        println!("{output}");
    }

    Ok(())
}
