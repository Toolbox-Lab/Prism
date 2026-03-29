//! Soroban RPC client.
//!
//! Communicates with Soroban RPC endpoints: `getTransaction`, `simulateTransaction`,
//! `getLedgerEntries`, `getEvents`, `getLatestLedger`. Handles retries and
//! basic rate-limit backoff.

use crate::types::config::NetworkConfig;
use crate::types::error::{PrismError, PrismResult};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

// ── simulateTransaction response types ──────────────────────────────────────

/// Ledger footprint returned by `simulateTransaction`.
///
/// Contains the read-only and read-write ledger keys the transaction will
/// access, expressed as base64-encoded XDR `LedgerKey` values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateFootprint {
    /// Keys the transaction reads but does not modify.
    #[serde(rename = "readOnly", default)]
    pub read_only: Vec<String>,
    /// Keys the transaction reads and may modify.
    #[serde(rename = "readWrite", default)]
    pub read_write: Vec<String>,
}

/// Per-invocation authorization entry returned by `simulateTransaction`.
///
/// Each entry is a base64-encoded XDR `SorobanAuthorizationEntry` that the
/// caller must sign (or leave unsigned for `invoker_contract_auth`) before
/// submitting the transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateAuthEntry {
    /// Base64-encoded XDR `SorobanAuthorizationEntry`.
    pub xdr: String,
}

/// Resource cost estimates returned by `simulateTransaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateCost {
    /// CPU instruction count consumed.
    #[serde(rename = "cpuInsns", default)]
    pub cpu_insns: String,
    /// Memory bytes consumed.
    #[serde(rename = "memBytes", default)]
    pub mem_bytes: String,
}

/// Soroban resource limits and fees returned by `simulateTransaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateSorobanData {
    /// Base64-encoded XDR `SorobanTransactionData` (footprint + resource limits).
    pub data: String,
    /// Minimum resource fee in stroops.
    #[serde(rename = "minResourceFee")]
    pub min_resource_fee: String,
}

/// Typed response from the `simulateTransaction` RPC method.
///
/// Callers use `soroban_data` to stamp the transaction's `SorobanTransactionData`
/// extension and `auth` to populate the authorization entries before submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateTransactionResponse {
    /// Latest ledger sequence number at the time of simulation.
    #[serde(rename = "latestLedger")]
    pub latest_ledger: u32,
    /// Soroban resource data (footprint + fees) to attach to the transaction.
    #[serde(rename = "transactionData", default)]
    pub soroban_data: Option<String>,
    /// Minimum resource fee in stroops required for submission.
    #[serde(rename = "minResourceFee", default)]
    pub min_resource_fee: Option<String>,
    /// Authorization entries that must be signed before submission.
    #[serde(default)]
    pub auth: Vec<String>,
    /// Return value of the simulated invocation (base64 XDR `ScVal`), if any.
    #[serde(default)]
    pub results: Vec<SimulateResult>,
    /// Error message if the simulation failed.
    #[serde(default)]
    pub error: Option<String>,
    /// Diagnostic events emitted during simulation.
    #[serde(default)]
    pub events: Vec<String>,
    /// Cost estimates for the simulation.
    #[serde(default)]
    pub cost: Option<SimulateCost>,
}

/// A single invocation result within a `simulateTransaction` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateResult {
    /// Base64-encoded XDR `ScVal` return value.
    #[serde(default)]
    pub xdr: String,
    /// Authorization entries required for this invocation.
    #[serde(default)]
    pub auth: Vec<String>,
}

impl SimulateTransactionResponse {
    /// Returns `true` if the simulation completed without an error.
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Convenience accessor for the first return value XDR, if present.
    pub fn return_value_xdr(&self) -> Option<&str> {
        self.results.first().map(|r| r.xdr.as_str())
    }
}

