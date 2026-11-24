use clap::Args;
use grat_core::decode::decode_context::{DecodeContextBuilder, OutputFormat};
use grat_core::types::config::NetworkConfig;
use grat_core::types::report::DiagnosticReport;

#[derive(Args)]
pub struct InspectArgs {
    /// Transaction hash to inspect
    #[arg(value_name = "TX_HASH")]
    pub tx_hash: String,

    /// Zero-based index of a specific operation to inspect
    #[arg(long)]
    pub op_index: Option<usize>,
}

pub async fn run(
    args: InspectArgs,
    network: &NetworkConfig,
    output_format: &str,
    save: Option<&str>,
) -> anyhow::Result<()> {
    let ctx = DecodeContextBuilder::from(network)
        .output_format(OutputFormat::parse(output_format))
        .build();

    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message("Fetching and decoding transaction...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let result = grat_core::decode::decode_transaction_with_op_filter(
        &args.tx_hash,
        &ctx.network,
        args.op_index,
    )
    .await;

    spinner.finish_and_clear();
    let reports = result?;

    crate::output::print_diagnostic_reports(&reports, ctx.output_format.as_str())?;

    if let Some(path) = save {
        save_reports(&reports, path)?;
        eprintln!("Saved report to {path}");
    }

    Ok(())
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
    use super::save_reports;

    #[test]
    fn save_reports_writes_pretty_json_and_creates_parent_folders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("nested")
            .join("reports")
            .join("inspect.json");
        let report = grat_core::types::report::DiagnosticReport::new(
            "Soroban",
            0,
            "Trapped",
            "Contract trapped during execution",
        );

        save_reports(&[report], path.to_str().unwrap()).unwrap();

        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved[0]["error_name"], "Trapped");
    }
}
