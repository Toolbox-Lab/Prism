//! `prism profile` - Resource consumption profile with hotspot analysis.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct ProfileArgs {
    /// Transaction hash to profile.
    pub tx_hash: String,
}

pub async fn run(
    args: ProfileArgs,
    network: &NetworkConfig,
    output_format: &str,
) -> anyhow::Result<()> {
    let progress = indicatif::ProgressBar::new_spinner();
    progress.set_message("Replaying transaction for resource profiling...");
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

    let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;

    progress.finish_and_clear();

    crate::output::print_resource_profile(&trace.resource_profile, output_format)?;

    Ok(())
}
