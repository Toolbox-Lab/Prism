use clap::Args;
use grat_core::decode::decode_context::{DecodeContextBuilder, OutputFormat};
use grat_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct InspectArgs {
    #[arg(value_name = "TX_HASH")]
    pub tx_hash: String,

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

    let reports = grat_core::decode::decode_transaction_with_op_filter(
        &args.tx_hash,
        &ctx.network,
        args.op_index,
    )
    .await?;

    spinner.finish_and_clear();

    for (i, report) in reports.iter().enumerate() {
        if reports.len() > 1 {
            println!("\n=== Operation {} ===", i + 1);
        }
        crate::output::print_diagnostic_report(report, output_format)?;
    }

    if let Some(path) = save {
        let json = serde_json::to_string_pretty(&reports)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{path}': {e}"))?;
        eprintln!("Saved report to {path}");
    }

    Ok(())
}
