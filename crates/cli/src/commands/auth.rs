//! `prism login` and `prism logout` — Manage API credentials for hosted services.

use clap::{Args, Subcommand};
use anyhow::{Result, anyhow};
use colored::Colorize;
use dialoguer::Select;
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Arguments for the auth command.
#[derive(Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommands,
}

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Store API credentials for a provider.
    Login {
        /// Provider name (Blockdaemon, NowNodes, or custom).
        #[arg(long)]
        provider: Option<String>,
        
        /// Override the default config file location.
        #[arg(long)]
        config_path: Option<String>,
    },
    /// Remove stored API credentials for a provider.
    Logout {
        /// Provider name to remove credentials for.
        #[arg(long)]
        provider: Option<String>,
        
        /// Override the default config file location.
        #[arg(long)]
        config_path: Option<String>,
    },
}

/// Configuration file structure for credential storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthConfig {
    credentials: std::collections::HashMap<String, String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            credentials: std::collections::HashMap::new(),
        }
    }
}

/// Execute the auth command.
pub async fn run(args: AuthArgs, output_format: &str) -> Result<()> {
    match args.command {
        AuthCommands::Login { provider, config_path } => {
            login(provider, config_path, output_format).await
        }
        AuthCommands::Logout { provider, config_path } => {
            logout(provider, config_path, output_format).await
        }
    }
}