/// Primary entry point for Soroban network communication.
#[derive(Debug, Clone)]
pub struct SorobanRpcClient {
    client: reqwest::Client,
    rpc_url: String,
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a, P: Serialize> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: P,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

impl SorobanRpcClient {
    /// Create a new `SorobanRpcClient` from a [`NetworkConfig`].
    pub fn new(config: &NetworkConfig) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .default_headers(headers)
            .build()
            .expect("Failed to build reqwest client");

        Self {
            client,
            rpc_url: config.rpc_url.clone(),
        }
    }

    /// Fetch a transaction by hash.
    pub async fn get_transaction(&self, tx_hash: &str) -> PrismResult<GetTransactionResponse> {
        let params = serde_json::json!([tx_hash]);
        self.call("getTransaction", params).await
    }

    /// Simulate a transaction against the current ledger state.
    ///
    /// Fires the `simulateTransaction` JSON-RPC method and returns a typed
    /// [`SimulateTransactionResponse`] containing:
    /// - `soroban_data` — the `SorobanTransactionData` XDR to stamp onto the
    ///   transaction before submission (footprint + resource limits).
    /// - `min_resource_fee` — the minimum fee in stroops required.
    /// - `auth` — authorization entries that must be signed by the relevant
    ///   parties before the transaction is submitted.
    /// - `results` — per-invocation return values.
    ///
    /// If the node returns an `error` field the method returns
    /// [`PrismError::RpcError`] so callers can surface the simulation failure
    /// without having to inspect the raw JSON.
    ///
    /// # Arguments
    /// * `tx_xdr` — base64-encoded XDR of the unsigned `TransactionEnvelope`.
    pub async fn simulate_transaction(
        &self,
        tx_xdr: &str,
    ) -> PrismResult<SimulateTransactionResponse> {
        let params = serde_json::json!({ "transaction": tx_xdr });
        let response: SimulateTransactionResponse =
            self.call("simulateTransaction", params).await?;

        // Surface simulation-level errors as a proper Rust error so callers
        // don't need to inspect the struct themselves.
        if let Some(ref err) = response.error {
            return Err(PrismError::RpcError(format!(
                "simulateTransaction failed: {err}"
            )));
        }

        Ok(response)
    }

