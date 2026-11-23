//! End-to-end tests for `grat decode --raw` (#517).
//!
//! Every test points `--rpc-url` at a closed local port, so a decode that
//! tried to reach the network would fail. Passing tests prove the raw path
//! works offline.

use assert_cmd::Command;
use grat_core::xdr::codec::XdrCodec;
use predicates::prelude::*;
use stellar_xdr::curr::{
    Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, InvokeHostFunctionResult, Memo,
    MuxedAccount, Operation, OperationBody, OperationResult, OperationResultTr, Preconditions,
    ScAddress, ScSymbol, ScVal, SequenceNumber, Transaction, TransactionEnvelope, TransactionExt,
    TransactionResult, TransactionResultExt, TransactionResultResult, TransactionV1Envelope,
    Uint256,
};

/// Nothing listens here, so any network call made by the command fails.
const CLOSED_RPC_URL: &str = "http://127.0.0.1:9";

fn trapped_result_xdr() -> String {
    TransactionResult {
        fee_charged: 450,
        result: TransactionResultResult::TxFailed(
            vec![OperationResult::OpInner(
                OperationResultTr::InvokeHostFunction(InvokeHostFunctionResult::Trapped),
            )]
            .try_into()
            .unwrap(),
        ),
        ext: TransactionResultExt::V0,
    }
    .to_xdr_base64()
    .unwrap()
}

fn invoke_envelope_xdr() -> String {
    let operation = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: ScAddress::Contract(Hash([2; 32])),
                function_name: ScSymbol("transfer".try_into().unwrap()),
                args: vec![ScVal::U32(7)].try_into().unwrap(),
            }),
            auth: vec![].try_into().unwrap(),
        }),
    };
    TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([1; 32])),
            fee: 500,
            seq_num: SequenceNumber(42),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![operation].try_into().unwrap(),
            ext: TransactionExt::V0,
        },
        signatures: vec![].try_into().unwrap(),
    })
    .to_xdr_base64()
    .unwrap()
}

/// A `grat` command isolated from the user's home folder and the network.
fn grat(home: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("grat").expect("the grat binary should build");
    cmd.env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(["--offline", "--no-color", "--rpc-url", CLOSED_RPC_URL]);
    cmd
}

#[test]
fn raw_result_decodes_offline_to_structured_json() {
    let home = tempfile::tempdir().unwrap();

    let assert = grat(&home)
        .args(["--output", "json", "decode", "--raw", &trapped_result_xdr()])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(report["error_name"], "Trapped");
    assert_eq!(report["error_category"], "Soroban");
    assert_eq!(report["severity"], "error");
    assert!(!report["root_causes"].as_array().unwrap().is_empty());
    assert!(!report["suggested_fixes"].as_array().unwrap().is_empty());
    assert_eq!(
        report["transaction_context"]["fee"]["total_charged_fee"],
        450
    );
}

#[test]
fn raw_result_renders_causes_fixes_and_fees_for_humans() {
    let home = tempfile::tempdir().unwrap();

    grat(&home)
        .args(["decode", "--raw", &trapped_result_xdr()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Trapped"))
        .stdout(predicate::str::contains("COMMON CAUSES"))
        .stdout(predicate::str::contains("SUGGESTED FIXES"))
        .stdout(predicate::str::contains("FEE BREAKDOWN"))
        .stdout(predicate::str::contains(
            "No resource usage data is available for this transaction.",
        ));
}

#[test]
fn short_flag_prints_one_line() {
    let home = tempfile::tempdir().unwrap();

    let assert = grat(&home)
        .args(["decode", "--raw", "--short", &trapped_result_xdr()])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert_eq!(stdout.trim().lines().count(), 1, "{stdout}");
    assert!(stdout.starts_with("[Soroban] Trapped:"), "{stdout}");
}

#[test]
fn envelope_and_result_name_the_failing_contract_function() {
    let home = tempfile::tempdir().unwrap();
    let input = format!("{},{}", invoke_envelope_xdr(), trapped_result_xdr());

    let assert = grat(&home)
        .args(["--output", "json", "decode", "--raw", &input])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(report["error_name"], "Trapped");
    assert_eq!(report["transaction_context"]["function_name"], "transfer");
    assert_eq!(report["transaction_context"]["fee"]["bid_fee"], 500);
}

#[test]
fn save_writes_the_json_report_and_creates_folders() {
    let home = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("reports").join("decode.json");

    grat(&home)
        .args(["decode", "--raw", &trapped_result_xdr(), "--save"])
        .arg(&path)
        .assert()
        .success()
        .stderr(predicate::str::contains("Saved report to"));

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved[0]["error_name"], "Trapped");
}

#[test]
fn raw_decode_does_not_touch_command_history() {
    let home = tempfile::tempdir().unwrap();

    grat(&home)
        .args(["decode", "--raw", &trapped_result_xdr()])
        .assert()
        .success();

    assert!(!home.path().join(".grat").join("history.json").exists());
}

#[test]
fn invalid_raw_input_fails_with_a_clear_message() {
    let home = tempfile::tempdir().unwrap();

    grat(&home)
        .args(["decode", "--raw", "AAAA"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not decode raw XDR"))
        .stderr(predicate::str::contains("not a TransactionEnvelope"));
}
