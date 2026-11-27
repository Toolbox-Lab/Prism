use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use clap::Args;
use grat_core::replay::export::{
    default_file_name, normalize_tx_hash, render_test_case, ExpectedOutcome, ExportedTestCase,
    TestCaseSpec,
};
use grat_core::rpc::client::TransactionStatus;
use grat_core::rpc::SorobanRpcClient;
use grat_core::types::config::NetworkConfig;

/// Export a mainnet/testnet transaction as a reproducible Rust test case.
#[derive(Args)]
pub struct ExportArgs {
    /// Hash of the transaction to export (64 hex characters).
    pub tx_hash: String,

    /// Export format. `test` writes a standalone Rust integration test.
    #[arg(long, default_value = "test", value_parser = ["test"])]
    pub format: String,

    /// File (or existing directory) to write to. Defaults to
    /// `grat_test_<hash>.rs` in the current directory.
    #[arg(long, short = 'o', value_name = "PATH")]
    pub out: Option<String>,
}

pub async fn run(
    args: ExportArgs,
    network: &NetworkConfig,
    output_format: &str,
    quiet: &bool,
) -> anyhow::Result<()> {
    let tx_hash = normalize_tx_hash(&args.tx_hash)?;
    let json = matches!(
        crate::output::OutputFormat::parse(output_format),
        crate::output::OutputFormat::Json
    );

    if !*quiet && !json {
        eprintln!(
            "Exporting {tx_hash} on {} as a {} case...",
            network.network, args.format
        );
    }

    let rpc = SorobanRpcClient::new(network);
    let tx = rpc
        .get_transaction(&tx_hash)
        .await
        .with_context(|| format!("failed to fetch transaction {tx_hash}"))?;
    let expected = expected_outcome(&tx.status, &tx_hash, &network.network.to_string())?;

    let state = grat_core::replay::state::reconstruct_state(&tx_hash, network)
        .await
        .with_context(|| format!("failed to reconstruct ledger state for {tx_hash}"))?;

    let network_name = network.network.to_string();
    let exported = render_test_case(&TestCaseSpec {
        tx_hash: &tx_hash,
        network: &network_name,
        expected,
        state: &state,
    })?;

    let path = resolve_output_path(args.out.as_deref(), &tx_hash);
    write_test_file(&path, &exported.source)?;

    if json {
        let payload = export_metadata(&exported, &args.format, &path);
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if !*quiet {
        println!(
            "Test case exported to {} ({} ledger entries, on-chain outcome: {})",
            path.display(),
            exported.ledger_entries,
            outcome_label(exported.expected_outcome)
        );
        println!(
            "Run it with: cargo test {} -- --nocapture",
            exported.test_name
        );
    }

    Ok(())
}

fn expected_outcome(
    status: &TransactionStatus,
    tx_hash: &str,
    network: &str,
) -> anyhow::Result<ExpectedOutcome> {
    match status {
        TransactionStatus::Success => Ok(ExpectedOutcome::Success),
        TransactionStatus::Failed => Ok(ExpectedOutcome::Failure),
        TransactionStatus::NotFound => {
            bail!("transaction {tx_hash} was not found on {network} (it may be outside the RPC retention window)")
        }
    }
}

fn outcome_label(outcome: ExpectedOutcome) -> &'static str {
    match outcome {
        ExpectedOutcome::Success => "success",
        ExpectedOutcome::Failure => "failure",
    }
}

/// `None` → `grat_test_<hash>.rs`; an existing directory → that file inside it;
/// anything else is used as the file path as given.
pub(crate) fn resolve_output_path(output: Option<&str>, tx_hash: &str) -> PathBuf {
    let file_name = default_file_name(tx_hash);
    match output {
        None => PathBuf::from(file_name),
        Some(p) => {
            let path = PathBuf::from(p);
            if path.is_dir() {
                path.join(file_name)
            } else {
                path
            }
        }
    }
}

fn write_test_file(path: &Path, source: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, source).with_context(|| format!("failed to write {}", path.display()))
}

pub(crate) fn export_metadata(
    exported: &ExportedTestCase,
    format: &str,
    path: &Path,
) -> serde_json::Value {
    serde_json::json!({
        "status": "exported",
        "tx_hash": exported.tx_hash,
        "network": exported.network,
        "ledger_sequence": exported.ledger_sequence,
        "format": format,
        "output_path": path.display().to_string(),
        "file_name": exported.file_name,
        "test_name": exported.test_name,
        "expected_outcome": exported.expected_outcome,
        "ledger_entries": exported.ledger_entries,
        "auth_entries": exported.auth_entries,
        "bytes_written": exported.source.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "abababababababababababababababababababababababababababababababab";

    #[test]
    fn default_path_uses_full_hash() {
        assert_eq!(
            resolve_output_path(None, HASH),
            PathBuf::from(format!("grat_test_{HASH}.rs"))
        );
    }

    #[test]
    fn directory_output_gets_default_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let p = resolve_output_path(dir.path().to_str(), HASH);
        assert_eq!(p, dir.path().join(format!("grat_test_{HASH}.rs")));
    }

    #[test]
    fn explicit_file_path_is_kept() {
        assert_eq!(
            resolve_output_path(Some("tests/regression.rs"), HASH),
            PathBuf::from("tests/regression.rs")
        );
    }

    #[test]
    fn not_found_transactions_are_rejected() {
        assert!(expected_outcome(&TransactionStatus::NotFound, HASH, "testnet").is_err());
        assert_eq!(
            expected_outcome(&TransactionStatus::Failed, HASH, "testnet").unwrap(),
            ExpectedOutcome::Failure
        );
    }

    #[test]
    fn write_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/case.rs");
        write_test_file(&path, "// hi\n").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "// hi\n");
    }

    #[test]
    fn metadata_describes_the_written_file() {
        let exported = ExportedTestCase {
            tx_hash: HASH.into(),
            network: "testnet".into(),
            ledger_sequence: 42,
            expected_outcome: ExpectedOutcome::Failure,
            test_name: "reproduce_tx_abababababababab".into(),
            file_name: format!("grat_test_{HASH}.rs"),
            ledger_entries: 3,
            auth_entries: 1,
            source: "x".repeat(10),
        };
        let v = export_metadata(&exported, "test", Path::new("out/case.rs"));
        assert_eq!(v["status"], "exported");
        assert_eq!(v["output_path"], "out/case.rs");
        assert_eq!(v["expected_outcome"], "failure");
        assert_eq!(v["ledger_entries"], 3);
        assert_eq!(v["bytes_written"], 10);
    }
}
