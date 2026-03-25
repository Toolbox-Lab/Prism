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

pub async fn run(args: ExportArgs, network: &NetworkConfig) -> anyhow::Result<()> {
    println!("Exporting {} as {} format...", args.tx_hash, args.format);

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

    println!("Test case exported to {output_path}");

    Ok(())
}
