//! `prism trace` — Replay transaction and output execution trace.

use clap::Args;
use prism_core::types::config::NetworkConfig;
use crate::output::trace_tree;

#[derive(Args)]
pub struct TraceArgs {
    /// Transaction hash to trace.
    pub tx_hash: String,

    /// Output trace to a file instead of stdout.
    #[arg(long, short)]
    pub output_file: Option<String>,

    /// Show authorization tree view.
    #[arg(long)]
    pub auth: bool,

    /// Show only authorization structure (no resource details).
    #[arg(long)]
    pub auth_only: bool,
}

pub async fn run(
    args: TraceArgs,
    network: &NetworkConfig,
    output_format: &str,
    save: Option<&str>,
) -> anyhow::Result<()> {
    let progress = indicatif::ProgressBar::new_spinner();
    progress.set_message("Reconstructing state and replaying transaction...");
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

        let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;

        progress.finish_and_clear();
    } else {
        let trace = prism_core::replay::replay_transaction(&args.tx_hash, network).await?;
    }

    // --- Terminal output (always shown) ---
    let output = if args.auth || args.auth_only {
        if args.auth_only {
            crate::output::auth_tree::render_auth_only(&trace)?
        } else {
            crate::output::auth_tree::render_auth_tree(&trace)?
        }
    } else {
        crate::output::format_trace(&trace, output_format)?
    };

    if let Some(path) = args.output_file {
        std::fs::write(&path, &output)?;
        if !*quiet {
            println!("Trace written to {path}");
        }
    } else {
        println!("{output}");
    }

    // --- Optional JSON save (--save flag) ---
    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&trace)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{}': {}", path, e))?;
        eprintln!("Saved trace to {path}");
    }

    Ok(())
}
