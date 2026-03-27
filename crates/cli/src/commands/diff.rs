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
) -> anyhow::Result<()> {
    let progress = indicatif::ProgressBar::new_spinner();
    progress.set_message("Computing state diff...");
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

        let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;

        progress.finish_and_clear();
    } else {
        let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;
    }

    // --- Terminal output (always shown) ---
    match output_format {
        "json" => println!("{}", serde_json::to_string_pretty(&trace.state_diff)?),
        _ => {
            if !*quiet {
                println!("{}", colored::Colorize::bold("State Diff"));
            }
            for entry in &trace.state_diff.entries {
                let symbol = match entry.change_type {
                    prism_core::types::trace::DiffChangeType::Created => {
                        colored::Colorize::green("+")
                    }
                    prism_core::types::trace::DiffChangeType::Deleted => {
                        colored::Colorize::red("-")
                    }
                    prism_core::types::trace::DiffChangeType::Updated => {
                        colored::Colorize::yellow("~")
                    }
                    prism_core::types::trace::DiffChangeType::Unchanged => {
                        colored::Colorize::dimmed(" ")
                    }
                };
                println!("{symbol} {}", entry.key);
            }
        }
    }

    // --- Optional JSON save (--save flag) ---
    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&trace.state_diff)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{}': {}", path, e))?;
        eprintln!("Saved diff to {path}");
    }

    Ok(())
}
