//! Network and application configuration types.

use serde::{Deserialize, Serialize};

/// Supported Stellar networks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Mainnet,
    Testnet,
    Futurenet,
    Standalone,
    Custom,
}

impl Default for Network {
    fn default() -> Self {
        Self::Testnet
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
            rpc_url: "https://soroban-testnet.stellar.org".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            archive_urls: vec![
                "https://history.stellar.org/prd/core-testnet/core_testnet_001".to_string(),
            ],
        }
    }

    /// Create configuration for Stellar mainnet.
    pub fn mainnet() -> Self {
        Self {
            network: Network::Mainnet,
            rpc_url: "https://soroban-mainnet.stellar.org".to_string(),
            network_passphrase: "Public Global Stellar Network ; September 2015".to_string(),
            archive_urls: vec![
                "https://history.stellar.org/prd/core-live/core_live_001".to_string()
            ],
        }
    }

    /// Create configuration for Stellar futurenet.
    pub fn futurenet() -> Self {
        Self {
            network: Network::Futurenet,
            rpc_url: "https://rpc-futurenet.stellar.org".to_string(),
            network_passphrase: "Test SDF Future Network ; October 2022".to_string(),
            archive_urls: vec!["https://history-futurenet.stellar.org".to_string()],
        }
    }

    /// Create a custom network configuration.
    pub fn custom(rpc_url: &str, passphrase: &str) -> Self {
        Self {
            network: Network::Custom,
            rpc_url: rpc_url.to_string(),
            network_passphrase: passphrase.to_string(),
            archive_urls: Vec::new(),
        }
    }
}

/// Global Prism configuration loaded from disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrismConfig {
    /// Default network to use.
    pub default_network: Network,
    /// Custom network configurations.
    pub networks: Vec<NetworkConfig>,
    /// Local cache directory override.
    pub cache_dir: Option<String>,
    /// Maximum cache size in MB.
    pub max_cache_size_mb: u64,
}

impl Default for PrismConfig {
    fn default() -> Self {
        Self {
            default_network: Network::Testnet,
            networks: vec![
                NetworkConfig::testnet(),
                NetworkConfig::mainnet(),
                NetworkConfig::futurenet(),
            ],
            cache_dir: None,
            max_cache_size_mb: 512,
        }
    }
}
