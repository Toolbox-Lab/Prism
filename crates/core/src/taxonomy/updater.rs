use crate::error::GratResult;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const TAXONOMY_URL: &str = "https://raw.githubusercontent.com/Killerjunior/Grat/codex/528-db-taxonomy-management/taxonomy/database.toml";

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

    if let Err(error) = crate::taxonomy::loader::TaxonomyParser::parse(&content) {
        tracing::warn!("Downloaded taxonomy database is invalid: {error}");
        return Ok(());
    }

    let Some(db_path) = db_file_path() else {
        return Ok(());
    };

    let Some(parent) = db_path.parent() else {
        return Ok(());
    };
    let _ = fs::create_dir_all(parent);

    // Stage beside the database so concurrent updates cannot overwrite each
    // other's temporary file and the final rename stays on the same filesystem.
    match create_staging_file(parent, &db_path) {
        Ok((temp_path, mut temp_file)) => {
            use std::io::Write;
            let staged = temp_file.write_all(content.as_bytes());
            drop(temp_file);
            if staged.is_err() || fs::rename(&temp_path, &db_path).is_err() {
                tracing::warn!("Failed to atomically replace taxonomy database");
                let _ = fs::remove_file(temp_path);
            } else {
                tracing::info!("Taxonomy database updated successfully");
            }
        }
        Err(error) => tracing::warn!("Failed to stage taxonomy database update: {error}"),
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

fn create_staging_file(
    parent: &std::path::Path,
    database_path: &std::path::Path,
) -> std::io::Result<(PathBuf, fs::File)> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

    for _ in 0..100 {
        let id = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
        let staging_path = parent.join(format!(
            "{}.{}.{}.tmp",
            database_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("taxonomy"),
            std::process::id(),
            id
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging_path)
        {
            Ok(file) => return Ok((staging_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique taxonomy staging file",
    ))
}
