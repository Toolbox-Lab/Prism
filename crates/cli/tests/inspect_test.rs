//! Integration tests for `grat inspect` (#518).
//!
//! Spins up a local mock Soroban JSON-RPC server returning a multi-operation
//! transaction so targeted `--op-index` filtering, full-transaction labeling,
//! JSON formatting, and `--save` persistence can be verified offline.

use assert_cmd::Command;
use grat_core::xdr::codec::XdrCodec;
use predicates::prelude::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use stellar_xdr::curr::{
    ExtensionPoint, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp,
    InvokeHostFunctionResult, LedgerEntryChanges, Memo, MuxedAccount, Operation, OperationBody,
    OperationMeta, OperationResult, OperationResultTr, Preconditions, ScAddress, ScSymbol, ScVal,
    SequenceNumber, SorobanTransactionMeta, SorobanTransactionMetaExt, SorobanTransactionMetaExtV1,
    Transaction, TransactionEnvelope, TransactionExt, TransactionMeta, TransactionMetaV3,
    TransactionResult, TransactionResultExt, TransactionResultResult, TransactionV1Envelope,
    Uint256, VecM,
};

struct MockRpcServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockRpcServer {
    fn start(response_body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock rpc listener");
        listener
            .set_nonblocking(true)
            .expect("set listener nonblocking");
        let addr = listener.local_addr().expect("listener local addr");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);

        let raw_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );

        let handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let mut buf = [0u8; 4096];
                        let _ = stream.read(&mut buf);
                        let _ = stream.write_all(raw_response.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            stop,
            handle: Some(handle),
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for MockRpcServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn invoke_op(contract_byte: u8, function_name: &str, arg: u32) -> Operation {
    Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: ScAddress::Contract(Hash([contract_byte; 32])),
                function_name: ScSymbol(function_name.try_into().unwrap()),
                args: vec![ScVal::U32(arg)].try_into().unwrap(),
            }),
            auth: VecM::default(),
        }),
    }
}

fn multi_op_rpc_response(tx_hash: &str) -> String {
    let tx = Transaction {
        source_account: MuxedAccount::Ed25519(Uint256([1u8; 32])),
        fee: 500,
        seq_num: SequenceNumber(42),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: vec![
            invoke_op(2u8, "first_op", 10),
            invoke_op(3u8, "second_op", 20),
        ]
        .try_into()
        .unwrap(),
        ext: TransactionExt::V0,
    };

    let envelope_xdr = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures: VecM::default(),
    })
    .to_xdr_base64()
    .unwrap();

    // Operation 0 succeeds; Operation 1 fails with Trapped.
    let result_xdr = TransactionResult {
        fee_charged: 350,
        result: TransactionResultResult::TxFailed(
            vec![
                OperationResult::OpInner(OperationResultTr::InvokeHostFunction(
                    InvokeHostFunctionResult::Success(Hash([0u8; 32])),
                )),
                OperationResult::OpInner(OperationResultTr::InvokeHostFunction(
                    InvokeHostFunctionResult::Trapped,
                )),
            ]
            .try_into()
            .unwrap(),
        ),
        ext: TransactionResultExt::V0,
    }
    .to_xdr_base64()
    .unwrap();

    let meta_xdr = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges(VecM::default()),
        operations: vec![
            OperationMeta {
                changes: LedgerEntryChanges(VecM::default()),
            },
            OperationMeta {
                changes: LedgerEntryChanges(VecM::default()),
            },
        ]
        .try_into()
        .unwrap(),
        tx_changes_after: LedgerEntryChanges(VecM::default()),
        soroban_meta: Some(SorobanTransactionMeta {
            ext: SorobanTransactionMetaExt::V1(SorobanTransactionMetaExtV1 {
                ext: ExtensionPoint::V0,
                total_non_refundable_resource_fee_charged: 100,
                total_refundable_resource_fee_charged: 200,
                rent_fee_charged: 50,
            }),
            events: VecM::default(),
            return_value: ScVal::Void,
            diagnostic_events: VecM::default(),
        }),
    })
    .to_xdr_base64()
    .unwrap();

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "status": "FAILED",
            "hash": tx_hash,
            "ledger": 12345,
            "envelopeXdr": envelope_xdr,
            "resultXdr": result_xdr,
            "resultMetaXdr": meta_xdr,
            "latestLedger": 12350,
            "latestLedgerCloseTime": 1711620000
        }
    })
    .to_string()
}

