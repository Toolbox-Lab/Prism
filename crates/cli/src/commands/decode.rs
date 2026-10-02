use clap::Args;
use grat_core::types::config::NetworkConfig;
use grat_core::types::report::{DiagnosticReport, Severity};

#[derive(Args)]
pub struct DecodeArgs {
    pub tx_hash: String,

    #[arg(long)]
    pub raw: bool,

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
        vec![build_raw_xdr_report(&args.tx_hash)?]
    } else {
        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_message(format!(
            "Fetching transaction {}...",
            &args.tx_hash[..8.min(args.tx_hash.len())]
        ));
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let reports =
            grat_core::decode::decode_transaction_with_op_filter(&args.tx_hash, network, None)
                .await?;
        spinner.finish_and_clear();
        reports
    };

    if !args.raw {
        if let Err(e) = crate::commands::history::append_to_history(&args.tx_hash) {
            eprintln!("Warning: failed to update command history: {e}");
        }
    }

    for (i, report) in reports.iter().enumerate() {
        if reports.len() > 1 {
            println!("\n=== Operation {} ===", i + 1);
        }
        crate::output::print_diagnostic_report(report, effective_output)?;
    }

    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&reports)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{path}': {e}"))?;
        eprintln!("Saved report to {path}");
    }

    Ok(())
}

fn build_raw_xdr_report(raw_xdr: &str) -> anyhow::Result<DiagnosticReport> {
    let bytes = grat_core::xdr::codec::decode_xdr_base64(raw_xdr)?;
    let mut report =
        DiagnosticReport::new("raw-xdr", 0, "RawXdr", "Decoded raw XDR input from --raw");
    report.severity = Severity::Info;
    report.detailed_explanation = format!(
        "Decoded {} bytes from the raw base64 XDR string provided on the command line.",
        bytes.len()
    );
    Ok(report)
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
