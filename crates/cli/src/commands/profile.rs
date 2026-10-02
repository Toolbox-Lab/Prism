use clap::Args;
use grat_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct ProfileArgs {
    pub tx_hash: String,
}

pub async fn run(
    args: ProfileArgs,
    network: &NetworkConfig,
    output_format: &str,
    save: Option<&str>,
) -> anyhow::Result<()> {
    let progress = indicatif::ProgressBar::new_spinner();
    progress.set_message("Replaying transaction for resource profiling...");
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

    let trace = grat_core::replay::replay_transaction(&args.tx_hash, network).await?;

    progress.finish_and_clear();

    crate::output::print_resource_profile(&trace.resource_profile, output_format)?;

    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&trace.resource_profile)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{path}': {e}"))?;
        eprintln!("Saved profile to {path}");
    }

    Ok(())
}
