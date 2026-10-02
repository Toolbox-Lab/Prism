use assert_cmd::Command;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use stellar_xdr::curr::{
    AccountId, ExtensionPoint, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp,
    LedgerEntryChanges, Memo, MuxedAccount, Operation, OperationBody, OperationMeta,
    OperationResult, OperationResultTr, Preconditions, PublicKey, ScAddress, ScSymbol, ScVal,
    SequenceNumber, SorobanTransactionMeta, SorobanTransactionMetaExt, SorobanTransactionMetaExtV1,
    Transaction, TransactionEnvelope, TransactionExt, TransactionMeta, TransactionMetaV3,
    TransactionResult, TransactionResultResult, TransactionResultExt, TransactionV1Envelope,
    Uint256, VecM,
};

async fn spawn_mock_server(responses: Vec<String>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let responses = Arc::new(responses);
    let counter = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let responses = Arc::clone(&responses);
            let counter = Arc::clone(&counter);
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let idx = counter.fetch_add(1, Ordering::SeqCst);
                let raw = responses
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| responses.last().cloned().unwrap_or_default());
                let _ = stream.write_all(raw.as_bytes()).await;
            });
        }
    });

    addr
}

fn http_response(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn build_envelope_xdr() -> String {
    let tx = Transaction {
        source_account: MuxedAccount::Ed25519(Uint256([1u8; 32])),
        fee: 200,
        seq_num: SequenceNumber(42),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: vec![Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(Hash([2u8; 32])),
                    function_name: ScSymbol("test_fn".try_into().unwrap()),
                    args: vec![ScVal::U32(42)].try_into().unwrap(),
                }),
                auth: VecM::default(),
            }),
        }]
        .try_into()
        .unwrap(),
        ext: TransactionExt::V0,
    };

    let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures: VecM::default(),
    });

    envelope.to_xdr_base64().unwrap()
}

fn build_result_xdr(success: bool) -> String {
    let result = if success {
        TransactionResult {
            fee_charged: 150,
            result: TransactionResultResult::TxSuccess(
                vec![OperationResult::OpInner(OperationResultTr::InvokeHostFunction(
                    InvokeHostFunctionResult::Success(Hash([0u8; 32])),
                ))]
                .try_into()
                .unwrap(),
            ),
            ext: TransactionResultExt::V0,
        }
    } else {
        TransactionResult {
            fee_charged: 150,
            result: TransactionResultResult::TxFailed(
                vec![OperationResult::OpInner(OperationResultTr::InvokeHostFunction(
                    InvokeHostFunctionResult::Trapped,
                ))]
                .try_into()
                .unwrap(),
            ),
            ext: TransactionResultExt::V0,
        }
    };

    result.to_xdr_base64().unwrap()
}

fn build_meta_xdr() -> String {
    let meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges(VecM::default()),
        operations: vec![OperationMeta {
            changes: LedgerEntryChanges(VecM::default()),
        }]
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
    });

    meta.to_xdr_base64().unwrap()
}

fn get_transaction_response(tx_hash: &str, success: bool) -> String {
    let envelope_xdr = build_envelope_xdr();
    let result_xdr = build_result_xdr(success);
    let meta_xdr = build_meta_xdr();

    let status = if success { "SUCCESS" } else { "FAILED" };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "status": status,
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

#[tokio::test]
async fn test_decode_command_loop_success() {
    let tx_hash = "testhash-success-loop";
    let body = get_transaction_response(tx_hash, true);
    let response = http_response(200, "OK", &body);
    let addr = spawn_mock_server(vec![response]).await;

    for i in 0..5 {
        let mut cmd = Command::cargo_bin("grat").expect("Failed to find or build 'grat' binary. Ensure the project compiles on this platform.");
        cmd.args([
            "decode",
            tx_hash,
            "--offline",
            "--output",
            "json",
            "--rpc-url",
            &format!("http://{}", addr),
        ]);

        let assert = cmd.assert().success();
        let output = String::from_utf8_lossy(&assert.get_output().stdout);

        assert!(
            output.contains("error_category"),
            "Loop {}: output missing error_category:\n{}",
            i,
            output
        );
        assert!(
            output.contains("error_name"),
            "Loop {}: output missing error_name:\n{}",
            i,
            output
        );
        assert!(
            output.contains("summary"),
            "Loop {}: output missing summary:\n{}",
            i,
            output
        );
        assert!(
            output.contains("severity"),
            "Loop {}: output missing severity:\n{}",
            i,
            output
        );
        assert!(
            output.contains("transaction_context"),
            "Loop {}: output missing transaction_context:\n{}",
            i,
            output
        );
    }
}

#[tokio::test]
async fn test_decode_command_loop_failure() {
    let tx_hash = "testhash-failure-loop";
    let body = get_transaction_response(tx_hash, false);
    let response = http_response(200, "OK", &body);
    let addr = spawn_mock_server(vec![response]).await;

    for i in 0..5 {
        let mut cmd = Command::cargo_bin("grat").expect("Failed to find or build 'grat' binary. Ensure the project compiles on this platform.");
        cmd.args([
            "decode",
            tx_hash,
            "--offline",
            "--output",
            "json",
            "--rpc-url",
            &format!("http://{}", addr),
        ]);

        let assert = cmd.assert().success();
        let output = String::from_utf8_lossy(&assert.get_output().stdout);

        assert!(
            output.contains("error_category"),
            "Loop {}: output missing error_category:\n{}",
            i,
            output
        );
        assert!(
            output.contains("severity"),
            "Loop {}: output missing severity:\n{}",
            i,
            output
        );
        assert!(
            output.contains("\"Error\"") || output.contains("\"Warning\""),
            "Loop {}: expected error or warning severity for failed tx:\n{}",
            i,
            output
        );
    }
}

#[tokio::test]
async fn test_decode_command_prints_reports_to_stdout() {
    let tx_hash = "testhash-print-check";
    let body = get_transaction_response(tx_hash, true);
    let response = http_response(200, "OK", &body);
    let addr = spawn_mock_server(vec![response]).await;

    let mut cmd = Command::cargo_bin("grat").expect("Failed to find or build 'grat' binary. Ensure the project compiles on this platform.");
    cmd.args([
        "decode",
        tx_hash,
        "--offline",
        "--output",
        "json",
        "--rpc-url",
        &format!("http://{}", addr),
    ]);

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    assert!(
        stdout.contains("error_category"),
        "stdout missing error_category:\n{}",
        stdout
    );
    assert!(
        stdout.contains("error_name"),
        "stdout missing error_name:\n{}",
        stdout
    );
    assert!(
        stdout.contains("summary"),
        "stdout missing summary:\n{}",
        stdout
    );
    assert!(
        stdout.contains("transaction_context"),
        "stdout missing transaction_context:\n{}",
        stdout
    );
}
