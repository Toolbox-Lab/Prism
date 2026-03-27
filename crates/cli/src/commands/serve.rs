//! `prism serve` - Local JSON-RPC bridge for the Web UI.

use anyhow::Context;
use clap::Args;
use prism_core::network::rpc::RpcClient;
use prism_core::types::config::NetworkConfig;
use prism_core::types::error::PrismError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

const JSON_RPC_VERSION: &str = "2.0";

#[derive(Args, Debug, Clone)]
pub struct ServeArgs {
    /// Host interface for the local bridge.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port for the local bridge.
    #[arg(long, short, default_value_t = 4040)]
    pub port: u16,
}

pub async fn run(args: ServeArgs, network: &NetworkConfig) -> anyhow::Result<()> {
    let bind_addr = format!("{}:{}", args.host, args.port);
    let listener = TcpListener::bind(&bind_addr)
        .with_context(|| format!("failed to bind local bridge on {bind_addr}"))?;
    listener
        .set_nonblocking(false)
        .context("failed to configure local bridge listener")?;

    let bridge = ApiBridge::new(network.clone());

    println!(
        "Prism API bridge listening on http://{} (JSON-RPC at /rpc)",
        bind_addr
    );

    loop {
        let (mut stream, remote_addr) = listener.accept().context("failed to accept connection")?;
        if let Err(err) = handle_connection(&mut stream, remote_addr, &bridge).await {
            tracing::warn!(error = %err, ?remote_addr, "failed to handle API bridge request");
        }
    }
}

async fn handle_connection(
    stream: &mut TcpStream,
    remote_addr: SocketAddr,
    bridge: &ApiBridge,
) -> anyhow::Result<()> {
    let request = match HttpRequest::read_from(stream)? {
        Some(request) => request,
        None => return Ok(()),
    };

    tracing::debug!(
        ?remote_addr,
        method = %request.method,
        path = %request.path,
        "received bridge request"
    );

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => {
            write_json_response(
                stream,
                200,
                &json!({
                    "status": "ok",
                    "version": prism_core::VERSION,
                    "jsonrpc": JSON_RPC_VERSION,
                }),
            )?;
        }
        ("OPTIONS", "/rpc") | ("OPTIONS", "/health") => {
            write_empty_response(stream, 204)?;
        }
        ("POST", "/rpc") => {
            let response = match serde_json::from_slice::<JsonRpcRequest>(&request.body) {
                Ok(rpc_request) => bridge.handle(rpc_request).await,
                Err(err) => JsonRpcResponse::error(
                    Value::Null,
                    -32700,
                    "Parse error",
                    Some(json!({ "details": err.to_string() })),
                ),
            };
            write_json_response(stream, 200, &serde_json::to_value(response)?)?;
        }
        _ => {
            write_json_response(
                stream,
                404,
                &json!({
                    "error": "not_found",
                    "message": "Route not found",
                }),
            )?;
        }
    }

    Ok(())
}

struct ApiBridge {
    default_network: NetworkConfig,
}

impl ApiBridge {
    fn new(default_network: NetworkConfig) -> Self {
        Self { default_network }
    }

    async fn handle(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        if request.jsonrpc != JSON_RPC_VERSION {
            return JsonRpcResponse::error(
                request.id,
                -32600,
                "Invalid Request",
                Some(json!({ "expected": JSON_RPC_VERSION })),
            );
        }

        let result = match request.method.as_str() {
            "prism.health" => Ok(json!({
                "status": "ok",
                "version": prism_core::VERSION,
            })),
            "prism.transaction.get" => self.get_transaction(&request.params).await,
            "prism.decode" => self.decode_transaction(&request.params).await,
            "prism.inspect" => self.inspect_transaction(&request.params).await,
            "prism.trace" => self.trace_transaction(&request.params).await,
            "prism.profile" => self.profile_transaction(&request.params).await,
            "prism.diff" => self.diff_transaction(&request.params).await,
            _ => Err(JsonRpcError::method_not_found(&request.method)),
        };

        match result {
            Ok(result) => JsonRpcResponse::success(request.id, result),
            Err(err) => JsonRpcResponse::error(request.id, err.code, &err.message, err.data),
        }
    }

    async fn get_transaction(&self, params: &Value) -> Result<Value, JsonRpcError> {
        let tx_hash = required_string(params, "txHash")?;
        let network = self.resolve_network(params);
        let rpc = RpcClient::new(network);
        rpc.get_transaction(&tx_hash)
            .await
            .map(|transaction| {
                json!({
                    "txHash": tx_hash,
                    "transaction": transaction,
                })
            })
            .map_err(map_prism_error)
    }

    async fn decode_transaction(&self, params: &Value) -> Result<Value, JsonRpcError> {
        let tx_hash = required_string(params, "txHash")?;
        let network = self.resolve_network(params);
        prism_core::decode::decode_transaction(&tx_hash, &network)
            .await
            .map(|report| {
                json!({
                    "txHash": tx_hash,
                    "report": report,
                })
            })
            .map_err(map_prism_error)
    }

    async fn inspect_transaction(&self, params: &Value) -> Result<Value, JsonRpcError> {
        let tx_hash = required_string(params, "txHash")?;
        let network = self.resolve_network(params);
        prism_core::decode::decode_transaction(&tx_hash, &network)
            .await
            .map(|report| {
                let context = report.transaction_context.clone();
                json!({
                    "txHash": tx_hash,
                    "report": report,
                    "transactionContext": context,
                })
            })
            .map_err(map_prism_error)
    }