fn grat(home: &tempfile::TempDir, rpc_url: &str) -> Command {
    let mut cmd = Command::cargo_bin("grat").expect("grat binary should build");
    cmd.env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args([
            "--offline",
            "--no-color",
            "--no-cache",
            "--rpc-url",
            rpc_url,
        ]);
    cmd
}

#[test]
fn inspect_op_index_zero_targets_first_operation() {
    let tx_hash = "a".repeat(64);
    let server = MockRpcServer::start(multi_op_rpc_response(&tx_hash));
    let home = tempfile::tempdir().unwrap();

    let assert = grat(&home, &server.url())
        .args(["--output", "json", "inspect", &tx_hash, "--op-index", "0"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("single-op JSON output should be valid JSON");

    assert!(
        report.is_object(),
        "expected a single JSON report object when --op-index 0 is passed:\n{stdout}"
    );
    assert_eq!(report["error_name"], "Success");
    assert_eq!(report["operation_index"], 0);
    assert_eq!(report["operation_count"], 2);
    assert_eq!(report["transaction_context"]["function_name"], "first_op");
    assert!(
        !stdout.contains("second_op"),
        "output for --op-index 0 should not include second operation:\n{stdout}"
    );
}

#[test]
fn inspect_op_index_one_isolates_failing_second_operation() {
    let tx_hash = "b".repeat(64);
    let server = MockRpcServer::start(multi_op_rpc_response(&tx_hash));
    let home = tempfile::tempdir().unwrap();

    let assert = grat(&home, &server.url())
        .args(["--output", "json", "inspect", &tx_hash, "--op-index", "1"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("single-op JSON output should be valid JSON");

    assert_eq!(report["error_name"], "Trapped");
    assert_eq!(report["operation_index"], 1);
    assert_eq!(report["operation_count"], 2);
    assert_eq!(report["transaction_context"]["function_name"], "second_op");
    assert!(
        !stdout.contains("first_op"),
        "output for --op-index 1 should not include first operation:\n{stdout}"
    );
}

#[test]
fn inspect_without_op_index_labels_all_operations_in_human_output() {
    let tx_hash = "c".repeat(64);
    let server = MockRpcServer::start(multi_op_rpc_response(&tx_hash));
    let home = tempfile::tempdir().unwrap();

    grat(&home, &server.url())
        .args(["inspect", &tx_hash])
        .assert()
        .success()
        .stdout(predicate::str::contains("=== Operation 1 ==="))
        .stdout(predicate::str::contains("=== Operation 2 ==="))
        .stdout(predicate::str::contains("Success"))
        .stdout(predicate::str::contains("Trapped"));
}

#[test]
fn inspect_without_op_index_emits_valid_json_array() {
    let tx_hash = "d".repeat(64);
    let server = MockRpcServer::start(multi_op_rpc_response(&tx_hash));
    let home = tempfile::tempdir().unwrap();

    let assert = grat(&home, &server.url())
        .args(["--output", "json", "inspect", &tx_hash])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let reports: serde_json::Value =
        serde_json::from_str(&stdout).expect("multi-op JSON output should be a valid JSON array");
    let array = reports.as_array().expect("expected JSON array");
    assert_eq!(array.len(), 2);
    assert_eq!(array[0]["transaction_context"]["function_name"], "first_op");
    assert_eq!(array[0]["error_name"], "Success");
    assert_eq!(
        array[1]["transaction_context"]["function_name"],
        "second_op"
    );
    assert_eq!(array[1]["error_name"], "Trapped");
}

#[test]
fn inspect_save_persists_report_and_creates_parent_directories() {
    let tx_hash = "e".repeat(64);
    let server = MockRpcServer::start(multi_op_rpc_response(&tx_hash));
    let home = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let save_path = out.path().join("nested").join("inspect_op0.json");

    grat(&home, &server.url())
        .args(["inspect", &tx_hash, "--op-index", "0", "--save"])
        .arg(&save_path)
        .assert()
        .success()
        .stderr(predicate::str::contains("Saved report to"));

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&save_path).unwrap()).unwrap();
    let arr = saved.as_array().expect("saved report should be JSON array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["transaction_context"]["function_name"], "first_op");
}

#[test]
fn inspect_out_of_range_op_index_fails_with_clear_error() {
    let tx_hash = "f".repeat(64);
    let server = MockRpcServer::start(multi_op_rpc_response(&tx_hash));
    let home = tempfile::tempdir().unwrap();

    grat(&home, &server.url())
        .args(["inspect", &tx_hash, "--op-index", "5"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("out of range"));
}
