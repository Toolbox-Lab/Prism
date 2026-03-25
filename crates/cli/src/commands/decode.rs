//! `prism decode` — Decode a transaction error into plain English.

use clap::Args;
use prism_core::types::config::NetworkConfig;

/// Arguments for the decode command.
#[derive(Args)]
pub struct DecodeArgs {
    /// Transaction hash to decode, or a raw error string with --raw.
    pub tx_hash: String,

    /// Decode a raw error string instead of fetching by TX hash.
    #[arg(long)]
    pub raw: bool,

    /// Show short one-line summary only.
    #[arg(long)]
    pub short: bool,
}

/// Execute the decode command.
pub async fn run(args: DecodeArgs, network: &NetworkConfig, output_format: &str, quiet: &bool) -> anyhow::Result<()> {
    if args.raw {
        if !*quiet {
            println!("Decoding raw error string: {}", args.tx_hash);
        }
        // TODO: Parse raw error string and decode
        return Ok(());
    }

    if !*quiet {
        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_message(format!("Fetching transaction {}...", &args.tx_hash[..8.min(args.tx_hash.len())]));
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let report = prism_core::decode::decode_transaction(&args.tx_hash, network).await?;

        spinner.finish_and_clear();
    } else {
        let report = prism_core::decode::decode_transaction(&args.tx_hash, network).await?;
    }

    match output_format {
        "json" => crate::output::json::print_report(&report)?,
        "compact" => crate::output::compact::print_report(&report)?,
        _ => crate::output::human::print_report(&report)?,
    }

    Ok(())
}
