//! `prism inspect` — Full transaction context inspection.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct InspectArgs {
    /// Transaction hash to inspect.
    #[arg(value_name = "TX_HASH")]
    pub tx_hash: String,

    /// Show detailed fee breakdown including bid vs charged values.
    #[arg(long)]
    pub fee_stats: bool,
}

pub async fn run(
    args: InspectArgs,
    network: &NetworkConfig,
    output_format: &str,
) -> anyhow::Result<()> {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message("Fetching and decoding transaction...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let report = prism_core::decode::decode_transaction(&args.tx_hash, network).await?;

    spinner.finish_and_clear();

    // Inspect shows the full context including decoded args, auth, resources, fees.
    crate::output::print_diagnostic_report(&report, output_format)?;

    Ok(())
}
