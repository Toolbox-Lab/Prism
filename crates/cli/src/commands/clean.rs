use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Arguments for the `grat clean` command.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(about = "Safely purge cached ledger entries and WASM files to reclaim disk space")]
pub struct CleanArgs {
    /// Target specific cache: WASM files only
    #[arg(long, help = "Only clear cached WASM files")]
    pub wasm: bool,

    /// Target specific cache: ledger entries only
    #[arg(long, help = "Only clear cached ledger entries")]
    pub ledger: bool,

    /// Clear all cache categories (default behavior if neither --wasm nor --ledger is specified)
    #[arg(long, help = "Clear all cached data")]
    pub all: bool,

    /// Optional custom cache directory to clean
    #[arg(
        long,
        value_name = "PATH",
        help = "Optional custom cache directory to clean (overrides OS default cache location)"
    )]
    pub cache_dir: Option<PathBuf>,
}

pub async fn run(args: CleanArgs, output_format: &str) -> anyhow::Result<()> {
    let bytes_cleared = clean_cache(&args)?;
    print_clean_result(bytes_cleared, output_format)?;
    Ok(())
}

/// Locates candidate cache directories.
///
/// Priority:
/// 1. Explicit `custom_path` if provided.
/// 2. OS-specific cache directory resolved via `directories::ProjectDirs` (`dev.grat.grat`).
/// 3. Legacy cache directory (`~/.grat/cache`).
///
/// When `custom_path` is None, returns any existing cache directories (or the primary OS
/// directory if none exist yet).
pub fn locate_cache_dirs(custom_path: Option<&Path>) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(path) = custom_path {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut dirs = Vec::new();

    // Primary cache directory using ProjectDirs ("dev", "grat", "grat")
    let primary_dir = directories::ProjectDirs::from("dev", "grat", "grat")
        .map(|proj| proj.cache_dir().to_path_buf());

    // Legacy fallback directory ~/.grat/cache
    let legacy_dir = directories::BaseDirs::new()
        .map(|base| base.home_dir().join(".grat").join("cache"))
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".grat").join("cache"))
        });

    if let Some(ref p) = primary_dir {
        if p.exists() {
            dirs.push(p.clone());
        }
    }

    if let Some(ref l) = legacy_dir {
        if l.exists() && !dirs.iter().any(|d| d == l) {
            dirs.push(l.clone());
        }
    }

    // If neither exists yet, default to the primary OS cache dir (or legacy dir if primary unavailable)
    if dirs.is_empty() {
        if let Some(p) = primary_dir {
            dirs.push(p);
        } else if let Some(l) = legacy_dir {
            dirs.push(l);
        } else {
            anyhow::bail!("Could not determine cache directory");
        }
    }

    Ok(dirs)
}

/// Helper to locate the primary cache directory (useful for single-dir lookups / diagnostics).
pub fn locate_cache_dir(custom_path: Option<&Path>) -> anyhow::Result<PathBuf> {
    let dirs = locate_cache_dirs(custom_path)?;
    dirs.into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No cache directory found"))
}

/// Purges cache files based on `CleanArgs` and recreates the clean directory structure.
/// Returns the total number of bytes cleared across all targeted locations.
pub fn clean_cache(args: &CleanArgs) -> anyhow::Result<u64> {
    let cache_dirs = locate_cache_dirs(args.cache_dir.as_deref())?;

    let is_wasm = args.wasm;
    let is_ledger = args.ledger;
    let clean_all = args.all || (!is_wasm && !is_ledger);

    let mut total_bytes = 0u64;

    for cache_dir in cache_dirs {
        if !cache_dir.exists() {
            recreate_cache_structure(&cache_dir, is_wasm && !clean_all, is_ledger && !clean_all)?;
            continue;
        }

        if clean_all {
            let bytes = directory_size_bytes(&cache_dir)?;
            total_bytes = total_bytes.saturating_add(bytes);
            clear_directory_contents(&cache_dir)?;
            recreate_cache_structure(&cache_dir, false, false)?;
        } else {
            if is_wasm {
                let wasm_dir = cache_dir.join("wasm");
                if wasm_dir.exists() {
                    let bytes = directory_size_bytes(&wasm_dir)?;
                    total_bytes = total_bytes.saturating_add(bytes);
                    clear_directory_contents(&wasm_dir)?;
                }
                fs::create_dir_all(&wasm_dir).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to recreate wasm directory {}: {}",
                        wasm_dir.display(),
                        e
                    )
                })?;
            }

            if is_ledger {
                let ledger_dir = cache_dir.join("ledger");
                if ledger_dir.exists() {
                    let bytes = directory_size_bytes(&ledger_dir)?;
                    total_bytes = total_bytes.saturating_add(bytes);
                    clear_directory_contents(&ledger_dir)?;
                }
                fs::create_dir_all(&ledger_dir).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to recreate ledger directory {}: {}",
                        ledger_dir.display(),
                        e
                    )
                })?;
            }

            // Ensure base cache dir exists
            fs::create_dir_all(&cache_dir).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to ensure cache directory {}: {}",
                    cache_dir.display(),
                    e
                )
            })?;
        }
    }

    Ok(total_bytes)
}

