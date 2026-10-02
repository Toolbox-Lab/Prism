#![allow(dead_code)]

use anyhow::Context;
use directories::BaseDirs;
use grat_core::types::config::GratConfig;
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

fn default_config_path() -> anyhow::Result<PathBuf> {
    let base_dirs = BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory for Grat config"))?;

    Ok(base_dirs.home_dir().join(".grat").join("config.toml"))
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
}