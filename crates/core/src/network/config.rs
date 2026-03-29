//! Network configuration management.
//!
//! Manages RPC endpoints, archive URLs, network passphrases for
//! mainnet/testnet/futurenet/standalone networks.

use crate::types::config::NetworkConfig;

/// Resolve a network name string to a `NetworkConfig`.
///
/// Accepts: "mainnet", "testnet", "futurenet", or a custom RPC URL.
pub fn resolve_network(network_str: &str) -> NetworkConfig {
    match network_str.to_lowercase().as_str() {
        "mainnet" | "main" | "pubnet" => NetworkConfig::mainnet(),
        "testnet" | "test" => NetworkConfig::testnet(),
        "futurenet" | "future" => NetworkConfig::futurenet(),
        url if url.starts_with("http") => NetworkConfig::custom(url, ""),
        _ => {
            tracing::warn!("Unknown network '{network_str}', defaulting to testnet");
            NetworkConfig::testnet()
        }
    }
}

/// Get the default network configuration.
pub fn default_network() -> NetworkConfig {
    NetworkConfig::testnet()
}

/// Validate that a network configuration is reachable.
pub async fn validate_network(config: &NetworkConfig) -> bool {
    let client = reqwest::Client::new();
    client
        .post(&config.rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getHealth",
            "params": {}
        }))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