/// Recreates the standard clean directory structure (`wasm` and `ledger` subdirectories).
pub fn recreate_cache_structure(
    base_dir: &Path,
    wasm_only: bool,
    ledger_only: bool,
) -> anyhow::Result<()> {
    fs::create_dir_all(base_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to recreate cache directory {}: {}",
            base_dir.display(),
            e
        )
    })?;

    if wasm_only {
        fs::create_dir_all(base_dir.join("wasm")).map_err(|e| {
            anyhow::anyhow!("Failed to recreate wasm cache directory: {}", e)
        })?;
    } else if ledger_only {
        fs::create_dir_all(base_dir.join("ledger")).map_err(|e| {
            anyhow::anyhow!("Failed to recreate ledger cache directory: {}", e)
        })?;
    } else {
        fs::create_dir_all(base_dir.join("wasm")).map_err(|e| {
            anyhow::anyhow!("Failed to recreate wasm cache directory: {}", e)
        })?;
        fs::create_dir_all(base_dir.join("ledger")).map_err(|e| {
            anyhow::anyhow!("Failed to recreate ledger cache directory: {}", e)
        })?;
    }

    Ok(())
}

/// Recursively calculates total size in bytes of all regular files in `path`.
pub fn directory_size_bytes(path: &Path) -> anyhow::Result<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let mut total = 0u64;

    for entry in WalkDir::new(path) {
        match entry {
            Ok(entry) => {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total = total.saturating_add(metadata.len());
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Error reading directory entry: {e}");
            }
        }
    }

    Ok(total)
}

/// Removes all files and subdirectories directly inside `path`, keeping `path` itself.
pub fn clear_directory_contents(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }

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

/// Formats byte count to megabytes with 1 decimal place (or whole number if exact).
pub fn format_mb(bytes: u64) -> String {
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

/// Builds the structured JSON response payload.
pub fn build_clean_json_payload(total_bytes: u64) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "bytes_cleared": total_bytes,
        "mb_cleared": format_mb(total_bytes),
    })
}

/// Formats the human-readable output message.
pub fn format_clean_human_message(total_bytes: u64) -> String {
    format!(
        "Successfully cleared {}MB of cache data",
        format_mb(total_bytes)
    )
}

