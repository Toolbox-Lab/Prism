//! `prism diff` - Show state diff (before/after) for a transaction.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct DiffArgs {
    /// Transaction hash to diff.
    #[arg(value_name = "TX_HASH")]
    pub tx_hash: String,
}

pub async fn run(
    args: DiffArgs,
    network: &NetworkConfig,
    output_format: &str,
    save: Option<&str>,
) -> anyhow::Result<()> {
    let progress = indicatif::ProgressBar::new_spinner();
    progress.set_message("Computing state diff...");
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

    let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;

    progress.finish_and_clear();

    crate::output::print_state_diff(&trace.state_diff, output_format)?;

    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&trace.state_diff)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{}': {}", path, e))?;
        eprintln!("Saved diff to {path}");
    }
    Ok(())
}
