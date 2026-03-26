//! `prism profile` - Resource consumption profile with hotspot analysis.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct ProfileArgs {
    /// Transaction hash to profile.
    pub tx_hash: String,

    /// Output profile to a file instead of stdout.
    #[arg(long, short)]
    pub output_file: Option<String>,
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

        let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;

        progress.finish_and_clear();
    } else {
        let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;
    }

    // --- Terminal output (always shown) ---
    match output_format {
        "json" => println!("{}", serde_json::to_string_pretty(&trace.resource_profile)?),
        _ => {
            println!("{}", colored::Colorize::bold("Resource Profile"));
            println!(
                "CPU: {}/{} instructions",
                trace.resource_profile.total_cpu, trace.resource_profile.cpu_limit
            );
            println!(
                "Memory: {}/{} bytes",
                trace.resource_profile.total_memory, trace.resource_profile.memory_limit
            );
            for warning in &trace.resource_profile.warnings {
                println!("{} {warning}", colored::Colorize::yellow("⚠"));
            }
        }
    }

    // --- Optional JSON save (--save flag) ---
    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&trace.resource_profile)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{}': {}", path, e))?;
        eprintln!("Saved profile to {path}");
    }

    Ok(())
}
