//! Network configuration management.
//!
//! Manages RPC endpoints, archive URLs, network passphrases for
//! mainnet/testnet/futurenet/custom networks.

use crate::types::error::{PrismError, PrismResult};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const MAINNET_RPC_URL: &str = "https://soroban-mainnet.stellar.org";
const TESTNET_RPC_URL: &str = "https://soroban-testnet.stellar.org";
const FUTURENET_RPC_URL: &str = "https://rpc-futurenet.stellar.org";
const LOCAL_RPC_URL: &str = "http://127.0.0.1:8000/rpc";

const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";
const FUTURENET_PASSPHRASE: &str = "Test SDF Future Network ; October 2022";
const LOCAL_PASSPHRASE: &str = "Standalone Network ; February 2017";

const MAINNET_ARCHIVE_URLS: [&str; 1] = ["https://history.stellar.org/prd/core-live/core_live_001"];
const TESTNET_ARCHIVE_URLS: [&str; 1] =
    ["https://history.stellar.org/prd/core-testnet/core_testnet_001"];
const FUTURENET_ARCHIVE_URLS: [&str; 1] = ["https://history-futurenet.stellar.org"];

/// Supported Stellar networks.
///
/// `Custom` is intentionally flexible so callers can target local or bespoke
/// Soroban/Stellar deployments without duplicating preset-selection logic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Network {
    Mainnet,
    Testnet,
    Futurenet,
    Custom(String),
}

impl Network {
    /// Conventional name used for the default local standalone network.
    pub const LOCAL: &str = "local";

    /// Parse a network selector into a strongly typed network.
    ///
    /// Accepts preset aliases such as `mainnet`, `testnet`, `futurenet`, and
    /// local aliases such as `local`, `localhost`, and `standalone`.
    /// Any HTTP(S) string is treated as a custom RPC target.
    pub fn parse(value: &str) -> PrismResult<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(PrismError::ConfigError(
                "network selector cannot be empty".to_string(),
            ));
        }

        let normalized = trimmed.to_ascii_lowercase();
        let network = match normalized.as_str() {
            "mainnet" | "main" | "pubnet" | "public" => Self::Mainnet,
            "testnet" | "test" => Self::Testnet,
            "futurenet" | "future" => Self::Futurenet,
            "local" | "localhost" | "standalone" => Self::Custom(Self::LOCAL.to_string()),
            _ if trimmed.starts_with("http://") || trimmed.starts_with("https://") => {
                Self::Custom(trimmed.to_string())
            }
            _ => Self::Custom(trimmed.to_string()),
        };

        Ok(network)
    }

    /// Canonical identifier for logging, config serialization, and display.
    pub fn as_key(&self) -> &str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Futurenet => "futurenet",
            Self::Custom(name) => name.as_str(),
        }
    }

    /// True when this network targets a local standalone deployment.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Custom(name) if name.eq_ignore_ascii_case(Self::LOCAL))
    }

    /// Build the default connection settings for this network target.
    pub fn config(&self) -> NetworkConfig {
        NetworkConfig::for_network(self.clone())
    }
}

impl Default for Network {
    fn default() -> Self {
        Self::Testnet
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_key())
    }
}

impl FromStr for Network {
    type Err = PrismError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for Network {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_key())
    }
}

impl<'de> Deserialize<'de> for Network {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Configuration for connecting to a Stellar network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// The network to connect to.
    pub network: Network,
    /// Soroban RPC endpoint URL.
    pub rpc_url: String,
    /// Network passphrase.
    pub network_passphrase: String,
    /// History archive URL(s).
    pub archive_urls: Vec<String>,
}

impl NetworkConfig {
    /// Create configuration for Stellar testnet.
    pub fn testnet() -> Self {
        Self {
            network: Network::Testnet,
            rpc_url: TESTNET_RPC_URL.to_string(),
            network_passphrase: TESTNET_PASSPHRASE.to_string(),
            archive_urls: TESTNET_ARCHIVE_URLS
                .iter()
                .map(|url| (*url).to_string())
                .collect(),
        }
    }

    /// Create configuration for Stellar mainnet.
    pub fn mainnet() -> Self {
        Self {
            network: Network::Mainnet,
            rpc_url: MAINNET_RPC_URL.to_string(),
            network_passphrase: MAINNET_PASSPHRASE.to_string(),
            archive_urls: MAINNET_ARCHIVE_URLS
                .iter()
                .map(|url| (*url).to_string())
                .collect(),
        }
    }

    /// Create configuration for Stellar futurenet.
    pub fn futurenet() -> Self {
        Self {
            network: Network::Futurenet,
            rpc_url: FUTURENET_RPC_URL.to_string(),
            network_passphrase: FUTURENET_PASSPHRASE.to_string(),
            archive_urls: FUTURENET_ARCHIVE_URLS
                .iter()
                .map(|url| (*url).to_string())
                .collect(),
        }
    }

