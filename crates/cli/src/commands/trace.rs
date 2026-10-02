use clap::Args;
use grat_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct TraceArgs {
    #[arg(index = 1, value_name = "TX_HASH")]
    pub tx_hash: String,

    #[arg(long, short)]
    pub output_file: Option<String>,

    #[arg(long)]
    pub auth: bool,

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

    let trace = grat_core::replay::replay_transaction(&args.tx_hash, network).await?;

    progress.finish_and_clear();

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
        println!("Trace written to {path}");
    } else {
        println!("{output}");
    }

    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&trace)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{path}': {e}"))?;
        eprintln!("Saved trace to {path}");
    }
    Ok(())
}
