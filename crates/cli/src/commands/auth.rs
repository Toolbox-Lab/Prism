use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};
use dialoguer::Select;
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommands,
}

#[derive(Subcommand)]
pub enum AuthCommands {
    Login {
        #[arg(long)]
        provider: Option<String>,

        #[arg(long)]
        config_path: Option<String>,
    },

    Logout {
        #[arg(long)]
        provider: Option<String>,

        #[arg(long)]
        config_path: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AuthConfig {
    credentials: std::collections::HashMap<String, String>,
}

pub async fn run(args: AuthArgs, output_format: &str) -> Result<()> {
    match args.command {
        AuthCommands::Login {
            provider,
            config_path,
        } => login(provider, config_path, output_format).await,
        AuthCommands::Logout {
            provider,
            config_path,
        } => logout(provider, config_path, output_format).await,
    }
}

async fn login(
    provider_param: Option<String>,
    config_path: Option<String>,
    output_format: &str,
) -> Result<()> {
    let palette = crate::output::theme::ColorPalette::default();

    let provider = match provider_param {
        Some(p) => p,
        None => select_provider_interactive()?,
    };

    let prompt = format!(
        "Enter your API key for {}: ",
        palette.success_text(&provider)
    );
    let api_key = prompt_password(&prompt)?;

    if api_key.trim().is_empty() {
        eprintln!("{}", palette.error_text("API key cannot be empty."));
        std::process::exit(1);
    }

    match store_credential(&provider, &api_key, config_path).await {
        Ok(()) => {
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
                println!(
                    "✓ Credentials for {} saved.",
                    palette.success_text(&provider)
                );
            }
        }
        Err(e) => {
            eprintln!("{} {}", palette.error_text("Error:"), e);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn logout(
    provider_param: Option<String>,
    config_path: Option<String>,
    output_format: &str,
) -> Result<()> {
    let palette = crate::output::theme::ColorPalette::default();

    let provider = match provider_param {
        Some(p) => p,
        None => select_provider_for_logout()?,
    };

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
                println!(
                    "✓ Credentials for {} removed.",
                    palette.success_text(&provider)
                );
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
                println!(
                    "No credentials found for {}.",
                    palette.warning_text(&provider)
                );
            }
        }
        Err(e) => {
            eprintln!("{} {}", palette.error_text("Error:"), e);
            std::process::exit(1);
        }
    }

    Ok(())
}

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

fn select_provider_for_logout() -> Result<String> {
    let mut items: Vec<String> = vec![
        "Blockdaemon".to_string(),
        "NowNodes".to_string(),
        "Custom".to_string(),
    ];

    if let Ok(config) = load_auth_config(None) {
        for provider in config.credentials.keys() {
            if !items.iter().any(|i| i == provider) {
                items.push(provider.clone());
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
        _ => Ok(items[selection].clone()),
    }
}

async fn store_credential(
    provider: &str,
    api_key: &str,
    config_path: Option<String>,
) -> Result<()> {
    let normalized_provider = normalize_provider_name(provider);

    store_credential_config(&normalized_provider, api_key, config_path).await
}

async fn store_credential_config(
    provider: &str,
    api_key: &str,
    config_path: Option<String>,
) -> Result<()> {
    let config_file = get_config_path(config_path)?;
    let mut config = load_auth_config(Some(&config_file)).unwrap_or_default();

    config
        .credentials
        .insert(provider.to_string(), api_key.to_string());

    save_auth_config(&config, &config_file).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&config_file)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&config_file, perms)?;
    }

    Ok(())
}

async fn remove_credential(provider: &str, config_path: Option<String>) -> Result<bool> {
    let normalized_provider = normalize_provider_name(provider);

    let config_file = get_config_path(config_path)?;
    if let Ok(mut config) = load_auth_config(Some(&config_file)) {
        if config
            .credentials
            .remove(&normalized_provider.clone())
            .is_some()
        {
            save_auth_config(&config, &config_file).await?;
            return Ok(true);
        }
    }

    Ok(false)
}

#[allow(dead_code)]
pub fn get_credential(provider: &str) -> Result<Option<String>> {
    let normalized_provider = normalize_provider_name(provider);

    get_credential_config(&normalized_provider)
}

#[allow(dead_code)]
fn get_credential_config(provider: &str) -> Result<Option<String>> {
    let config_file = get_config_path(None)?;
    let config = load_auth_config(Some(&config_file)).unwrap_or_default();
    Ok(config.credentials.get(provider).cloned())
}

fn load_auth_config(config_file: Option<&PathBuf>) -> Result<AuthConfig> {
    let default_path;
    let config_file = if let Some(p) = config_file {
        p
    } else {
        default_path = get_config_path(None)?;
        &default_path
    };

    if !config_file.exists() {
        return Ok(AuthConfig::default());
    }

    let content = fs::read_to_string(config_file)?;
    let config: AuthConfig =
        toml::from_str(&content).map_err(|e| anyhow!("Failed to parse config file: {e}"))?;

    Ok(config)
}

async fn save_auth_config(config: &AuthConfig, config_file: &PathBuf) -> Result<()> {
    if let Some(parent) = config_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let content =
        toml::to_string_pretty(config).map_err(|e| anyhow!("Failed to serialize config: {e}"))?;

    fs::write(config_file, content).map_err(|e| anyhow!("Failed to write config file: {e}"))?;

    Ok(())
}

fn get_config_path(override_path: Option<String>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(PathBuf::from(path));
    }

    let project_dirs = directories::ProjectDirs::from("dev", "grat", "grat")
        .ok_or_else(|| anyhow!("Could not determine config directory"))?;

    Ok(project_dirs.config_dir().join("auth.toml"))
}

fn normalize_provider_name(provider: &str) -> String {
    provider.to_lowercase().replace(' ', "-")
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

        let config = load_auth_config(Some(&config_path)).unwrap();
        assert!(config.credentials.get("nonexistent").is_none());
    }

    #[test]
    fn test_credential_round_trip_via_config_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("auth.toml");

        let provider = "test-provider";
        let api_key = "test-api-key-123";

        let mut config = AuthConfig::default();
        config
            .credentials
            .insert(provider.to_string(), api_key.to_string());

        let content = toml::to_string_pretty(&config).unwrap();
        fs::write(&config_path, content).unwrap();

        let loaded = load_auth_config(Some(&config_path)).unwrap();
        assert_eq!(loaded.credentials.get(provider), Some(&api_key.to_string()));
    }

    #[test]
    fn test_empty_key_is_rejected() {
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
        assert_eq!(
            normalize_provider_name("Custom Provider"),
            "custom-provider"
        );
        assert_eq!(normalize_provider_name("CUSTOM"), "custom");
    }

    #[tokio::test]
    async fn test_save_and_load_auth_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("auth.toml");

        let mut config = AuthConfig::default();
        config
            .credentials
            .insert("test".to_string(), "key123".to_string());

        save_auth_config(&config, &config_path).await.unwrap();
        assert!(config_path.exists());

        let loaded = load_auth_config(Some(&config_path)).unwrap();
        assert_eq!(loaded.credentials.get("test"), Some(&"key123".to_string()));
    }
}
