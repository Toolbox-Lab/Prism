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
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            config_path: default_config_path()?,
        })
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
        let manager = ConfigManager::new().expect("manager with default path");

        let path = manager.path().to_string_lossy();
        assert!(path.ends_with(".grat/config.toml") || path.ends_with(".grat\\config.toml"));
    }

    #[test]
    fn load_strict_errors_when_file_missing() {
        let path = unique_path("strict_missing").join("no_such_file.toml");
        let manager = ConfigManager::with_path(path.clone());

        let result = manager.load_strict();

        assert!(result.is_err(), "expected Err when file does not exist");
        let msg = format!("{:#}", result.unwrap_err());
        assert!(
            msg.contains(&path.to_string_lossy().to_string()),
            "error message should contain the path; got: {msg}"
        );
    }

    #[test]
    fn load_strict_errors_on_invalid_toml() {
        let path = unique_path("strict_bad_toml");
        std::fs::write(&path, b"not valid toml ]][\n").expect("write bad toml");

        let manager = ConfigManager::with_path(path.clone());
        let result = manager.load_strict();

        let _ = std::fs::remove_file(&path);

        assert!(result.is_err(), "expected Err for invalid TOML");
        let msg = format!("{:#}", result.unwrap_err());
        assert!(
            msg.contains(&path.to_string_lossy().to_string()),
            "error message should contain the path; got: {msg}"
        );
    }

    #[test]
    fn load_strict_succeeds_on_valid_toml() {
        let path = unique_path("strict_valid_toml.toml");
        let toml_content = r#"
default_network = "testnet"
max_cache_size_mb = 256
networks = []
"#;
        std::fs::write(&path, toml_content).expect("write valid toml");

        let manager = ConfigManager::with_path(path.clone());
        let result = manager.load_strict();

        let _ = std::fs::remove_file(&path);

        let config = result.expect("load_strict should succeed on valid TOML");
        assert_eq!(config.max_cache_size_mb, 256);
    }
}
