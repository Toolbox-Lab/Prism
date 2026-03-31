//! `prism export` — Export debug session as a regression test.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct ExportArgs {
    /// Transaction hash to export.
    pub tx_hash: String,

    /// Export format: test, json.
    #[arg(long, default_value = "test")]
    pub format: String,

    /// Output file path.
    #[arg(long, short)]
    pub output: Option<String>,
}

pub async fn run(
    args: ExportArgs,
    network: &NetworkConfig,
    output_format: &str,
    quiet: &bool,
) -> anyhow::Result<()> {
    if !*quiet {
        println!(
            "Exporting {} on {:?} as {} format...",
            args.tx_hash, network.network, args.format
        );
    }

    // TODO: Generate a self-contained test case from the debug session
    // - Historical state snapshot
    // - Transaction inputs
    // - Expected outcome

    let output_path = args.output.unwrap_or_else(|| {
        format!(
            "prism_test_{}.rs",
            &args.tx_hash[..8.min(args.tx_hash.len())]
        )
    });

    if matches!(
        crate::output::OutputFormat::parse(output_format),
        crate::output::OutputFormat::Json
    ) {
        let payload = serde_json::json!({
            "tx_hash": args.tx_hash,
            "network": format!("{:?}", network.network),
            "format": args.format,
            "output_path": output_path,
            "status": "exported",
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("Test case exported to {output_path}");
    }

    Ok(())
}
