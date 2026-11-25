use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use dialoguer::Select;
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

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
    #[serde(default)]
    credentials: HashMap<String, String>,
}

pub async fn run(args: AuthArgs, output_format: &str) -> Result<()> {
    match args.command {
        AuthCommands::Login {
            provider,
            config_path,
        } => login(provider, config_path, output_format),
        AuthCommands::Logout {
            provider,
            config_path,
        } => logout(provider, config_path, output_format),
    }
}

fn login(
    provider_param: Option<String>,
    config_path: Option<String>,
    output_format: &str,
) -> Result<()> {
    let palette = crate::output::theme::ColorPalette::default();
    let provider = match provider_param {
        Some(provider) => provider,
        None => select_provider_interactive()?,
    };
    let provider = normalize_provider_name(&provider)?;
    let prompt = format!(
        "Enter API key or token for {}: ",
        palette.success_text(&provider)
    );
    let api_key = prompt_password(&prompt)?;
    if api_key.trim().is_empty() {
        return Err(anyhow!("API key cannot be empty"));
    }

    let config_file = get_config_path(config_path)?;
    let mut config = load_auth_config(&config_file)?;
    config.credentials.insert(provider.clone(), api_key);
    save_auth_config(&config, &config_file)?;

    if is_json_output(output_format) {
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
    Ok(())
}

fn logout(
    provider_param: Option<String>,
    config_path: Option<String>,
    output_format: &str,
) -> Result<()> {
    let palette = crate::output::theme::ColorPalette::default();
    let config_file = get_config_path(config_path)?;
    let provider = match provider_param {
        Some(provider) => provider,
        None => select_provider_for_logout(&config_file)?,
    };
    let provider = normalize_provider_name(&provider)?;
    let mut config = load_auth_config(&config_file)?;
    let removed = config.credentials.remove(&provider).is_some();
    if removed {
        save_auth_config(&config, &config_file)?;
    }

    if is_json_output(output_format) {
        let payload = serde_json::json!({
            "status": "ok",
            "action": "logout",
            "provider": provider,
            "removed": removed,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if removed {
        println!(
            "✓ Credentials for {} removed.",
            palette.success_text(&provider)
        );
    } else {
        println!(
            "No credentials found for {}.",
            palette.warning_text(&provider)
        );
    }
    Ok(())
}

fn is_json_output(output_format: &str) -> bool {
    matches!(
        crate::output::OutputFormat::parse(output_format),
        crate::output::OutputFormat::Json
    )
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
        2 => prompt_custom_provider(),
        _ => Err(anyhow!("Invalid provider selection")),
    }
}

fn select_provider_for_logout(config_file: &Path) -> Result<String> {
    let mut items: Vec<String> = vec![
        "Blockdaemon".to_string(),
        "NowNodes".to_string(),
        "Custom".to_string(),
    ];
    let config = load_auth_config(config_file)?;
    for provider in config.credentials.keys() {
        if !items.iter().any(|item| item == provider) {
            items.push(provider.clone());
        }
    }

    let selection = Select::new()
        .with_prompt("Select provider to logout:")
        .items(&items)
        .interact()?;

    match selection {
        0 => Ok("Blockdaemon".to_string()),
        1 => Ok("NowNodes".to_string()),
        2 => prompt_custom_provider(),
        index => items
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow!("Invalid provider selection")),
    }
}

fn prompt_custom_provider() -> Result<String> {
    use dialoguer::Input;

    Ok(Input::new()
        .with_prompt("Enter custom provider name:")
        .interact_text()?)
}

#[allow(dead_code)]
pub fn get_credential(provider: &str) -> Result<Option<String>> {
    let provider = normalize_provider_name(provider)?;
    let config = load_auth_config(&get_config_path(None)?)?;
    Ok(config.credentials.get(&provider).cloned())
}

fn load_auth_config(config_file: &Path) -> Result<AuthConfig> {
    let content = match fs::read(config_file) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AuthConfig::default());
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!("Failed to read auth config at {}", config_file.display())
            });
        }
    };
    serde_json::from_slice(&content)
        .with_context(|| format!("Failed to parse auth config at {}", config_file.display()))
}

fn save_auth_config(config: &AuthConfig, config_file: &Path) -> Result<()> {
    if let Some(parent) = config_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
        #[cfg(unix)]
        if let Some(home_dir) = dirs::home_dir() {
            let grat_dir = home_dir.join(".grat");
            if parent == grat_dir {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).with_context(
                    || format!("Failed to restrict permissions on {}", parent.display()),
                )?;
            }
        }
    }

    let content = serde_json::to_vec_pretty(config).context("Failed to serialize auth config")?;
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        if config_file.exists() {
            fs::set_permissions(config_file, fs::Permissions::from_mode(0o600)).with_context(
                || {
                    format!(
                        "Failed to restrict permissions on {}",
                        config_file.display()
                    )
                },
            )?;
        }
    }

    let mut file = options
        .open(config_file)
        .with_context(|| format!("Failed to open auth config at {}", config_file.display()))?;
    file.write_all(&content)
        .with_context(|| format!("Failed to write auth config at {}", config_file.display()))?;
    file.flush()
        .with_context(|| format!("Failed to flush auth config at {}", config_file.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(config_file, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "Failed to restrict permissions on {}",
                config_file.display()
            )
        })?;
    }

    Ok(())
}

fn get_config_path(override_path: Option<String>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(PathBuf::from(path));
    }

    let home_dir = dirs::home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?;
    Ok(home_dir.join(".grat").join("auth.json"))
}

fn normalize_provider_name(provider: &str) -> Result<String> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err(anyhow!("Provider name cannot be empty"));
    }
    Ok(provider.to_lowercase().replace(' ', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn auth_config_round_trips_as_json() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("auth.json");
        let config = AuthConfig {
            credentials: HashMap::from([("test-provider".to_string(), "secret".to_string())]),
        };

        save_auth_config(&config, &config_path).unwrap();
        let loaded = load_auth_config(&config_path).unwrap();

        assert_eq!(
            loaded.credentials.get("test-provider"),
            Some(&"secret".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn auth_config_is_written_with_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("private").join("auth.json");
        save_auth_config(&AuthConfig::default(), &config_path).unwrap();

        assert_eq!(
            fs::metadata(config_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn default_auth_config_is_in_grat_home_directory() {
        let path = get_config_path(None).unwrap();
        assert_eq!(
            path,
            dirs::home_dir().unwrap().join(".grat").join("auth.json")
        );
    }

    #[test]
    fn normalize_provider_name_rejects_empty_names() {
        assert!(normalize_provider_name("  ").is_err());
        assert_eq!(
            normalize_provider_name("Custom Provider").unwrap(),
            "custom-provider"
        );
    }
}