    /// Create configuration for a local standalone Soroban RPC.
    pub fn local() -> Self {
        Self {
            network: Network::Custom(Network::LOCAL.to_string()),
            rpc_url: LOCAL_RPC_URL.to_string(),
            network_passphrase: LOCAL_PASSPHRASE.to_string(),
            archive_urls: Vec::new(),
        }
    }

    /// Create a custom network configuration.
    pub fn custom(
        network_name: impl Into<String>,
        rpc_url: impl Into<String>,
        passphrase: impl Into<String>,
    ) -> Self {
        Self {
            network: Network::Custom(network_name.into()),
            rpc_url: rpc_url.into(),
            network_passphrase: passphrase.into(),
            archive_urls: Vec::new(),
        }
    }

    /// Attach history archive URLs to a configuration.
    pub fn with_archive_urls(mut self, archive_urls: Vec<String>) -> Self {
        self.archive_urls = archive_urls;
        self
    }

    /// Build the default configuration for a typed network target.
    pub fn for_network(network: Network) -> Self {
        match network {
            Network::Mainnet => Self::mainnet(),
            Network::Testnet => Self::testnet(),
            Network::Futurenet => Self::futurenet(),
            Network::Custom(name) if name.eq_ignore_ascii_case(Network::LOCAL) => Self::local(),
            Network::Custom(name)
                if name.starts_with("http://") || name.starts_with("https://") =>
            {
                Self::custom(name.clone(), name, "")
            }
            Network::Custom(name) => Self::custom(name, "", ""),
        }
    }
}
use crate::types::config::NetworkConfig;

/// Resolve a network name string to a `NetworkConfig`.
///
/// Accepts preset names, local aliases, or a custom RPC URL.
pub fn resolve_network(network_str: &str) -> NetworkConfig {
    match resolve_network_target(network_str) {
        Ok(network) => NetworkConfig::for_network(network),
        Err(error) => {
            tracing::warn!(%error, network = network_str, "Unknown network, defaulting to testnet");
            NetworkConfig::testnet()
    match network_str.to_lowercase().as_str() {
        "mainnet" | "main" | "pubnet" => NetworkConfig::mainnet(),
        "testnet" | "test" => NetworkConfig::testnet(),
        "futurenet" | "future" => NetworkConfig::futurenet(),
        url if url.starts_with("http") => NetworkConfig::custom(url, ""),
        _ => {
            tracing::warn!("Unknown network '{network_str}', defaulting to testnet");
            default_network()
        }
    }
}

/// Resolve a network selector to a strongly typed target.
pub fn resolve_network_target(network_str: &str) -> PrismResult<Network> {
    Network::parse(network_str)
}

/// Get the default network configuration.
pub fn default_network() -> NetworkConfig {
    Network::default().config()
}

/// Validate that a network configuration is reachable.
#[allow(dead_code)]
pub async fn validate_network(config: &NetworkConfig) -> bool {
    let timeout = Duration::from_secs(config.request_timeout_secs);
    let transport = JsonRpcTransport::new(&config.rpc_url, 0, timeout);
    let req = JsonRpcRequest::new(1, "getHealth", GetHealthParams {});
    transport
        .call::<_, serde_json::Value>(&req)
        .await
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_network_aliases() {
        assert_eq!(Network::parse("main").unwrap(), Network::Mainnet);
        assert_eq!(Network::parse("testnet").unwrap(), Network::Testnet);
        assert_eq!(Network::parse("future").unwrap(), Network::Futurenet);
    }

    #[test]
    fn parses_local_aliases_as_custom_local_network() {
        assert_eq!(
            Network::parse("standalone").unwrap(),
            Network::Custom(Network::LOCAL.to_string())
        );
        assert!(Network::parse("local").unwrap().is_local());
    }

    #[test]
    fn resolves_local_network_defaults() {
        let config = resolve_network("local");

        assert!(config.network.is_local());
        assert_eq!(config.rpc_url, LOCAL_RPC_URL);
        assert_eq!(config.network_passphrase, LOCAL_PASSPHRASE);
        assert!(config.archive_urls.is_empty());
    }

    #[test]
    fn resolves_custom_rpc_url_without_losing_identity() {
        let rpc_url = "http://127.0.0.1:9000/rpc";
        let config = resolve_network(rpc_url);

        assert_eq!(config.network, Network::Custom(rpc_url.to_string()));
        assert_eq!(config.rpc_url, rpc_url);
        assert!(config.network_passphrase.is_empty());
    }

    #[test]
    fn serializes_network_as_string_key() {
        let serialized = serde_json::to_string(&Network::Custom("local-dev".to_string()))
            .expect("network should serialize");
        assert_eq!(serialized, "\"local-dev\"");

        let parsed: Network = serde_json::from_str("\"testnet\"").expect("network should parse");
        assert_eq!(parsed, Network::Testnet);
    }
}
