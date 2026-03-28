//! Network and application configuration types.

pub use crate::network::config::{Network, NetworkConfig};
use serde::{Deserialize, Serialize};

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
                NetworkConfig::local(),
            ],
            cache_dir: None,
            max_cache_size_mb: 512,
        }
    }
}
