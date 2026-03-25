//! `prism whatif` — Re-simulate with modified inputs.

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

pub async fn run(args: WhatifArgs, network: &NetworkConfig, output_format: &str, quiet: &bool) -> anyhow::Result<()> {
    if !*quiet {
        println!("What-if simulation for {}", args.tx_hash);
    }

    if let Some(patch_file) = &args.modify {
        let patch_content = std::fs::read_to_string(patch_file)?;
        let _patches: Vec<prism_core::debugger::whatif::WhatIfPatch> =
            serde_json::from_str(&patch_content)?;
        // TODO: Run what-if simulation with patches
        if !*quiet {
            println!("Patches loaded from {patch_file}");
        }
    } else {
        if !*quiet {
            println!("No --modify file provided. Use a JSON patch file to specify modifications.");
        }
    }

    Ok(())
}
