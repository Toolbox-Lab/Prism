#![allow(dead_code)]

use anyhow::Context;
use directories::BaseDirs;
use grat_core::types::config::{GratConfig, NetworkConfig};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    /// Create a new ConfigManager. If `custom_path` is `Some`, that path is used
    /// verbatim; otherwise falls back to the default `~/.grat/config.toml` location.
    pub fn new(custom_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let config_path = match custom_path {
            Some(path) => path,
            None => default_config_path()?,
        };
        Ok(Self { config_path })
    }

    #[cfg(test)]
    pub fn with_path(config_path: PathBuf) -> Self {
        Self { config_path }
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.config_path
    }

    pub fn load(&self) -> anyhow::Result<GratConfig> {
        if !self.config_path.exists() {
            return Ok(GratConfig::default());
        }

        let content = std::fs::read_to_string(&self.config_path).with_context(|| {
            format!("Failed to read config file {}", self.config_path.display())
        })?;

        let config: GratConfig = toml::from_str(&content).with_context(|| {
            format!(
                "Failed to parse config file {} as TOML",
                self.config_path.display()
            )
        })?;

        Ok(config)
    }

    #[cfg(test)]
    pub fn save(&self, config: &GratConfig) -> anyhow::Result<()> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory {}", parent.display())
            })?;
        }

        let serialized =
            toml::to_string_pretty(config).context("Failed to serialize Grat config to TOML")?;

        std::fs::write(&self.config_path, serialized).with_context(|| {
            format!("Failed to write config file {}", self.config_path.display())
        })?;

        Ok(())
    }
}

/// Load configuration. A missing default config file yields defaults, but an
/// explicitly supplied `--config-path` that is missing or malformed is an error.
pub fn load_config(custom_path: Option<&std::path::Path>) -> anyhow::Result<GratConfig> {
    if let Some(path) = custom_path {
        if !path.exists() {
            anyhow::bail!("Config file {} does not exist", path.display());
        }
    }
    ConfigManager::new(custom_path.map(PathBuf::from))?.load()
}

/// Network name to use: the CLI flag wins, then the config default.
pub fn effective_network_name(cli_network: Option<&str>, config: &GratConfig) -> String {
    match cli_network {
        Some(name) => name.to_string(),
        None => config.default_network.as_key().to_string(),
    }
}

/// Resolve a network, preferring a matching entry from the user's config
/// (so a configured custom `rpc_url` applies) over the built-in defaults.
pub fn resolve_network_config(name: &str, config: &GratConfig) -> NetworkConfig {
    let builtin = grat_core::network::config::resolve_network(name);
    config
        .networks
        .iter()
        .find(|n| n.network == builtin.network)
        .cloned()
        .unwrap_or(builtin)
}

fn default_config_path() -> anyhow::Result<PathBuf> {
    let home_dir = BaseDirs::new()
        .map(|base_dirs| base_dirs.home_dir().to_path_buf())
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory for Grat config"))?;

    Ok(home_dir.join(".grat").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "grat_cli_config_test_{}_{}",
            name,
            std::process::id()
        ))
    }

    #[test]
    fn load_returns_default_when_file_missing() {
        let path = unique_path("missing").join("config.toml");
        let manager = ConfigManager::with_path(path.clone());

        let loaded = manager.load().expect("load default config");

        assert_eq!(
            loaded.default_network,
            GratConfig::default().default_network
        );
        assert!(!path.exists());
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let root = unique_path("roundtrip");
        let path = root.join("config.toml");
        let manager = ConfigManager::with_path(path.clone());

        let config = GratConfig {
            max_cache_size_mb: 1024,
            ..GratConfig::default()
        };

        manager.save(&config).expect("save config");
        let loaded = manager.load().expect("load config");

        assert_eq!(loaded.max_cache_size_mb, 1024);
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn default_path_uses_grat_config_toml() {
        let manager = ConfigManager::new(None).expect("manager with default path");

        let path = manager.path().to_string_lossy();
        assert!(path.ends_with(".grat/config.toml") || path.ends_with(".grat\\config.toml"));
    }

    #[test]
    fn new_uses_custom_path_when_provided() {
        let path = unique_path("custom_override").join("custom_config.toml");
        let manager = ConfigManager::new(Some(path.clone())).expect("manager with custom path");

        assert_eq!(manager.path(), path.as_path());
    }

    #[test]
    fn explicit_missing_config_path_is_an_error() {
        let path = unique_path("explicit_missing").join("nope.toml");
        assert!(load_config(Some(&path)).is_err());
    }

    #[test]
    fn default_config_is_used_without_custom_path_file() {
        let cfg = GratConfig::default();
        assert_eq!(effective_network_name(None, &cfg), "testnet");
        assert_eq!(effective_network_name(Some("mainnet"), &cfg), "mainnet");
    }

    #[test]
    fn config_default_network_and_rpc_url_are_applied() {
        let mut cfg = GratConfig::default();
        cfg.default_network = grat_core::types::config::Network::Mainnet;
        for n in cfg.networks.iter_mut() {
            if n.network == grat_core::types::config::Network::Mainnet {
                n.rpc_url = "https://custom.example/rpc".to_string();
            }
        }
        let name = effective_network_name(None, &cfg);
        assert_eq!(name, "mainnet");
        assert_eq!(
            resolve_network_config(&name, &cfg).rpc_url,
            "https://custom.example/rpc"
        );
        assert_eq!(
            resolve_network_config("testnet", &cfg).rpc_url,
            grat_core::network::config::resolve_network("testnet").rpc_url
        );
    }

    #[test]
    fn load_config_from_custom_toml() {
        let root = unique_path("custom_toml");
        let path = root.join("c.toml");
        ConfigManager::with_path(path.clone())
            .save(&GratConfig {
                max_cache_size_mb: 7,
                ..GratConfig::default()
            })
            .unwrap();
        assert_eq!(load_config(Some(&path)).unwrap().max_cache_size_mb, 7);
        let _ = std::fs::remove_dir_all(root);
    }
}
