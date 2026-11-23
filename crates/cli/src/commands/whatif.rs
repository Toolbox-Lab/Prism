use clap::Args;
use grat_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct WhatifArgs {
    pub tx_hash: String,

    #[arg(long)]
    pub modify: Option<String>,
}

pub async fn run(
    args: WhatifArgs,
    network: &NetworkConfig,
    output_format: &str,
    save: Option<&str>,
) -> anyhow::Result<()> {
    let mut divergence_point: Option<usize> = None;
    let mut simulation_error: Option<String> = None;

    let patches = if let Some(patch_file) = &args.modify {
        let patch_content = std::fs::read_to_string(patch_file)
            .map_err(|e| anyhow::anyhow!("Failed to read patch file '{patch_file}': {e}"))?;
        let patches: Vec<grat_core::debugger::whatif::WhatIfPatch> =
            serde_json::from_str(&patch_content)
                .map_err(|e| anyhow::anyhow!("Invalid patch file '{patch_file}': {e}"))?;

        match grat_core::debugger::whatif::simulate_whatif(&args.tx_hash, &patches, network).await
        {
            Ok(result) => divergence_point = result.divergence_point,
            Err(e) => simulation_error = Some(e.to_string()),
        }

        crate::output::print_whatif_status(
            &args.tx_hash,
            Some(patch_file),
            Some(patches.len()),
            divergence_point,
            output_format,
        )?;
        Some(patches)
    } else {
        crate::output::print_whatif_status(&args.tx_hash, None, None, None, output_format)?;
        None
    };

    if let Some(err) = &simulation_error {
        eprintln!("Warning: simulation did not complete: {err}");
    }

    if let Some(path) = save {
        #[derive(serde::Serialize)]
        struct WhatIfSavePayload<'a> {
            tx_hash: &'a str,
            patch_file: Option<&'a str>,
            patch_count: Option<usize>,
            divergence_point: Option<usize>,
            simulation_error: Option<&'a str>,
            patches: &'a Option<Vec<grat_core::debugger::whatif::WhatIfPatch>>,
        }

        let payload = WhatIfSavePayload {
            tx_hash: &args.tx_hash,
            patch_file: args.modify.as_deref(),
            patch_count: patches.as_ref().map(std::vec::Vec::len),
            divergence_point,
            simulation_error: simulation_error.as_deref(),
            patches: &patches,
        };

        let json = serde_json::to_string_pretty(&payload)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{path}': {e}"))?;
        eprintln!("Saved what-if session to {path}");
    }

    Ok(())
}
