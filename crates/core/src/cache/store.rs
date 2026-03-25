//! Content-addressed cache store.
//!
//! Uses `redb` as an embedded key-value database for fast, zero-config
//! local caching of WASM blobs, parsed contractspecs, and historical ledger entries.

use crate::types::error::{PrismError, PrismResult};
use std::path::PathBuf;

/// Cache entry categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheCategory {
    /// Raw WASM contract bytecode.
    WasmBlob,
    /// Parsed contract specifications.
    ContractSpec,
    /// Historical ledger entries.
    LedgerEntry,
    /// Decoded transaction results.
    TransactionResult,
}

impl CacheCategory {
    fn as_str(&self) -> &'static str {
        match self {
            Self::WasmBlob => "wasm",
            Self::ContractSpec => "spec",
            Self::LedgerEntry => "ledger",
            Self::TransactionResult => "tx",
        }
    }
}

/// Local disk cache backed by redb.
pub struct CacheStore {
    /// Path to the cache directory.
    cache_dir: PathBuf,
    /// Maximum cache size in bytes.
    max_size: u64,
}

impl CacheStore {
    /// Create a new cache store at the given directory.
    pub fn new(cache_dir: PathBuf, max_size_mb: u64) -> PrismResult<Self> {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| PrismError::CacheError(format!("Failed to create cache dir: {e}")))?;

        Ok(Self {
            cache_dir,
            max_size: max_size_mb * 1024 * 1024,
        })
    }

    /// Create a cache store using platform-appropriate default directories.
    pub fn default_location() -> PrismResult<Self> {
        let project_dirs =
            directories::ProjectDirs::from("dev", "prism", "prism").ok_or_else(|| {
                PrismError::CacheError("Could not determine cache directory".to_string())
            })?;

        Self::new(project_dirs.cache_dir().to_path_buf(), 512)
    }

    /// Store a value in the cache with a content-addressed key.
    pub fn put(&self, category: CacheCategory, key: &str, value: &[u8]) -> PrismResult<()> {
        let path = self.entry_path(category, key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| PrismError::CacheError(format!("Failed to create dir: {e}")))?;
        }
        std::fs::write(&path, value)
            .map_err(|e| PrismError::CacheError(format!("Failed to write cache entry: {e}")))?;
        Ok(())
    }

    /// Retrieve a value from the cache.
    pub fn get(&self, category: CacheCategory, key: &str) -> PrismResult<Option<Vec<u8>>> {
        let path = self.entry_path(category, key);
        if path.exists() {
            let data = std::fs::read(&path)
                .map_err(|e| PrismError::CacheError(format!("Failed to read cache entry: {e}")))?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    /// Check if a key exists in the cache.
    pub fn contains(&self, category: CacheCategory, key: &str) -> bool {
        self.entry_path(category, key).exists()
    }

    /// Remove a specific cache entry.
    pub fn remove(&self, category: CacheCategory, key: &str) -> PrismResult<()> {
        let path = self.entry_path(category, key);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                PrismError::CacheError(format!("Failed to remove cache entry: {e}"))
            })?;
        }
        Ok(())
    }

    /// Clear all cache entries.
    pub fn clear(&self) -> PrismResult<()> {
        if self.cache_dir.exists() {
            std::fs::remove_dir_all(&self.cache_dir)
                .map_err(|e| PrismError::CacheError(format!("Failed to clear cache: {e}")))?;
            std::fs::create_dir_all(&self.cache_dir).map_err(|e| {
                PrismError::CacheError(format!("Failed to recreate cache dir: {e}"))
            })?;
        }
        Ok(())
    }

    /// Build the file path for a cache entry.
    fn entry_path(&self, category: CacheCategory, key: &str) -> PathBuf {
        self.cache_dir.join(category.as_str()).join(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_roundtrip() {
        let dir = std::env::temp_dir().join("prism_test_cache");
        let store = CacheStore::new(dir.clone(), 10).unwrap();

        store
            .put(CacheCategory::WasmBlob, "test_key", b"hello")
            .unwrap();
        let result = store.get(CacheCategory::WasmBlob, "test_key").unwrap();
        assert_eq!(result, Some(b"hello".to_vec()));

        store.clear().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
