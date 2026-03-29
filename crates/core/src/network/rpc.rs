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

/// Transaction status in Soroban.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionStatus {
    Success,
    NotFound,
    Failed,
}

/// Response for the `getTransaction` RPC method.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionResponse {
    pub status: TransactionStatus,
    pub latest_ledger: u32,
    pub latest_ledger_close_time: Option<u64>,
    pub oldest_ledger: Option<u32>,
    pub oldest_ledger_close_time: Option<u64>,
    pub ledger: Option<u32>,
    pub created_at: Option<String>,
    pub application_order: Option<u32>,
    pub fee_bump: Option<String>,
    pub envelope_xdr: Option<String>,
    pub result_xdr: Option<String>,
    pub result_meta_xdr: Option<String>,
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

    /// Simulate a transaction given its XDR envelope.
    pub async fn simulate_transaction(&self, tx_xdr: &str) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "transaction": tx_xdr,
        });
        self.call("simulateTransaction", params).await
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
}
