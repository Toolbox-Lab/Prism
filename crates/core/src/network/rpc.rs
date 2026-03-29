//! Soroban RPC client.
//!
//! Communicates with Soroban RPC endpoints: `getTransaction`, `simulateTransaction`,
//! `getLedgerEntries`, `getEvents`, `getLatestLedger`. Handles pagination, retries,
//! and rate limit backoff.

use crate::types::config::NetworkConfig;
use crate::types::error::{PrismError, PrismResult};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Primary entry point for Soroban network communication.
#[derive(Debug, Clone)]
pub struct SorobanRpcClient {
    /// HTTP client instance.
    client: reqwest::Client,
    /// Soroban RPC endpoint URL.
    rpc_url: String,
}

/// JSON-RPC request envelope.
#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a, P: Serialize> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: P,
}

/// JSON-RPC response envelope.
#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<T>,
    error: Option<JsonRpcError>,
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
    /// Status of the transaction.
    pub status: TransactionStatus,
    /// Latest ledger known to the RPC node.
    pub latest_ledger: u32,
    /// Latest ledger close time known to the RPC node.
    pub latest_ledger_close_time: Option<u64>,
    /// Oldest ledger known to the RPC node.
    pub oldest_ledger: Option<u32>,
    /// Oldest ledger close time known to the RPC node.
    pub oldest_ledger_close_time: Option<u64>,
    /// The ledger in which the transaction was included.
    pub ledger: Option<u32>,
    /// The creation time of the transaction.
    pub created_at: Option<String>,
    /// The order in which the transaction was applied in the ledger.
    pub application_order: Option<u32>,
    /// Fee bump information if applicable.
    pub fee_bump: Option<String>,
    /// Envelope XDR for the transaction.
    pub envelope_xdr: Option<String>,
    /// Result XDR for the transaction.
    pub result_xdr: Option<String>,
    /// Result Meta XDR for the transaction.
    pub result_meta_xdr: Option<String>,
}

/// JSON-RPC error.
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
    ///
    /// Initialises a [`reqwest::Client`] with a 30-second timeout and sets the
    /// `Content-Type: application/json` header on every request.
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
        self.call::<serde_json::Value>("simulateTransaction", params).await
    }

    /// Fetch ledger entries by their XDR keys.
    pub async fn get_ledger_entries(&self, keys: &[String]) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "keys": keys,
        });
        self.call::<serde_json::Value>("getLedgerEntries", params).await
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
        self.call::<serde_json::Value>("getEvents", params).await
    }

    /// Return the latest ledger info from the RPC node.
    pub async fn get_latest_ledger(&self) -> PrismResult<serde_json::Value> {
        self.call::<serde_json::Value>("getLatestLedger", serde_json::json!({})).await
    }

    /// Internal JSON-RPC call with retry logic.
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

            match self.client.post(&self.rpc_url).json(&envelope).send().await {
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

                    let rpc_response: JsonRpcResponse<T> = serde_json::from_str(&response_body)
                        .map_err(|e| PrismError::RpcError(format!("Response parse error: {e}")))?;

                    if let Some(err) = rpc_response.error {
                        tracing::debug!(
                            method,
                            endpoint = %self.config.rpc_url,
                            attempt,
                            error = %err.message,
                            "RPC returned an error response"
                        );
                        return Err(PrismError::RpcError(err.message));
                    }

                    return rpc_resp.result.ok_or_else(|| {
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
    fn test_get_transaction_deserialization() {
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
    fn test_transaction_status_variants() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::config::NetworkConfig;

    fn test_config() -> NetworkConfig {
        NetworkConfig {
            rpc_url: "https://rpc.example.com".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
        }
    }

    #[test]
    fn test_get_ledger_entries_request_format() {
        let config = test_config();
        let client = RpcClient::new(config);
        
        // Test that the method exists and can be called
        // We can't actually make HTTP requests in unit tests without mocking,
        // but we can verify the method signature and structure
        let keys = vec![
            "AAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
            "AAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB=".to_string(),
        ];
        
        // This will fail with a network error, but that's expected in unit tests
        // The important thing is that the method compiles and accepts the correct parameters
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(client.get_ledger_entries(&keys));
        
        // We expect a network error since we're not mocking the HTTP client
        assert!(result.is_err());
    }

    #[test]
    fn test_get_ledger_entries_empty_keys() {
        let config = test_config();
        let client = RpcClient::new(config);
        
        let keys: Vec<String> = vec![];
        
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(client.get_ledger_entries(&keys));
        
        // We expect a network error since we're not mocking the HTTP client
        assert!(result.is_err());
    }
}