    /// Fetch ledger entries by their XDR keys.
    pub async fn get_ledger_entries(&self, keys: &[String]) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "keys": keys,
        });
        self.call("getLedgerEntries", params).await
    }

    /// Query events starting from `start_ledger` with the given filters.
    pub async fn get_events(
        &self,
        start_ledger: u32,
        filters: serde_json::Value,
    ) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "startLedger": start_ledger,
            "filters": filters,
        });
        self.call("getEvents", params).await
    }

    /// Return the latest ledger info from the RPC node.
    pub async fn get_latest_ledger(&self) -> PrismResult<serde_json::Value> {
        self.call("getLatestLedger", serde_json::json!({})).await
    }

    /// Internal JSON-RPC call with retry and rate-limit backoff.
    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> PrismResult<T> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method,
            params,
        };

        const MAX_RETRIES: u32 = 3;
        let mut last_error: Option<PrismError> = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_millis(100 * 2u64.pow(attempt));
                tokio::time::sleep(backoff).await;
                tracing::debug!(attempt, method, "Retrying RPC request");
            }

            let started = Instant::now();
            tracing::debug!(method, endpoint = %self.rpc_url, attempt, "Sending RPC request");

            match self.client.post(&self.rpc_url).json(&request).send().await {
                Ok(response) => {
                    let status = response.status();
                    let elapsed_ms = started.elapsed().as_millis();
                    let body = response.text().await.map_err(|e| {
                        PrismError::RpcError(format!("Failed to read response body: {e}"))
                    })?;

                    tracing::debug!(
                        method,
                        endpoint = %self.rpc_url,
                        attempt,
                        %status,
                        elapsed_ms,
                        "RPC response received"
                    );

                    if status == 429 {
                        tracing::warn!(method, "Rate limited by RPC node, backing off");
                        last_error =
                            Some(PrismError::RpcError("Rate limited (HTTP 429)".to_string()));
                        continue;
                    }

                    if !status.is_success() {
                        return Err(PrismError::RpcError(format!(
                            "RPC request failed with HTTP {}: {}",
                            status, body
                        )));
                    }

                    let rpc_response: JsonRpcResponse<T> = serde_json::from_str(&body)
                        .map_err(|e| PrismError::RpcError(format!("Response parse error: {e}")))?;

                    if let Some(err) = rpc_response.error {
                        tracing::debug!(
                            method,
                            endpoint = %self.rpc_url,
                            attempt,
                            error = %err.message,
                            "RPC returned an error response"
                        );
                        return Err(PrismError::RpcError(err.message));
                    }

                    return rpc_response.result.ok_or_else(|| {
                        PrismError::RpcError("Empty result in RPC response".into())
                    });
                }
                Err(e) => {
                    tracing::debug!(
                        method,
                        endpoint = %self.rpc_url,
                        attempt,
                        elapsed_ms = started.elapsed().as_millis(),
                        error = %e,
                        "RPC request failed"
                    );
                    last_error = Some(PrismError::RpcError(format!("HTTP request failed: {e}")));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| PrismError::RpcError("Unknown RPC error".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_transaction_response_deserializes() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "status": "SUCCESS",
                "latestLedger": 123,
                "latestLedgerCloseTime": 1711620000,
                "ledger": 120,
                "createdAt": "2024-03-28T10:00:00Z",
                "applicationOrder": 1,
                "envelopeXdr": "AAAAAg...",
                "resultXdr": "AAAAAw...",
                "resultMetaXdr": "AAAABA..."
            }
        }"#;

        let resp: JsonRpcResponse<GetTransactionResponse> = serde_json::from_str(json).unwrap();
        let result = resp.result.unwrap();

        assert_eq!(result.status, TransactionStatus::Success);
        assert_eq!(result.latest_ledger, 123);
        assert_eq!(result.ledger, Some(120));
    }

    #[test]
    fn transaction_status_variants_deserialize() {
        let cases = [
            ("\"SUCCESS\"", TransactionStatus::Success),
            ("\"NOT_FOUND\"", TransactionStatus::NotFound),
            ("\"FAILED\"", TransactionStatus::Failed),
        ];

        for (raw, expected) in cases {
            let got: TransactionStatus = serde_json::from_str(raw).unwrap();
            assert_eq!(got, expected);
        }
    }
    #[test]
    fn test_simulate_response_is_success() {
        let ok = SimulateTransactionResponse {
            latest_ledger: 100,
            soroban_data: Some("AAAA".to_string()),
            min_resource_fee: Some("1000".to_string()),
            auth: vec![],
            results: vec![],
            error: None,
            events: vec![],
            cost: None,
        };
        assert!(ok.is_success());

        let err = SimulateTransactionResponse {
            error: Some("contract trap".to_string()),
            ..ok
        };
        assert!(!err.is_success());
    }

    #[test]
    fn test_simulate_response_deserialization() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "latestLedger": 200,
                "transactionData": "AAAAXDR=",
                "minResourceFee": "5000",
                "auth": ["AUTHXDR="],
                "results": [{"xdr": "RETVAL=", "auth": []}],
                "events": []
            }
        }"#;

        let resp: JsonRpcResponse<SimulateTransactionResponse> =
            serde_json::from_str(json).unwrap();
        let result = resp.result.unwrap();

        assert_eq!(result.latest_ledger, 200);
        assert_eq!(result.soroban_data.as_deref(), Some("AAAAXDR="));
        assert_eq!(result.min_resource_fee.as_deref(), Some("5000"));
        assert_eq!(result.auth, vec!["AUTHXDR="]);
        assert_eq!(result.return_value_xdr(), Some("RETVAL="));
        assert!(result.is_success());
    }
}
