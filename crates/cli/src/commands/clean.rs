//! `prism clean` — Clear local Prism cache data.

use clap::Args;
use std::fs;
use std::path::Path;

#[derive(Args)]
pub struct CleanArgs;

pub async fn run(_args: CleanArgs) -> anyhow::Result<()> {
    let cache_dir = prism_cache_dir()?;

    if !cache_dir.exists() {
        println!("Successfully cleared 0MB of cache data");
        return Ok(());
    }

    let total_bytes = directory_size_bytes(&cache_dir)?;

    clear_directory_contents(&cache_dir)?;

    // Keep cache directory present after cleanup.
    fs::create_dir_all(&cache_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to recreate cache directory {}: {}",
            cache_dir.display(),
            e
        )
    })?;

    println!(
        "Successfully cleared {}MB of cache data",
        format_mb(total_bytes)
    );

    Ok(())
}

fn prism_cache_dir() -> anyhow::Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory for cache path"))?;

    Ok(std::path::PathBuf::from(home).join(".prism").join("cache"))
}

fn directory_size_bytes(path: &Path) -> anyhow::Result<u64> {
    let mut total = 0u64;

    for entry in fs::read_dir(path)
        .map_err(|e| anyhow::anyhow!("Failed to read cache directory {}: {}", path.display(), e))?
    {
        let entry = entry.map_err(|e| {
            anyhow::anyhow!(
                "Failed to read an entry in cache directory {}: {}",
                path.display(),
                e
            )
        })?;

        let entry_path = entry.path();
        let metadata = entry.metadata().map_err(|e| {
            anyhow::anyhow!(
                "Failed to read metadata for cache entry {}: {}",
                entry_path.display(),
                e
            )
        })?;

        if metadata.is_dir() {
            total = total.saturating_add(directory_size_bytes(&entry_path)?);
        } else {
            total = total.saturating_add(metadata.len());
        }
    }

    Ok(total)
}

fn clear_directory_contents(path: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(path)
        .map_err(|e| anyhow::anyhow!("Failed to read cache directory {}: {}", path.display(), e))?
    {
        let entry = entry.map_err(|e| {
            anyhow::anyhow!(
                "Failed to read an entry in cache directory {}: {}",
                path.display(),
                e
            )
        })?;

        let entry_path = entry.path();
        let metadata = entry.metadata().map_err(|e| {
            anyhow::anyhow!(
                "Failed to read metadata for cache entry {}: {}",
                entry_path.display(),
                e
            )
        })?;

        if metadata.is_dir() {
            fs::remove_dir_all(&entry_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to remove cache directory {}: {}",
                    entry_path.display(),
                    e
                )
            })?;
        } else {
            fs::remove_file(&entry_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to remove cache file {}: {}",
                    entry_path.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

fn format_mb(bytes: u64) -> String {
    if bytes == 0 {
        return "0".to_string();
    }

    let mb = bytes as f64 / (1024.0 * 1024.0);
    let rounded = (mb * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as u64)
    } else {
        format!("{rounded:.1}")
    }
}
