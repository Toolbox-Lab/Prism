use clap::Args;
use grat_core::types::config::NetworkConfig;
use grat_core::types::report::DiagnosticReport;

#[derive(Args)]
pub struct DecodeArgs {
    /// Transaction hash to look up, or base64 XDR with --raw (the envelope,
    /// result and/or result meta, separated by spaces or commas)
    pub tx_hash: String,

    /// Treat the input as raw base64 XDR and decode it offline, without any
    /// network calls
    #[arg(long)]
    pub raw: bool,

    /// Print a one-line summary per operation (same as --output short)
    #[arg(long)]
    pub short: bool,
}

pub async fn run(
    args: DecodeArgs,
    network: &NetworkConfig,
    output_format: &str,
    save: Option<&str>,
) -> anyhow::Result<()> {
    let effective_output = if args.short { "short" } else { output_format };

    let reports = if args.raw {
        decode_raw(&args.tx_hash)?
    } else {
        let reports = fetch_and_decode(&args.tx_hash, network).await?;
        if let Err(e) = crate::commands::history::append_to_history(&args.tx_hash) {
            eprintln!("Warning: failed to update command history: {e}");
        }
        reports
    };

    for (i, report) in reports.iter().enumerate() {
        if reports.len() > 1 {
            println!("\n=== Operation {} ===", i + 1);
        }
        crate::output::print_diagnostic_report(report, effective_output)?;
    }

    if let Some(path) = save {
        save_reports(&reports, path)?;
        eprintln!("Saved report to {path}");
    }

    Ok(())
}

/// Fetch the transaction over RPC and decode it, with a spinner while waiting.
async fn fetch_and_decode(
    tx_hash: &str,
    network: &NetworkConfig,
) -> anyhow::Result<Vec<DiagnosticReport>> {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message(format!(
        "Fetching transaction {}...",
        tx_hash.get(..8).unwrap_or(tx_hash)
    ));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let result = grat_core::decode::decode_transaction_with_op_filter(tx_hash, network, None).await;
    // Clear the spinner whether the lookup succeeded or not, so an error
    // message is not printed over a half-drawn spinner line.
    spinner.finish_and_clear();
    Ok(result?)
}

/// Decode raw base64 XDR locally. No network calls are made.
fn decode_raw(input: &str) -> anyhow::Result<Vec<DiagnosticReport>> {
    grat_core::decode::raw_xdr::decode_raw_xdr(input)
        .map_err(|e| anyhow::anyhow!("Could not decode raw XDR: {e}"))
}

/// Write the reports as pretty-printed JSON, creating parent folders if needed.
fn save_reports(reports: &[DiagnosticReport], path: &str) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(reports)?;
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                anyhow::anyhow!("Failed to create folder '{}': {e}", parent.display())
            })?;
        }
    }
    std::fs::write(path, json)
        .map_err(|e| anyhow::anyhow!("Failed to write save file '{path}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::{decode_raw, save_reports};

    #[test]
    fn raw_input_that_is_not_xdr_is_rejected_with_context() {
        let message = decode_raw("AAAA").unwrap_err().to_string();

        assert!(message.starts_with("Could not decode raw XDR"), "{message}");
        assert!(message.contains("not a TransactionEnvelope"), "{message}");
    }

    #[test]
    fn raw_input_that_is_not_base64_is_rejected() {
        let message = decode_raw("%%%").unwrap_err().to_string();

        assert!(message.contains("Base64"), "{message}");
    }

    #[test]
    fn save_creates_missing_parent_folders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("nested")
            .join("reports")
            .join("decode.json");
        let report = grat_core::types::report::DiagnosticReport::new(
            "Transaction",
            0,
            "TxBadSeq",
            "Transaction failed with TxBadSeq",
        );

        save_reports(&[report], path.to_str().unwrap()).unwrap();

        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved[0]["error_name"], "TxBadSeq");
    }
}
