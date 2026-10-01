use crate::error::{GratError, GratResult};

use std::cmp::Ordering;

use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub enum CacheCategory {
    WasmBlob,

    ContractSpec,

    LedgerEntry,

    TransactionResult,
}

impl CacheCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::WasmBlob => "wasm",
            Self::ContractSpec => "spec",
            Self::LedgerEntry => "ledger",
            Self::TransactionResult => "tx",
        }
    }
}

pub struct CacheStore {
    cache_dir: PathBuf,
    max_size: u64,
}

impl CacheStore {
    pub fn new(cache_dir: PathBuf, max_size_mb: u64) -> GratResult<Self> {
        Self::with_max_size_bytes(cache_dir, max_size_mb * 1024 * 1024)
    }

    pub fn with_max_size_bytes(cache_dir: PathBuf, max_size_bytes: u64) -> GratResult<Self> {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| GratError::CacheError(format!("Failed to create cache dir: {e}")))?;

        Ok(Self {
            cache_dir,
            max_size: max_size_bytes,
        })
    }

    pub fn default_location() -> GratResult<Self> {
        let project_dirs =
            directories::ProjectDirs::from("dev", "grat", "grat").ok_or_else(|| {
                GratError::CacheError("Could not determine cache directory".to_string())
            })?;

        Self::new(project_dirs.cache_dir().to_path_buf(), 512)
    }

    pub fn put(
        &self,
        category: CacheCategory,
        key: &str,
        value: &[u8],
        ledger_sequence: Option<u32>,
    ) -> GratResult<()> {
        let new_size = value.len() as u64;
        if new_size > self.max_size {
            return Err(GratError::CacheError(format!(
                "Cache entry exceeds configured cache size limit of {} bytes",
                self.max_size
            )));
        }

        // Ensure we can fit the new entry by evicting least-recently-used files.
        let current_size = self.total_cache_size()?;
        if current_size.saturating_add(new_size) > self.max_size {
            self.evict_lru_to_fit(new_size)?;
        }

        let path = self.entry_path(category, key, ledger_sequence);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GratError::CacheError(format!("Failed to create dir: {e}")))?;
        }

        std::fs::write(&path, value)
            .map_err(|e| GratError::CacheError(format!("Failed to write cache entry: {e}")))?;
        Ok(())
    }

    pub fn get(
        &self,
        category: CacheCategory,
        key: &str,
        ledger_sequence: Option<u32>,
    ) -> GratResult<Option<Vec<u8>>> {
        let path = self.entry_path(category, key, ledger_sequence);
        if path.exists() {
            // Explicitly update access metadata to ensure LRU eviction works even if atime is disabled.
            if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) {
                let now = SystemTime::now();
                let _ = file.set_times(
                    std::fs::FileTimes::new()
                        .set_accessed(now)
                        .set_modified(now),
                );
            }
            let data = std::fs::read(&path)
                .map_err(|e| GratError::CacheError(format!("Failed to read cache entry: {e}")))?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    pub fn contains(&self, category: CacheCategory, key: &str, ledger_sequence: Option<u32>) -> bool {
        self.entry_path(category, key, ledger_sequence).exists()
    }

    pub fn remove(
        &self,
        category: CacheCategory,
        key: &str,
        ledger_sequence: Option<u32>,
    ) -> GratResult<()> {
        let path = self.entry_path(category, key, ledger_sequence);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| GratError::CacheError(format!("Failed to remove cache entry: {e}")))?;
        }
        Ok(())
    }

    pub fn prune_older_than(&self, category: CacheCategory, ledger_sequence: u32) -> GratResult<()> {
        let base_dir = self.cache_dir.join(category.as_str());
        if !base_dir.exists() {
            return Ok(());
        }

        let mut stale_paths = Vec::new();
        for entry in walk_dir_files(&base_dir) {
            let path = entry.path();
            let Some(parent) = path.parent() else {
                continue;
            };
            let Some(seq_text) = parent.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let Ok(seq) = seq_text.parse::<u32>() else {
                continue;
            };
            if seq < ledger_sequence {
                stale_paths.push(path);
            }
        }

        for path in stale_paths {
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| GratError::CacheError(format!("Failed to prune cache entry: {e}")))?;
            }
        }

        Ok(())
    }

    pub fn clear(&self) -> GratResult<()> {
        if self.cache_dir.exists() {
            std::fs::remove_dir_all(&self.cache_dir)
                .map_err(|e| GratError::CacheError(format!("Failed to clear cache: {e}")))?;
            std::fs::create_dir_all(&self.cache_dir)
                .map_err(|e| GratError::CacheError(format!("Failed to recreate cache dir: {e}")))?;
        }
        Ok(())
    }

    fn entry_path(
        &self,
        category: CacheCategory,
        key: &str,
        ledger_sequence: Option<u32>,
    ) -> PathBuf {
        let mut path = self.cache_dir.join(category.as_str());
        if let Some(sequence) = ledger_sequence {
            path = path.join(sequence.to_string());
        }
        path.join(key)
    }

    fn total_cache_size(&self) -> GratResult<u64> {
        let mut total: u64 = 0;

        if !self.cache_dir.exists() {
            return Ok(0);
        }

        for entry in walk_dir_files(&self.cache_dir) {
            let size = entry
                .metadata()
                .map_err(|e| {
                    GratError::CacheError(format!("Failed to read cache file metadata: {e}"))
                })?
                .len();
            total = total.saturating_add(size);
        }

        Ok(total)
    }

    fn evict_lru_to_fit(&self, required_new_entry_size: u64) -> GratResult<()> {
        // Keep evicting oldest files until we can fit the required new entry.
        loop {
            let current_size = self.total_cache_size()?;
            if current_size.saturating_add(required_new_entry_size) <= self.max_size {
                return Ok(());
            }

            let mut files = Vec::new();
            if self.cache_dir.exists() {
                for entry in walk_dir_files(&self.cache_dir) {
                    let meta = entry.metadata().map_err(|e| {
                        GratError::CacheError(format!("Failed to read cache file metadata: {e}"))
                    })?;

                    let accessed = meta.accessed().unwrap_or(SystemTime::UNIX_EPOCH);
                    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    let last_used = std::cmp::max(accessed, modified);

                    files.push((last_used, entry));
                }
            }

            if files.is_empty() {
                // No files to evict, but we still don't fit.
                return Err(GratError::CacheError(format!(
                    "Cache max_size={} too small or eviction impossible",
                    self.max_size
                )));
            }

            // Oldest first => delete until we create headroom.
            files.sort_by(|a, b| {
                let ord = a.0.cmp(&b.0);
                if ord == Ordering::Equal {
                    // Stable tie-breaker: delete longer ago deterministically.
                    a.1.path().cmp(&b.1.path())
                } else {
                    ord
                }
            });

            let (oldest_ts, oldest_file) = files.into_iter().next().expect("checked empty");

            let path = oldest_file.path();
            // Best-effort delete.
            std::fs::remove_file(&path).map_err(|e| {
                GratError::CacheError(format!(
                    "Failed to evict cache file {}: {e}",
                    path.display()
                ))
            })?;

            // If deletion succeeded, loop will re-check size.
            let _ = oldest_ts;
        }
    }
}

