//! `prism whatif` - Re-simulate with modified inputs.

use clap::Args;
use prism_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct WhatifArgs {
    /// Transaction hash to re-simulate.
    pub tx_hash: String,

    /// Path to a JSON patch file with modifications.
    #[arg(long)]
    pub modify: Option<String>,
}

pub async fn run(
    args: WhatifArgs,
    network: &NetworkConfig,
    output_format: &str,
) -> anyhow::Result<()> {
    println!("What-if simulation for {}", args.tx_hash);

    if let Some(patch_file) = &args.modify {
        let patch_content = std::fs::read_to_string(patch_file)?;
        let patches: Vec<prism_core::debugger::whatif::WhatIfPatch> =
            serde_json::from_str(&patch_content)?;
        // TODO: Run what-if simulation with patches
        crate::output::print_whatif_status(
            &args.tx_hash,
            Some(patch_file),
            Some(patches.len()),
            output_format,
        )?;
    } else {
        crate::output::print_whatif_status(&args.tx_hash, None, None, output_format)?;
    }

    Ok(())
}