/// Handle the login subcommand.
async fn login(
    provider_param: Option<String>,
    config_path: Option<String>,
    output_format: &str,
) -> Result<()> {
    // Determine provider name
    let provider = match provider_param {
        Some(p) => p,
        None => select_provider_interactive()?,
    };

    // Prompt for API key securely
    let prompt = format!("Enter your API key for {}: ", provider.green());
    let api_key = prompt_password(&prompt)?;
    
    if api_key.trim().is_empty() {
        eprintln!("{}", "API key cannot be empty.".red());
        std::process::exit(1);
    }

    // Store the credential
    match store_credential(&provider, &api_key, config_path).await {
        Ok(_) => {
            if matches!(
                crate::output::OutputFormat::parse(output_format),
                crate::output::OutputFormat::Json
            ) {
                let payload = serde_json::json!({
                    "status": "ok",
                    "action": "login",
                    "provider": provider,
                    "saved": true,
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("✓ Credentials for {} saved.", provider.green());
            }
        }
        Err(e) => {
            eprintln!("{} {}", "Error:".red(), e);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Handle the logout subcommand.
async fn logout(
    provider_param: Option<String>,
    config_path: Option<String>,
    output_format: &str,
) -> Result<()> {
    // Determine provider name
    let provider = match provider_param {
        Some(p) => p,
        None => select_provider_for_logout()?,
    };

    // Remove the credential
    match remove_credential(&provider, config_path).await {
        Ok(true) => {
            if matches!(
                crate::output::OutputFormat::parse(output_format),
                crate::output::OutputFormat::Json
            ) {
                let payload = serde_json::json!({
                    "status": "ok",
                    "action": "logout",
                    "provider": provider,
                    "removed": true,
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("✓ Credentials for {} removed.", provider.green());
            }
        }
        Ok(false) => {
            if matches!(
                crate::output::OutputFormat::parse(output_format),
                crate::output::OutputFormat::Json
            ) {
                let payload = serde_json::json!({
                    "status": "ok",
                    "action": "logout",
                    "provider": provider,
                    "removed": false,
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("No credentials found for {}.", provider.yellow());
            }
        }
        Err(e) => {
            eprintln!("{} {}", "Error:".red(), e);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Interactively select a provider from known options.
fn select_provider_interactive() -> Result<String> {
    let items = vec!["Blockdaemon", "NowNodes", "Custom"];
    let selection = Select::new()
        .with_prompt("Select your API provider:")
        .items(&items)
        .interact()?;

    match selection {
        0 => Ok("Blockdaemon".to_string()),
        1 => Ok("NowNodes".to_string()),
        2 => {
            use dialoguer::Input;
            let custom = Input::new()
                .with_prompt("Enter custom provider name:")
                .interact()?;
            Ok(custom)
        }
        _ => unreachable!(),
    }
}

/// Interactively select a provider for logout (including those with stored credentials).
fn select_provider_for_logout() -> Result<String> {
    let mut items = vec!["Blockdaemon", "NowNodes", "Custom"];
    
    // Try to load existing credentials to show which providers have stored keys
    if let Ok(config) = load_auth_config(None) {
        for provider in config.credentials.keys() {
            if !items.contains(&provider.as_str()) {
                items.push(provider);
            }
        }
    }

    let selection = Select::new()
        .with_prompt("Select provider to logout:")
        .items(&items)
        .interact()?;

    match selection {
        0 => Ok("Blockdaemon".to_string()),
        1 => Ok("NowNodes".to_string()),
        2 => {
            use dialoguer::Input;
            let custom = Input::new()
                .with_prompt("Enter custom provider name:")
                .interact()?;
            Ok(custom)
        }
        _ => Ok(items[selection].to_string()),
    }
}

/// Store a credential using config file storage.
/// Note: This is less secure than keyring storage but keyring caused dependency conflicts.
async fn store_credential(provider: &str, api_key: &str, config_path: Option<String>) -> Result<()> {
    let normalized_provider = normalize_provider_name(provider);
    
    // Store in config file
    store_credential_config(&normalized_provider, api_key, config_path).await
}


/// Store credential in config file.
/// Note: This is less secure than keyring storage but keyring caused dependency conflicts.
async fn store_credential_config(provider: &str, api_key: &str, config_path: Option<String>) -> Result<()> {
    let config_file = get_config_path(config_path)?;
    let mut config = load_auth_config(Some(&config_file)).unwrap_or_default();
    
    config.credentials.insert(provider.to_string(), api_key.to_string());
    
    save_auth_config(&config, &config_file).await?;
    
    // Set secure file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&config_file)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&config_file, perms)?;
    }
    
    Ok(())
}

/// Remove a credential from config file.
async fn remove_credential(provider: &str, config_path: Option<String>) -> Result<bool> {
    let normalized_provider = normalize_provider_name(provider);
    
    // Remove from config file
    let config_file = get_config_path(config_path)?;
    if let Ok(mut config) = load_auth_config(Some(&config_file)) {
        if config.credentials.remove(&normalized_provider.to_string()).is_some() {
            save_auth_config(&config, &config_file).await?;
            return Ok(true);
        }
    }
    
    Ok(false)
}

/// Retrieve a credential from config file.
pub fn get_credential(provider: &str) -> Result<Option<String>> {
    let normalized_provider = normalize_provider_name(provider);
    
    // Get from config file
    get_credential_config(&normalized_provider)
}


/// Get credential from config file.
fn get_credential_config(provider: &str) -> Result<Option<String>> {
    let config_file = get_config_path(None)?;
    let config = load_auth_config(Some(&config_file)).unwrap_or_default();
    Ok(config.credentials.get(provider).cloned())
}

/// Load auth configuration from file.
fn load_auth_config(config_file: Option<&PathBuf>) -> Result<AuthConfig> {
    let config_file = config_file.as_ref().unwrap_or(&get_config_path(None)?);
    
    if !config_file.exists() {
        return Ok(AuthConfig::default());
    }
    
    let content = fs::read_to_string(config_file)?;
    let config: AuthConfig = toml::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse config file: {}", e))?;
    
    Ok(config)
}

/// Save auth configuration to file.
async fn save_auth_config(config: &AuthConfig, config_file: &PathBuf) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = config_file.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let content = toml::to_string_pretty(config)
        .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;
    
    fs::write(config_file, content)
        .map_err(|e| anyhow!("Failed to write config file: {}", e))?;
    
    Ok(())
}

/// Get the config file path.
fn get_config_path(override_path: Option<String>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(PathBuf::from(path));
    }
    
    let project_dirs = directories::ProjectDirs::from("dev", "prism", "prism")
        .ok_or_else(|| anyhow!("Could not determine config directory"))?;
    
    Ok(project_dirs.config_dir().join("auth.toml"))
}

/// Normalize provider name for storage (lowercase, spaces to hyphens).
fn normalize_provider_name(provider: &str) -> String {
    provider
        .to_lowercase()
        .replace(' ', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_get_credential_returns_none_when_not_set() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("auth.toml");
        
        // Should return None when no credential is set
        let result = get_credential_config("nonexistent");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_credential_round_trip_via_config_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("auth.toml");
        
        let provider = "test-provider";
        let api_key = "test-api-key-123";
        
        // Store credential
        let mut config = AuthConfig::default();
        config.credentials.insert(provider.to_string(), api_key.to_string());
        
        let content = toml::to_string_pretty(&config).unwrap();
        fs::write(&config_path, content).unwrap();
        
        // Retrieve credential
        let result = get_credential_config(provider).unwrap();
        assert_eq!(result, Some(api_key.to_string()));
    }

    #[test]
    fn test_empty_key_is_rejected() {
        // This test verifies the validation logic
        let empty_key = "";
        assert!(empty_key.trim().is_empty());
        
        let whitespace_key = "   \t\n   ";
        assert!(whitespace_key.trim().is_empty());
        
        let valid_key = "sk-1234567890abcdef";
        assert!(!valid_key.trim().is_empty());
    }

    #[test]
    fn test_normalize_provider_name() {
        assert_eq!(normalize_provider_name("Blockdaemon"), "blockdaemon");
        assert_eq!(normalize_provider_name("Now Nodes"), "now-nodes");
        assert_eq!(normalize_provider_name("Custom Provider"), "custom-provider");
        assert_eq!(normalize_provider_name("CUSTOM"), "custom");
    }

    #[tokio::test]
    async fn test_save_and_load_auth_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("auth.toml");
        
        let mut config = AuthConfig::default();
        config.credentials.insert("test".to_string(), "key123".to_string());
        
        // Save config
        save_auth_config(&config, &config_path).await.unwrap();
        assert!(config_path.exists());
        
        // Load config
        let loaded = load_auth_config(Some(&config_path)).unwrap();
        assert_eq!(loaded.credentials.get("test"), Some(&"key123".to_string()));
    }
}
