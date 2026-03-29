//! Background version checking for the Prism CLI.
//!
//! Silently checks GitHub for new releases and caches the result for 24 hours
//! to avoid rate limits and keep the CLI fast.

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
struct VersionCache {
    last_check: DateTime<Utc>,
    latest_version: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

/// Determines the path for the version check cache file.
fn cache_file_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("prism").join("version_check.json"))
}

/// Asynchronously checks if a newer version of Prism is available.
/// Fails silently on any network or parsing error.
/// Returns the new version (without 'v' prefix) if an update is available.
pub async fn check_for_updates() -> Option<String> {
    check_for_updates_internal().await.unwrap_or(None)
}

/// Internal execution of the update check, returning a `Result` for easy error propagation.
async fn check_for_updates_internal() -> anyhow::Result<Option<String>> {
    let cache_path =
        cache_file_path().ok_or_else(|| anyhow::anyhow!("No cache directory found"))?;

    // 1. Read cache and return early if check happened < 24 hours ago
    if let Ok(content) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<VersionCache>(&content) {
            let now = Utc::now();
            if now.signed_duration_since(cache.last_check).num_hours() < 24 {
                return Ok(compare_versions(&cache.latest_version));
            }
        }
    }

    // 2. Fetch the latest release from GitHub
    let client = Client::builder()
        .user_agent("prism-cli-updater")
        .timeout(Duration::from_secs(2))
        .build()?;

    let response = client
        .get("https://api.github.com/repos/prism/prism/releases/latest")
        .send()
        .await?
        .error_for_status()?;

    let release: GitHubRelease = response.json().await?;

    // 3. Process the version tag (strip 'v' prefix if present)
    let latest_version = release.tag_name.trim_start_matches('v').to_string();

    // 4. Update the cache file
    if let Some(parent) = cache_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let new_cache = VersionCache {
        last_check: Utc::now(),
        latest_version: latest_version.clone(),
    };

    if let Ok(serialized) = serde_json::to_string(&new_cache) {
        let _ = fs::write(&cache_path, serialized);
    }

    // 5. Compare with the current binary version
    Ok(compare_versions(&latest_version))
}

/// Compares the given latest version string with the current binary version.
fn compare_versions(latest: &str) -> Option<String> {
    let current_version = env!("CARGO_PKG_VERSION");

    let current_semver = semver::Version::parse(current_version).ok()?;
    let latest_semver = semver::Version::parse(latest).ok()?;

    if latest_semver > current_semver {
        Some(latest.to_string())
    } else {
        None
    }
}