fn walk_dir_files(dir: &Path) -> Vec<std::fs::DirEntry> {
    fn visit_dir(dir: &Path, out: &mut Vec<std::fs::DirEntry>) {
        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for e in read_dir.flatten() {
                let p = e.path();
                if p.is_dir() {
                    visit_dir(&p, out);
                } else {
                    out.push(e);
                }
            }
        }
    }

    let mut files = Vec::new();
    visit_dir(dir, &mut files);
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_roundtrip() {
        let dir = std::env::temp_dir().join("grat_test_cache");
        let store = CacheStore::new(dir.clone(), 10).unwrap();

        store
            .put(CacheCategory::WasmBlob, "test_key", b"hello", None)
            .unwrap();
        let result = store.get(CacheCategory::WasmBlob, "test_key", None).unwrap();
        assert_eq!(result, Some(b"hello".to_vec()));

        store.clear().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let dir = std::env::temp_dir().join("grat_test_lru");
        let _ = std::fs::remove_dir_all(&dir);
        let store = CacheStore::with_max_size_bytes(dir.clone(), 10).unwrap();

        store.put(CacheCategory::WasmBlob, "key1", b"1234", None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.put(CacheCategory::WasmBlob, "key2", b"5678", None).unwrap();

        assert!(store.contains(CacheCategory::WasmBlob, "key1", None));
        assert!(store.contains(CacheCategory::WasmBlob, "key2", None));

        store.get(CacheCategory::WasmBlob, "key1", None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        store.put(CacheCategory::WasmBlob, "key3", b"9012", None).unwrap();

        assert!(store.contains(CacheCategory::WasmBlob, "key1", None));
        assert!(!store.contains(CacheCategory::WasmBlob, "key2", None));
        assert!(store.contains(CacheCategory::WasmBlob, "key3", None));

        store.clear().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
