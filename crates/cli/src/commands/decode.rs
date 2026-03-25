//! `prism decode` — Decode a transaction error into plain English.

use clap::Args;
use prism_core::types::config::NetworkConfig;
use prism_core::types::report::{DiagnosticReport, Severity};

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
pub async fn run(
    args: DecodeArgs,
    network: &NetworkConfig,
    output_format: &str,
) -> anyhow::Result<()> {
    if args.raw {
        let report = build_raw_xdr_report(&args.tx_hash)?;
        print_report(&report, output_format)?;
        return Ok(());
    }

    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message(format!(
        "Fetching transaction {}...",
        &args.tx_hash[..8.min(args.tx_hash.len())]
    ));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let report = prism_core::decode::decode_transaction(&args.tx_hash, network).await?;

    spinner.finish_and_clear();

    let effective_output = if args.short { "short" } else { output_format };
    crate::output::print_diagnostic_report(&report, effective_output)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_raw_xdr_report;

    #[test]
    fn raw_xdr_input_builds_a_local_report() {
        let report = build_raw_xdr_report("AAAA").expect("raw XDR should decode");

        assert_eq!(report.error_category, "raw-xdr");
        assert_eq!(report.error_name, "RawXdr");
        assert_eq!(report.summary, "Decoded raw XDR input from --raw");
        assert!(report.detailed_explanation.contains("3 bytes"));
    }
}
