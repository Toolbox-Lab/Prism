//! Soroban RPC client.
//!
//! Communicates with Soroban RPC endpoints: `getTransaction`, `simulateTransaction`,
//! `getLedgerEntries`, `getEvents`, `getLatestLedger`. Handles pagination, retries,
//! and rate-limit backoff via [`super::jsonrpc::JsonRpcTransport`].

use crate::network::jsonrpc::{
    EmptyParams, GetEventsParams, GetLedgerEntriesParams, GetTransactionParams,
    JsonRpcRequest, JsonRpcTransport, SimulateTransactionParams,
};
use crate::types::config::NetworkConfig;
use crate::types::error::PrismResult;

/// Soroban RPC client with retry and rate-limit handling.
pub struct RpcClient {
    /// HTTP client instance.
    client: reqwest::Client,
    /// Network configuration.
    config: NetworkConfig,
    /// Maximum number of retries for failed requests.
    max_retries: u32,
}

/// JSON-RPC request envelope.
#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: serde_json::Value,
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

impl RpcClient {
    /// Create a new RPC client for the given network.
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            transport: JsonRpcTransport::new(config.rpc_url, 3),
        }
    }

    /// Fetch a transaction by hash.
    pub async fn get_transaction(&self, tx_hash: &str) -> PrismResult<GetTransactionResponse> {
        let params = serde_json::json!([tx_hash]);
        self.call("getTransaction", params).await
    }

    /// Simulate a transaction (`simulateTransaction`).
    pub async fn simulate_transaction(&self, tx_xdr: &str) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "transaction": tx_xdr,
        });
        self.call::<serde_json::Value>("simulateTransaction", params).await
    }

    /// Get ledger entries by keys (`getLedgerEntries`).
    pub async fn get_ledger_entries(&self, keys: &[String]) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({
            "keys": keys,
        });
        self.call::<serde_json::Value>("getLedgerEntries", params).await
    }

    /// Get events matching a filter (`getEvents`).
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

    /// Get the latest ledger info (`getLatestLedger`).
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

        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                tokio::time::sleep(backoff).await;
                tracing::debug!("Retry attempt {attempt} for RPC method {method}");
            }

            let started_at = Instant::now();
            let request_body = serde_json::to_string(&request)
                .unwrap_or_else(|_| "<failed to serialize request>".to_string());
            tracing::debug!(
                method,
                endpoint = %self.config.rpc_url,
                attempt,
                "Sending RPC request"
            );
            tracing::trace!(
                method,
                endpoint = %self.config.rpc_url,
                attempt,
                request = %request_body,
                "RPC request payload"
            );

            match self
                .client
                .post(&self.config.rpc_url)
                .json(&request)
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    let response_body = response
                        .text()
                        .await
                        .map_err(|e| PrismError::RpcError(format!("Response read error: {e}")))?;
                    let elapsed_ms = started_at.elapsed().as_millis();

                    tracing::debug!(
                        method,
                        endpoint = %self.config.rpc_url,
                        attempt,
                        status = %status,
                        elapsed_ms,
                        "RPC response received"
                    );
                    tracing::trace!(
                        method,
                        endpoint = %self.config.rpc_url,
                        attempt,
                        elapsed_ms,
                        response = %response_body,
                        "RPC response payload"
                    );

                    if status == 429 {
                        tracing::warn!("Rate limited by RPC, backing off...");
                        last_error = Some(PrismError::RpcError("Rate limited".to_string()));
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

                    return rpc_response
                        .result
                        .ok_or_else(|| PrismError::RpcError("Empty response".to_string()));
                }
                Err(e) => {
                    tracing::debug!(
                        method,
                        endpoint = %self.config.rpc_url,
                        attempt,
                        elapsed_ms = started_at.elapsed().as_millis(),
                        error = %e,
                        "RPC request failed"
                    );
                    last_error = Some(PrismError::RpcError(format!("Request failed: {e}")));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| PrismError::RpcError("Unknown error".to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_transaction_deserialization() {
        let response_json = r#"{
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

        let rpc_response: JsonRpcResponse<GetTransactionResponse> =
            serde_json::from_str(response_json).unwrap();
        let result = rpc_response.result.unwrap();

        assert_eq!(result.status, TransactionStatus::Success);
        assert_eq!(result.latest_ledger, 123);
        assert_eq!(result.ledger, Some(120));
    }

    #[test]
    fn test_transaction_status_enum() {
        let status: TransactionStatus = serde_json::from_str("\"SUCCESS\"").unwrap();
        assert_eq!(status, TransactionStatus::Success);

        let status: TransactionStatus = serde_json::from_str("\"NOT_FOUND\"").unwrap();
        assert_eq!(status, TransactionStatus::NotFound);

        let status: TransactionStatus = serde_json::from_str("\"FAILED\"").unwrap();
        assert_eq!(status, TransactionStatus::Failed);
    }
}