    async fn trace_transaction(&self, params: &Value) -> Result<Value, JsonRpcError> {
        let tx_hash = required_string(params, "txHash")?;
        let network = self.resolve_network(params);
        prism_core::replay::replay_transaction(&tx_hash, &network)
            .await
            .map(|trace| {
                json!({
                    "txHash": tx_hash,
                    "trace": trace,
                })
            })
            .map_err(map_prism_error)
    }

    async fn profile_transaction(&self, params: &Value) -> Result<Value, JsonRpcError> {
        let tx_hash = required_string(params, "txHash")?;
        let network = self.resolve_network(params);
        prism_core::replay::replay_transaction(&tx_hash, &network)
            .await
            .map(|trace| {
                json!({
                    "txHash": tx_hash,
                    "resourceProfile": trace.resource_profile,
                })
            })
            .map_err(map_prism_error)
    }

    async fn diff_transaction(&self, params: &Value) -> Result<Value, JsonRpcError> {
        let tx_hash = required_string(params, "txHash")?;
        let network = self.resolve_network(params);
        prism_core::replay::replay_transaction(&tx_hash, &network)
            .await
            .map(|trace| {
                json!({
                    "txHash": tx_hash,
                    "stateDiff": trace.state_diff,
                })
            })
            .map_err(map_prism_error)
    }

    fn resolve_network(&self, params: &Value) -> NetworkConfig {
        params
            .get("network")
            .and_then(Value::as_str)
            .map(prism_core::network::config::resolve_network)
            .unwrap_or_else(|| self.default_network.clone())
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcErrorBody>,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JSON_RPC_VERSION,
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Self {
        Self {
            jsonrpc: JSON_RPC_VERSION,
            id,
            result: None,
            error: Some(JsonRpcErrorBody {
                code,
                message: message.to_string(),
                data,
            }),
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonRpcErrorBody {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl JsonRpcError {
    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {method}"),
            data: None,
        }
    }
}

fn map_prism_error(error: PrismError) -> JsonRpcError {
    let (code, message) = match error {
        PrismError::TransactionNotFound(hash) => (-32004, format!("Transaction not found: {hash}")),
        PrismError::ConfigError(message) => (-32010, message),
        PrismError::RpcError(message) => (-32020, message),
        other => (-32000, other.to_string()),
    };

    JsonRpcError {
        code,
        message,
        data: None,
    }
}

fn required_string(params: &Value, field: &str) -> Result<String, JsonRpcError> {
    params
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            JsonRpcError::invalid_params(format!("missing required string field `{field}`"))
        })
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

impl HttpRequest {
    fn read_from(stream: &mut TcpStream) -> anyhow::Result<Option<Self>> {
        let mut buffer = Vec::new();
        let mut temp = [0u8; 1024];
        let mut header_end = None;

        loop {
            let read = stream.read(&mut temp)?;
            if read == 0 {
                if buffer.is_empty() {
                    return Ok(None);
                }
                break;
            }

            buffer.extend_from_slice(&temp[..read]);
            header_end = find_header_end(&buffer);
            if header_end.is_some() {
                break;
            }

            if buffer.len() > 1024 * 1024 {
                anyhow::bail!("request headers too large");
            }
        }

        let header_end = header_end.context("malformed HTTP request")?;
        let headers = &buffer[..header_end];
        let header_text =
            String::from_utf8(headers.to_vec()).context("request headers were not valid UTF-8")?;
        let mut lines = header_text.lines();
        let request_line = lines.next().context("missing HTTP request line")?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().context("missing HTTP method")?.to_string();
        let path = parts.next().context("missing HTTP path")?.to_string();

        let mut content_length = 0usize;
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.trim().parse().context("invalid content-length")?;
                }
            }
        }

        let body_start = header_end + 4;
        let mut body = buffer[body_start..].to_vec();
        while body.len() < content_length {
            let read = stream.read(&mut temp)?;
            if read == 0 {
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "request body ended before content-length bytes were read",
                )
                .into());
            }
            body.extend_from_slice(&temp[..read]);
        }
        body.truncate(content_length);

        Ok(Some(Self { method, path, body }))
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_json_response(stream: &mut TcpStream, status: u16, body: &Value) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(body)?;
    write_response(stream, status, "application/json", &payload)
}

fn write_empty_response(stream: &mut TcpStream, status: u16) -> anyhow::Result<()> {
    write_response(stream, status, "text/plain", &[])
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> anyhow::Result<()> {
    let status_text = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        _ => "OK",
    };

    let headers = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nConnection: close\r\n\r\n",
        body.len()
    );

    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_header_boundary() {
        let request = b"POST /rpc HTTP/1.1\r\nHost: localhost\r\n\r\n{}";
        assert_eq!(find_header_end(request), Some(35));
    }

    #[test]
    fn validates_required_string_param() {
        let params = json!({ "txHash": "abc123" });
        assert_eq!(required_string(&params, "txHash").unwrap(), "abc123");
        assert!(required_string(&json!({}), "txHash").is_err());
    }

    #[tokio::test]
    async fn returns_method_not_found_for_unknown_calls() {
        let bridge = ApiBridge::new(prism_core::network::config::resolve_network("testnet"));
        let response = bridge
            .handle(JsonRpcRequest {
                jsonrpc: JSON_RPC_VERSION.to_string(),
                id: json!(1),
                method: "prism.unknown".to_string(),
                params: json!({}),
            })
            .await;

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn returns_health_payload() {
        let bridge = ApiBridge::new(prism_core::network::config::resolve_network("testnet"));
        let response = bridge
            .handle(JsonRpcRequest {
                jsonrpc: JSON_RPC_VERSION.to_string(),
                id: json!("health"),
                method: "prism.health".to_string(),
                params: json!({}),
            })
            .await;

        assert_eq!(
            response.result.unwrap()["status"],
            Value::String("ok".to_string())
        );
    }
}
