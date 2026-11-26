use clap::Args;
use grat_core::debugger::whatif::{simulate_whatif, WhatIfPatch, WhatIfResult};
use grat_core::types::config::NetworkConfig;

#[derive(Args)]
pub struct WhatifArgs {
    pub tx_hash: String,

    /// JSON file containing the list of patches to apply before re-simulating.
    #[arg(long, value_name = "PATCH_FILE")]
    pub modify: Option<String>,
}

#[derive(serde::Serialize)]
struct WhatIfSavePayload<'a> {
    tx_hash: &'a str,
    patch_file: &'a str,
    patch_count: usize,
    patches: &'a [WhatIfPatch],
    result: &'a WhatIfResult,
}

pub async fn run(
    args: WhatifArgs,
    network: &NetworkConfig,
    output_format: &str,
    save: Option<&str>,
) -> anyhow::Result<()> {
    let Some(patch_file) = args.modify.as_deref() else {
        crate::output::print_whatif_status(&args.tx_hash, None, None, output_format)?;
        return Ok(());
    };

    let patches = load_patches(patch_file)?;

    let progress = indicatif::ProgressBar::new_spinner();
    progress.set_message(format!(
        "Re-simulating transaction with {} patch(es)...",
        patches.len()
    ));
    progress.enable_steady_tick(std::time::Duration::from_millis(100));

    let result = simulate_whatif(&args.tx_hash, &patches, network).await;
    progress.finish_and_clear();
    let result = result
        .map_err(|e| anyhow::anyhow!("What-if simulation failed for {}: {e}", args.tx_hash))?;

    crate::output::print_whatif_result(&result, patches.len(), output_format)?;

    if let Some(path) = save {
        let payload = WhatIfSavePayload {
            tx_hash: &args.tx_hash,
            patch_file,
            patch_count: patches.len(),
            patches: &patches,
            result: &result,
        };
        let json = serde_json::to_string_pretty(&payload)?;
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write save file '{path}': {e}"))?;
        eprintln!("Saved what-if results to {path}");
    }

    Ok(())
}

fn load_patches(patch_file: &str) -> anyhow::Result<Vec<WhatIfPatch>> {
    let content = std::fs::read_to_string(patch_file)
        .map_err(|e| anyhow::anyhow!("Failed to read patch file '{patch_file}': {e}"))?;
    let patches: Vec<WhatIfPatch> = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Invalid patch file '{patch_file}': {e}"))?;
    if patches.is_empty() {
        anyhow::bail!("Patch file '{patch_file}' contains no patches");
    }
    Ok(patches)
}

#[cfg(test)]
mod tests {
    use super::load_patches;
    use grat_core::debugger::whatif::WhatIfPatch;

    fn write_patch_file(content: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(file.path(), content).expect("write patch file");
        file
    }

    #[test]
    fn loads_every_patch_variant() {
        let file = write_patch_file(
            r#"[
                {"ModifyArgument": {"index": 0, "new_value": "100"}},
                {"ModifyLedgerEntry": {"key": "AAAA", "new_value": "BBBB"}},
                {"ModifyResourceLimits": {"cpu_limit": 1000, "memory_limit": null}},
                {"ModifyAuth": {"add_signer": "GABC", "remove_signer": null}}
            ]"#,
        );
        let patches = load_patches(file.path().to_str().unwrap()).unwrap();
        assert_eq!(patches.len(), 4);
        assert!(matches!(
            patches[0],
            WhatIfPatch::ModifyArgument { index: 0, .. }
        ));
    }

    #[test]
    fn reports_the_file_when_the_json_is_invalid() {
        let file = write_patch_file(r#"[{"ModifyArgument": {"index": "zero"}}]"#);
        let path = file.path().to_str().unwrap().to_string();
        let err = load_patches(&path).unwrap_err().to_string();
        assert!(
            err.starts_with(&format!("Invalid patch file '{path}'")),
            "{err}"
        );
    }

    #[test]
    fn reports_the_file_when_it_cannot_be_read() {
        let err = load_patches("does-not-exist.json").unwrap_err().to_string();
        assert!(
            err.starts_with("Failed to read patch file 'does-not-exist.json'"),
            "{err}"
        );
    }

    #[test]
    fn rejects_an_empty_patch_list() {
        let file = write_patch_file("[]");
        let err = load_patches(file.path().to_str().unwrap())
            .unwrap_err()
            .to_string();
        assert!(err.ends_with("contains no patches"), "{err}");
    }
}
