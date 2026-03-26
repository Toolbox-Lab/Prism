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
    save: Option<&str>,
) -> anyhow::Result<()> {
    let patches = if let Some(patch_file) = &args.modify {
        let patch_content = std::fs::read_to_string(patch_file)?;
        let patches: Vec<prism_core::debugger::whatif::WhatIfPatch> =
            serde_json::from_str(&patch_content)?;
        crate::output::print_whatif_status(
            &args.tx_hash,
            Some(patch_file),
            Some(patches.len()),
            output_format,
        )?;
        Some(patches)
    } else {
        crate::output::print_whatif_status(&args.tx_hash, None, None, output_format)?;
        None
    };

    // --- Optional JSON save (--save flag) ---
    if let Some(path) = save {
        #[derive(serde::Serialize)]
        struct WhatIfSavePayload<'a> {
            tx_hash: &'a str,
            patch_file: Option<&'a str>,
            patch_count: Option<usize>,
            patches: &'a Option<Vec<prism_core::debugger::whatif::WhatIfPatch>>,
        }

        let payload = WhatIfSavePayload {
            tx_hash: &args.tx_hash,
            patch_file: args.modify.as_deref(),
            patch_count: patches.as_ref().map(|p| p.len()),
            patches: &patches,
        };

        let json = serde_json::to_string_pretty(&payload)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{}': {}", path, e))?;
        eprintln!("Saved what-if session to {path}");
    }

    Ok(())
}
