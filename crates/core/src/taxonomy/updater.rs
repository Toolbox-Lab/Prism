use crate::error::GratResult;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const TAXONOMY_URL: &str = "https://raw.githubusercontent.com/grat-soroban/grat/main/taxonomy/enhanced_error_taxonomy.toml";

#[derive(Serialize, Deserialize)]
struct UpdateCache {
    last_check: SystemTime,
    version: Option<String>,
}

pub fn cache_file_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "grat")
        .map(|proj_dirs| proj_dirs.cache_dir().join("taxonomy_update.json"))
}

pub fn db_file_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "grat")
        .map(|proj_dirs| proj_dirs.data_dir().join("taxonomy").join("database.toml"))
}

pub async fn check_and_update(offline: bool) -> GratResult<()> {
    if offline {
        tracing::debug!("Offline mode enabled, skipping taxonomy update check");
        return Ok(());
    }

    let Some(cache_path) = cache_file_path() else {
        return Ok(());
    };

    if let Ok(content) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<UpdateCache>(&content) {
            if let Ok(elapsed) = cache.last_check.elapsed() {
                if elapsed.as_secs() < 24 * 60 * 60 {
                    tracing::debug!("Taxonomy update checked recently, skipping");
                    return Ok(());
                }
            }
        }
    }

    let client = match reqwest::Client::builder()
        .user_agent("grat-taxonomy-updater")
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to build reqwest client for taxonomy update: {e}");
            return Ok(());
        }
    };

    tracing::debug!("Checking for latest taxonomy version");

    let response = match client.get(TAXONOMY_URL).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to download taxonomy database: {e}");
            return Ok(());
        }
    };

    if !response.status().is_success() {
        tracing::warn!(
            "Failed to download taxonomy database: HTTP {}",
            response.status()
        );
        return Ok(());
    }

    let content = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Failed to read taxonomy database response: {e}");
            return Ok(());
        }
    };

    let Some(db_path) = db_file_path() else {
        return Ok(());
    };

    if let Some(parent) = db_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Atomically replace the database file
    let temp_path = db_path.with_extension("tmp");
    if fs::write(&temp_path, &content).is_ok() {
        if fs::rename(&temp_path, &db_path).is_err() {
            tracing::warn!("Failed to replace taxonomy database atomically");
            let _ = fs::remove_file(temp_path);
        } else {
            tracing::info!("Taxonomy database updated successfully");
        }
    }

    if let Some(parent) = cache_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let new_cache = UpdateCache {
        last_check: SystemTime::now(),
        version: None, // We don't have versioning for raw file yet, just timestamp cache
    };

    if let Ok(serialized) = serde_json::to_string(&new_cache) {
        let _ = fs::write(&cache_path, serialized);
    }

    Ok(())
}
