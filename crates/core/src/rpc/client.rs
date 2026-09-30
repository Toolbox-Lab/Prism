use crate::error::{PrismError, PrismResult};
use crate::network::NetworkConfig;
use crate::rpc::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const BASE_DELAY_MS: u64 = 100;

const MAX_DELAY_MS: u64 = 10_000;

fn backoff_duration(attempt: u32) -> Duration {
    let ms = BASE_DELAY_MS.saturating_mul(2u64.saturating_pow(attempt));
    Duration::from_millis(ms.min(MAX_DELAY_MS))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateFootprint {
    #[serde(rename = "readOnly", default)]
    pub read_only: Vec<String>,
    #[serde(rename = "readWrite", default)]
    pub read_write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateAuthEntry {
    pub xdr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateCost {
    #[serde(rename = "cpuInsns", default)]
    pub cpu_insns: String,
    #[serde(rename = "memBytes", default)]
    pub mem_bytes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateSorobanData {
    pub data: String,
    #[serde(rename = "minResourceFee")]
    pub min_resource_fee: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateTransactionResponse {
    #[serde(rename = "latestLedger")]
    pub latest_ledger: u32,
    #[serde(rename = "transactionData", default)]
    pub soroban_data: Option<String>,
    #[serde(rename = "minResourceFee", default)]
    pub min_resource_fee: Option<String>,
    #[serde(default)]
    pub auth: Vec<String>,
    #[serde(default)]
    pub results: Vec<SimulateResult>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub cost: Option<SimulateCost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateResult {
    #[serde(default)]
    pub xdr: String,
    #[serde(default)]
    pub auth: Vec<String>,
}

impl SimulateTransactionResponse {
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    pub fn return_value_xdr(&self) -> Option<&str> {
        self.results.first().map(|r| r.xdr.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct SorobanRpcClient {
    client: reqwest::Client,

    rpc_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionStatus {
    Success,
    NotFound,
    Failed,
}

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
    pub fn new(config: &NetworkConfig) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .user_agent(concat!("prism-cli/", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .build()
            .expect("Failed to build reqwest client");

        Self {
            client,
            rpc_url: config.rpc_url.clone(),
        }
    }

    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        self.client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent(concat!("prism-cli/", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .build()
            .expect("Failed to build reqwest client");
        self
    }

    pub async fn get_transaction(&self, tx_hash: &str) -> PrismResult<GetTransactionResponse> {
        let params = serde_json::json!([tx_hash]);
        self.call("getTransaction", params).await
    }

    pub async fn simulate_transaction(
        &self,
        tx_xdr: &str,
    ) -> PrismResult<SimulateTransactionResponse> {
        let params = serde_json::json!({ "transaction": tx_xdr });
        let raw = self
            .call::<serde_json::Value>("simulateTransaction", params)
            .await?;

        let response: SimulateTransactionResponse = serde_json::from_value(raw).map_err(|e| {
            PrismError::RpcError(format!("Failed to parse simulateTransaction response: {e}"))
        })?;

        if let Some(ref err) = response.error {
            return Err(PrismError::RpcError(format!(
                "simulateTransaction failed: {err}"
            )));
        }

        Ok(response)
    }

    pub async fn get_ledger_entries(&self, keys: &[String]) -> PrismResult<serde_json::Value> {
        let params = serde_json::json!({ "keys": keys });
        self.call::<serde_json::Value>("getLedgerEntries", params)
            .await
    }

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

    pub async fn get_latest_ledger(&self) -> PrismResult<serde_json::Value> {
        self.call("getLatestLedger", serde_json::json!({})).await
    }

    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &'static str,
        params: serde_json::Value,
    ) -> PrismResult<T> {
        let request = JsonRpcRequest::new(1, method, params);

        const MAX_RETRIES: u32 = 3;
        let mut last_error: Option<PrismError> = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = backoff_duration(attempt);
                tracing::debug!(
                    method,
                    attempt,
                    delay_ms = delay.as_millis(),
                    "Backing off before retry"
                );
                tokio::time::sleep(delay).await;
                tracing::debug!(attempt, method, "Retrying RPC request");
            }

            let started = Instant::now();
            tracing::debug!(method, endpoint = %self.rpc_url, attempt, "Sending RPC request");

            match self.client.post(&self.rpc_url).json(&request).send().await {
                Ok(response) => {
                    let status = response.status();
                    let elapsed_ms = started.elapsed().as_millis();
                    let duration_secs = started.elapsed().as_secs_f64();
                    tracing::info!(
                        method,
                        endpoint = %self.rpc_url,
                        attempt,
                        elapsed_ms,
                        "RPC request latency"
                    );

                    // Retry on 429 Too Many Requests.
                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        crate::rpc::record_rpc_duration(method, duration_secs, false);
                        tracing::warn!(
                            method,
                            attempt,
                            "Rate limited by RPC node (429), will retry"
                        );
                        last_error = Some(PrismError::RpcError(format!(
                            "Rate limited (attempt {attempt})"
                        )));
                        continue;
                    }

                    // Retry on any 5xx Server Error — these are transient node failures.
                    if status.is_server_error() {
                        crate::rpc::record_rpc_duration(method, duration_secs, false);
                        tracing::warn!(
                            method,
                            attempt,
                            status = %status,
                            elapsed_ms,
                            "RPC node returned a server error (5xx), will retry"
                        );
                        last_error = Some(PrismError::RpcError(format!(
                            "Server error {status} on attempt {attempt}"
                        )));
                        continue;
                    }

                    let body = response.text().await.map_err(|e| {
                        crate::rpc::record_rpc_duration(method, duration_secs, false);
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

                    if !status.is_success() {
                        crate::rpc::record_rpc_duration(method, duration_secs, false);
                        return Err(PrismError::RpcError(format!(
                            "RPC request failed with HTTP {status}: {body}"
                        )));
                    }

                    let rpc_response: JsonRpcResponse<T> =
                        serde_json::from_str(&body).map_err(|e| {
                            crate::rpc::record_rpc_duration(method, duration_secs, false);
                            PrismError::RpcError(format!("Response parse error: {e}"))
                        })?;

                    if let Some(err) = rpc_response.error {
                        crate::rpc::record_rpc_duration(method, duration_secs, false);
                        tracing::debug!(
                            method,
                            endpoint = %self.rpc_url,
                            attempt,
                            error = %err.message,
                            code = err.code,
                            "RPC returned an error response"
                        );
                        return Err(PrismError::JsonRpc(err));
                    }

                    crate::rpc::record_rpc_duration(method, duration_secs, true);
                    return rpc_response.result.ok_or_else(|| {
                        PrismError::RpcError("Empty result in RPC response".into())
                    });
                }
                Err(e) => {
                    let elapsed_ms = started.elapsed().as_millis();
                    let duration_secs = started.elapsed().as_secs_f64();
                    crate::rpc::record_rpc_duration(method, duration_secs, false);
                    tracing::info!(
                        method,
                        endpoint = %self.rpc_url,
                        attempt,
                        elapsed_ms,
                        error = %e,
                        "RPC request latency"
                    );
                    tracing::debug!(
                        method,
                        endpoint = %self.rpc_url,
                        attempt,
                        elapsed_ms,
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
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    // -------------------------------------------------------------------------
    // Test helpers
    // -------------------------------------------------------------------------

    /// Spawn an in-process HTTP/1.1 mock server that replies to each successive
    /// connection with the next entry from `responses`.  If there are more
    /// connections than responses the last entry is repeated.
    /// Returns the bound local socket address.
    async fn spawn_mock_server(responses: Vec<String>) -> std::net::SocketAddr {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use tokio::io::AsyncReadExt;

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

    /// Build a raw HTTP/1.1 response string.
    fn http_response(status: u16, reason: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// Minimal valid JSON-RPC 2.0 success body (getLatestLedger shape).
    fn ok_body() -> &'static str {
        r#"{"jsonrpc":"2.0","id":1,"result":{"id":"test","protocolVersion":"21","sequence":100}}"#
    }

    fn make_client(addr: std::net::SocketAddr) -> SorobanRpcClient {
        let config = NetworkConfig {
            network: crate::network::Network::Testnet,
            rpc_url: format!("http://{addr}"),
            network_passphrase: "test".to_string(),
            archive_urls: vec![],
            api_key: None,
            request_timeout_secs: 5,
        };
        SorobanRpcClient::new(&config)
    }

    #[test]
    fn backoff_increases_exponentially() {
        assert_eq!(backoff_duration(1), Duration::from_millis(200));
        assert_eq!(backoff_duration(2), Duration::from_millis(400));
        assert_eq!(backoff_duration(3), Duration::from_millis(800));
        assert_eq!(backoff_duration(4), Duration::from_millis(1_600));
        assert_eq!(backoff_duration(5), Duration::from_millis(3_200));
        assert_eq!(backoff_duration(6), Duration::from_millis(6_400));
    }

    #[test]
    fn backoff_is_capped_at_max_delay() {
        assert_eq!(backoff_duration(7), Duration::from_millis(MAX_DELAY_MS));

        assert_eq!(backoff_duration(63), Duration::from_millis(MAX_DELAY_MS));
    }

    #[test]
    fn backoff_attempt_zero_returns_base_delay() {
        assert_eq!(backoff_duration(0), Duration::from_millis(BASE_DELAY_MS));
    }

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

    #[test]
    fn test_get_transaction_success_status() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "status": "SUCCESS",
                "latestLedger": 500,
                "latestLedgerCloseTime": 1711620000,
                "oldestLedger": 100,
                "oldestLedgerCloseTime": 1711610000,
                "ledger": 450,
                "createdAt": "2024-03-28T10:00:00Z",
                "applicationOrder": 2,
                "envelopeXdr": "AAAAAgAAAABqYWNrQGV4YW1wbGUuY29tAAABkA==",
                "resultXdr": "AAAAAAAAAGQAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAA=",
                "resultMetaXdr": "AAAAAwAAAAAAAAACAAAAAwAAAcQAAAAAAAAAAA=="
            }
        }"#;

        let resp: JsonRpcResponse<GetTransactionResponse> = serde_json::from_str(json).unwrap();
        let result = resp.result.unwrap();

        assert_eq!(result.status, TransactionStatus::Success);
        assert_eq!(result.latest_ledger, 500);
        assert_eq!(result.latest_ledger_close_time, Some(1711620000));
        assert_eq!(result.oldest_ledger, Some(100));
        assert_eq!(result.oldest_ledger_close_time, Some(1711610000));
        assert_eq!(result.ledger, Some(450));
        assert_eq!(result.created_at, Some("2024-03-28T10:00:00Z".to_string()));
        assert_eq!(result.application_order, Some(2));
        assert_eq!(
            result.envelope_xdr,
            Some("AAAAAgAAAABqYWNrQGV4YW1wbGUuY29tAAABkA==".to_string())
        );
    }

    #[test]
    fn test_get_transaction_not_found_status() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "status": "NOT_FOUND",
                "latestLedger": 600,
                "latestLedgerCloseTime": 1711625000,
                "oldestLedger": 200,
                "oldestLedgerCloseTime": 1711615000
            }
        }"#;

        let resp: JsonRpcResponse<GetTransactionResponse> = serde_json::from_str(json).unwrap();
        let result = resp.result.unwrap();

        assert_eq!(result.status, TransactionStatus::NotFound);
        assert_eq!(result.latest_ledger, 600);
        assert_eq!(result.ledger, None);
    }

    #[test]
    fn test_get_transaction_failed_status() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "status": "FAILED",
                "latestLedger": 700,
                "latestLedgerCloseTime": 1711630000,
                "oldestLedger": 300,
                "oldestLedgerCloseTime": 1711620000,
                "ledger": 650,
                "createdAt": "2024-03-28T11:00:00Z",
                "applicationOrder": 5,
                "envelopeXdr": "AAAAAgAAAABmYWlsZWRAdHguY29tAAABkA==",
                "resultXdr": "AAAAAAAAAGT////7AAAAAA==",
                "resultMetaXdr": "AAAAAwAAAAAAAAACAAAAAwAAAoYAAAAAAAAAAA=="
            }
        }"#;

        let resp: JsonRpcResponse<GetTransactionResponse> = serde_json::from_str(json).unwrap();
        let result = resp.result.unwrap();

        assert_eq!(result.status, TransactionStatus::Failed);
        assert_eq!(result.latest_ledger, 700);
        assert_eq!(result.ledger, Some(650));
    }

    #[tokio::test]
    async fn retries_once_on_500_then_succeeds() {
        let responses = vec![
            http_response(500, "Internal Server Error", ""),
            http_response(200, "OK", ok_body()),
        ];
        let addr = spawn_mock_server(responses).await;
        let result = make_client(addr).get_latest_ledger().await;
        assert!(
            result.is_ok(),
            "Expected success after retry, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn exhausts_retries_on_persistent_500() {
        let responses = vec![
            http_response(500, "Internal Server Error", ""),
            http_response(500, "Internal Server Error", ""),
            http_response(500, "Internal Server Error", ""),
            http_response(500, "Internal Server Error", ""),
        ];
        let addr = spawn_mock_server(responses).await;
        let result = make_client(addr).get_latest_ledger().await;
        assert!(result.is_err(), "Expected error after retries exhausted");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Server error") || err.contains("500"),
            "Error should mention the server error, got: {err}"
        );
    }

    #[tokio::test]
    async fn retries_on_503_service_unavailable() {
        let responses = vec![
            http_response(503, "Service Unavailable", ""),
            http_response(503, "Service Unavailable", ""),
            http_response(200, "OK", ok_body()),
        ];
        let addr = spawn_mock_server(responses).await;
        let result = make_client(addr).get_latest_ledger().await;
        assert!(
            result.is_ok(),
            "Expected success after retrying 503s, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn retries_on_502_bad_gateway() {
        let responses = vec![
            http_response(502, "Bad Gateway", ""),
            http_response(200, "OK", ok_body()),
        ];
        let addr = spawn_mock_server(responses).await;
        let result = make_client(addr).get_latest_ledger().await;
        assert!(
            result.is_ok(),
            "Expected success after retrying 502, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn retries_on_429_rate_limit() {
        let responses = vec![
            http_response(429, "Too Many Requests", ""),
            http_response(200, "OK", ok_body()),
        ];
        let addr = spawn_mock_server(responses).await;
        let result = make_client(addr).get_latest_ledger().await;
        assert!(
            result.is_ok(),
            "Expected success after retrying 429, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn does_not_retry_on_4xx_client_error() {
        let bad_body =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid request"}}"#;
        let responses = vec![http_response(400, "Bad Request", bad_body)];
        let addr = spawn_mock_server(responses).await;
        let result = make_client(addr).get_latest_ledger().await;

        assert!(result.is_err(), "Expected error for 4xx response");
    }

    #[tokio::test]
    async fn returns_immediately_on_jsonrpc_error_in_200() {
        let rpc_err = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"not found"}}"#;
        let responses = vec![http_response(200, "OK", rpc_err)];
        let addr = spawn_mock_server(responses).await;
        let result = make_client(addr).get_latest_ledger().await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not found"),
            "Error should propagate the JSON-RPC error message"
        );
    }

    #[tokio::test]
    async fn test_get_ledger_entries_empty_response() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let rpc_url = format!("http://{}", addr);

        let config = NetworkConfig {
            network: crate::network::Network::Testnet,
            rpc_url,
            network_passphrase: "test".to_string(),
            archive_urls: vec![],
            api_key: None,
            request_timeout_secs: 30,
        };
        let client = SorobanRpcClient::new(&config);

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let body = r#"{"jsonrpc":"2.0","id":1,"result":{"latestLedger":123,"entries":[]}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let result = client
            .get_ledger_entries(&["key1".to_string()])
            .await
            .unwrap();
        assert_eq!(result["entries"].as_array().unwrap().len(), 0);
        assert_eq!(result["latestLedger"], 123);
    }

    #[tokio::test]
    async fn test_get_transaction_mocked_response() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let rpc_url = format!("http://{}", addr);

        let config = NetworkConfig {
            network: crate::network::Network::Testnet,
            rpc_url,
            network_passphrase: "test".to_string(),
            archive_urls: vec![],
            api_key: None,
            request_timeout_secs: 30,
        };
        let client = SorobanRpcClient::new(&config);

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let body = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"SUCCESS","latestLedger":123,"latestLedgerCloseTime":1711620000,"ledger":120}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let result = client.get_transaction("hash123").await.unwrap();
        assert_eq!(result.status, TransactionStatus::Success);
        assert_eq!(result.latest_ledger, 123);
        assert_eq!(result.ledger, Some(120));
    }

    #[tokio::test]
    async fn test_client_respects_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let rpc_url = format!("http://{}", addr);

        let config = NetworkConfig {
            network: crate::network::Network::Testnet,
            rpc_url,
            network_passphrase: "test".to_string(),
            archive_urls: vec![],
            api_key: None,
            request_timeout_secs: 1,
        };
        let client = SorobanRpcClient::new(&config);

        tokio::spawn(async move {
            while let Ok((_socket, _)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        let result = client.get_latest_ledger().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.to_lowercase().contains("timeout")
                || err_msg.to_lowercase().contains("error sending request"),
            "Actual error: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_simulate_transaction_returns_rpc_error_on_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let rpc_url = format!("http://{}", addr);

        let config = NetworkConfig {
            network: crate::network::Network::Testnet,
            rpc_url,
            network_passphrase: "test".to_string(),
            archive_urls: vec![],
            api_key: None,
            request_timeout_secs: 30,
        };
        let client = SorobanRpcClient::new(&config);

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let body =
                r#"{"jsonrpc":"2.0","id":1,"result":{"latestLedger":100,"error":"contract trap"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let result = client.simulate_transaction("AAAA").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PrismError::RpcError(msg) => assert!(msg.contains("contract trap")),
            _ => panic!("Expected PrismError::RpcError"),
        }
    }
}
