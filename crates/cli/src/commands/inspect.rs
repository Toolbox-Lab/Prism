//! `prism inspect` — Full transaction context inspection.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct InspectArgs {
    /// Transaction hash to inspect.
    pub tx_hash: String,
}

pub async fn run(args: InspectArgs, network: &NetworkConfig, output_format: &str, quiet: &bool) -> anyhow::Result<()> {
    if !*quiet {
        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_message("Fetching and decoding transaction...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let report = prism_core::decode::decode_transaction(&args.tx_hash, network).await?;

        spinner.finish_and_clear();
    } else {
        let report = prism_core::decode::decode_transaction(&args.tx_hash, network).await?;
    }

    // Inspect shows the full context including decoded args, auth, resources, fees
    match output_format {
        "json" => crate::output::json::print_report(&report)?,
        _ => crate::output::human::print_report(&report)?,
    }

    Ok(())
}
