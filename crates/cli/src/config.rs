//! Global Prism configuration manager.
//!
//! Handles reading and writing user preferences at `~/.prism/config.toml`.

use anyhow::Context;
use directories::BaseDirs;
use prism_core::types::config::PrismConfig;
use std::path::{Path, PathBuf};

/// Reads and writes Prism's global configuration file.
#[derive(Debug, Clone)]
pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    /// Create a config manager using the default global config location.
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            config_path: default_config_path()?,
        })
    }

    /// Create a config manager using an explicit config file path.
    ///
    /// Useful for tests and tooling that need an isolated config file.
    pub fn with_path(config_path: PathBuf) -> Self {
        Self { config_path }
    }

    /// Return the full path to the config file.
    pub fn path(&self) -> &Path {
        &self.config_path
    }

    /// Load config from disk, returning defaults when the file does not exist.
    pub fn load(&self) -> anyhow::Result<PrismConfig> {
        if !self.config_path.exists() {
            return Ok(PrismConfig::default());
        }

        let content = std::fs::read_to_string(&self.config_path).with_context(|| {
            format!("Failed to read config file {}", self.config_path.display())
        })?;

        let config: PrismConfig = toml::from_str(&content).with_context(|| {
            format!(
                "Failed to parse config file {} as TOML",
                self.config_path.display()
            )
        })?;

        Ok(config)
    }

    /// Save config to disk in TOML format.
    pub fn save(&self, config: &PrismConfig) -> anyhow::Result<()> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory {}", parent.display())
            })?;
        }

        let serialized =
            toml::to_string_pretty(config).context("Failed to serialize Prism config to TOML")?;

        std::fs::write(&self.config_path, serialized).with_context(|| {
            format!("Failed to write config file {}", self.config_path.display())
        })?;

        Ok(())
    }
}

fn default_config_path() -> anyhow::Result<PathBuf> {
    let base_dirs = BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory for Prism config"))?;

    Ok(base_dirs.home_dir().join(".prism").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "prism_cli_config_test_{}_{}",
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
            PrismConfig::default().default_network
        );
        assert!(!path.exists());
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let root = unique_path("roundtrip");
        let path = root.join("config.toml");
        let manager = ConfigManager::with_path(path.clone());

        let mut config = PrismConfig::default();
        config.max_cache_size_mb = 1024;

        manager.save(&config).expect("save config");
        let loaded = manager.load().expect("load config");

        assert_eq!(loaded.max_cache_size_mb, 1024);
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn default_path_uses_prism_config_toml() {
        let manager = ConfigManager::new().expect("manager with default path");

        let path = manager.path().to_string_lossy();
        assert!(path.ends_with(".prism/config.toml") || path.ends_with(".prism\\config.toml"));
    }
}