pub fn print_clean_result(total_bytes: u64, output_format: &str) -> anyhow::Result<()> {
    if matches!(
        crate::output::OutputFormat::parse(output_format),
        crate::output::OutputFormat::Json
    ) {
        let payload = build_clean_json_payload(total_bytes);
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", format_clean_human_message(total_bytes));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_format_mb_zero() {
        assert_eq!(format_mb(0), "0");
    }

    #[test]
    fn test_format_mb_fractional_and_whole() {
        // Exactly 1 MB
        assert_eq!(format_mb(1024 * 1024), "1");
        // Exactly 10 MB
        assert_eq!(format_mb(10 * 1024 * 1024), "10");
        // 1.5 MB
        assert_eq!(format_mb((1.5 * 1024.0 * 1024.0) as u64), "1.5");
        // 0.5 MB
        assert_eq!(format_mb(512 * 1024), "0.5");
        // Small byte count (< 0.05 MB rounds to 0)
        assert_eq!(format_mb(1024), "0");
    }

    #[test]
    fn test_build_clean_json_payload() {
        let payload = build_clean_json_payload(1024 * 1024);
        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["bytes_cleared"], 1048576);
        assert_eq!(payload["mb_cleared"], "1");
    }

    #[test]
    fn test_format_clean_human_message() {
        let msg = format_clean_human_message(1024 * 1024);
        assert_eq!(msg, "Successfully cleared 1MB of cache data");
    }

    #[test]
    fn test_directory_size_bytes_nonexistent() {
        let path = PathBuf::from("nonexistent_cache_dir_12345");
        assert_eq!(directory_size_bytes(&path).unwrap(), 0);
    }

    #[test]
    fn test_directory_size_bytes_nested() {
        let tmp = tempdir().unwrap();
        let base = tmp.path();

        let file1 = base.join("file1.bin");
        let sub = base.join("sub");
        fs::create_dir(&sub).unwrap();
        let file2 = sub.join("file2.bin");

        fs::write(&file1, vec![0u8; 1000]).unwrap();
        fs::write(&file2, vec![0u8; 2500]).unwrap();

        let size = directory_size_bytes(base).unwrap();
        assert_eq!(size, 3500);
    }

    #[test]
    fn test_locate_cache_dir_custom() {
        let custom = PathBuf::from("/custom/cache/dir");
        let dirs = locate_cache_dirs(Some(&custom)).unwrap();
        assert_eq!(dirs, vec![custom.clone()]);

        let dir = locate_cache_dir(Some(&custom)).unwrap();
        assert_eq!(dir, custom);
    }

    #[test]
    fn test_clean_cache_all() {
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().join("cache");
        fs::create_dir_all(cache_dir.join("wasm")).unwrap();
        fs::create_dir_all(cache_dir.join("ledger")).unwrap();
        fs::create_dir_all(cache_dir.join("other")).unwrap();

        fs::write(cache_dir.join("wasm").join("test.wasm"), vec![1u8; 4096]).unwrap();
        fs::write(cache_dir.join("ledger").join("test.entry"), vec![2u8; 8192]).unwrap();
        fs::write(cache_dir.join("other").join("data.bin"), vec![3u8; 1024]).unwrap();

        let args = CleanArgs {
            cache_dir: Some(cache_dir.clone()),
            ..Default::default()
        };

        let cleared = clean_cache(&args).unwrap();
        assert_eq!(cleared, 4096 + 8192 + 1024);

        // Verify clean structure is recreated
        assert!(cache_dir.exists());
        assert!(cache_dir.join("wasm").exists());
        assert!(cache_dir.join("ledger").exists());
        assert!(!cache_dir.join("other").exists());
        assert_eq!(directory_size_bytes(&cache_dir).unwrap(), 0);
    }

    #[test]
    fn test_clean_cache_wasm_only() {
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().join("cache");
        fs::create_dir_all(cache_dir.join("wasm")).unwrap();
        fs::create_dir_all(cache_dir.join("ledger")).unwrap();

        fs::write(cache_dir.join("wasm").join("test.wasm"), vec![1u8; 4096]).unwrap();
        fs::write(cache_dir.join("ledger").join("test.entry"), vec![2u8; 8192]).unwrap();

        let args = CleanArgs {
            wasm: true,
            cache_dir: Some(cache_dir.clone()),
            ..Default::default()
        };

        let cleared = clean_cache(&args).unwrap();
        assert_eq!(cleared, 4096);

        // wasm should be cleared and recreated
        assert!(cache_dir.join("wasm").exists());
        assert_eq!(directory_size_bytes(&cache_dir.join("wasm")).unwrap(), 0);

        // ledger should remain untouched
        assert!(cache_dir.join("ledger").join("test.entry").exists());
        assert_eq!(directory_size_bytes(&cache_dir.join("ledger")).unwrap(), 8192);
    }

    #[test]
    fn test_clean_cache_ledger_only() {
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().join("cache");
        fs::create_dir_all(cache_dir.join("wasm")).unwrap();
        fs::create_dir_all(cache_dir.join("ledger")).unwrap();

        fs::write(cache_dir.join("wasm").join("test.wasm"), vec![1u8; 4096]).unwrap();
        fs::write(cache_dir.join("ledger").join("test.entry"), vec![2u8; 8192]).unwrap();

        let args = CleanArgs {
            ledger: true,
            cache_dir: Some(cache_dir.clone()),
            ..Default::default()
        };

        let cleared = clean_cache(&args).unwrap();
        assert_eq!(cleared, 8192);

        // ledger should be cleared and recreated
        assert!(cache_dir.join("ledger").exists());
        assert_eq!(directory_size_bytes(&cache_dir.join("ledger")).unwrap(), 0);

        // wasm should remain untouched
        assert!(cache_dir.join("wasm").join("test.wasm").exists());
        assert_eq!(directory_size_bytes(&cache_dir.join("wasm")).unwrap(), 4096);
    }

    #[test]
    fn test_clean_cache_both_wasm_and_ledger() {
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().join("cache");
        fs::create_dir_all(cache_dir.join("wasm")).unwrap();
        fs::create_dir_all(cache_dir.join("ledger")).unwrap();
        fs::create_dir_all(cache_dir.join("keep")).unwrap();

        fs::write(cache_dir.join("wasm").join("test.wasm"), vec![1u8; 2000]).unwrap();
        fs::write(cache_dir.join("ledger").join("test.entry"), vec![2u8; 3000]).unwrap();
        fs::write(cache_dir.join("keep").join("keep.bin"), vec![3u8; 5000]).unwrap();

        let args = CleanArgs {
            wasm: true,
            ledger: true,
            cache_dir: Some(cache_dir.clone()),
            ..Default::default()
        };

        let cleared = clean_cache(&args).unwrap();
        assert_eq!(cleared, 5000);

        // wasm and ledger are cleared and recreated
        assert!(cache_dir.join("wasm").exists());
        assert_eq!(directory_size_bytes(&cache_dir.join("wasm")).unwrap(), 0);
        assert!(cache_dir.join("ledger").exists());
        assert_eq!(directory_size_bytes(&cache_dir.join("ledger")).unwrap(), 0);

        // keep directory is preserved
        assert!(cache_dir.join("keep").join("keep.bin").exists());
        assert_eq!(directory_size_bytes(&cache_dir.join("keep")).unwrap(), 5000);
    }

    #[test]
    fn test_clean_nonexistent_directory() {
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().join("does_not_exist_yet");

        let args = CleanArgs {
            cache_dir: Some(cache_dir.clone()),
            ..Default::default()
        };

        let cleared = clean_cache(&args).unwrap();
        assert_eq!(cleared, 0);

        // Directory structure should be created
        assert!(cache_dir.exists());
        assert!(cache_dir.join("wasm").exists());
        assert!(cache_dir.join("ledger").exists());
    }
}
