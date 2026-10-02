//! [`DiskCache`]: a [`CacheProvider`] backed by [`sled`], a persistent,
//! embedded, lock-free B+tree store. Values are serialized with `bincode`
//! before being written to the tree, and survive process restarts.

use crate::cache::provider::CacheProvider;
use crate::error::{GratError, GratResult};

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;

/// Persistent, embedded cache backed by a [`sled`] database.
///
/// `sled` provides lock-free concurrent access internally, so `DiskCache`
/// can be freely cloned and shared across tasks; blocking I/O is offloaded
/// to `tokio::task::spawn_blocking` to keep the async executor unblocked.
#[derive(Clone)]
pub struct DiskCache {
    db: Arc<sled::Db>,
}

impl DiskCache {
    fn ledger_key(key: &str, ledger_sequence: u32) -> Vec<u8> {
        let mut bytes = ledger_sequence.to_be_bytes().to_vec();
        bytes.extend_from_slice(b":");
        bytes.extend_from_slice(key.as_bytes());
        bytes
    }

    /// Opens (or creates) a sled database at `path`.
    pub fn new(path: impl AsRef<Path>) -> GratResult<Self> {
        let path = path.as_ref();
        let db = sled::open(path).map_err(|e| {
            GratError::CacheError(format!(
                "Failed to open sled database at {}: {e}",
                path.display()
            ))
        })?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Opens the disk cache at the platform's default cache directory.
    pub fn default_location() -> GratResult<Self> {
        let project_dirs =
            directories::ProjectDirs::from("dev", "grat", "grat").ok_or_else(|| {
                GratError::CacheError("Could not determine cache directory".to_string())
            })?;

        Self::new(project_dirs.cache_dir().join("disk_cache"))
    }

    pub async fn put<V>(&self, key: &str, value: &V, ledger_sequence: Option<u32>) -> GratResult<()>
    where
        V: Serialize + Sync,
    {
        let key = key.to_owned();
        let encoded = bincode::serialize(value).map_err(|e| GratError::CacheSerializationError {
            key: key.clone(),
            reason: e.to_string(),
        });

        let db = Arc::clone(&self.db);

        async move {
            let encoded = encoded?;
            let cache_key = match ledger_sequence {
                Some(sequence) => Self::ledger_key(&key, sequence),
                None => key.as_bytes().to_vec(),
            };

            tokio::task::spawn_blocking(move || {
                db.insert(cache_key.clone(), encoded.clone())
                    .map_err(|e| GratError::CacheError(format!("Sled insert failed: {e}")))?;

                if let Some(sequence) = ledger_sequence {
                    let ledger_index = db.open_tree(b"ledger_index")
                        .map_err(|e| GratError::CacheError(format!("Failed to open ledger index: {e}")))?;
                    let entry_key = Self::ledger_key(&key, sequence);
                    ledger_index
                        .insert(entry_key.clone(), key.as_bytes())
                        .map_err(|e| GratError::CacheError(format!("Failed to index ledger entry: {e}")))?;
                    let _ = entry_key;
                }
                Ok::<(), GratError>(())
            })
            .await
            .map_err(|e| GratError::CacheError(format!("Cache task panicked: {e}")))??;

            Ok(())
        }
    }

    pub async fn get<V>(&self, key: &str, ledger_sequence: Option<u32>) -> GratResult<Option<V>>
    where
        V: DeserializeOwned + Send,
    {
        let db = Arc::clone(&self.db);
        let key = key.to_owned();

        async move {
            let lookup_key = match ledger_sequence {
                Some(sequence) => Self::ledger_key(&key, sequence),
                None => key.as_bytes().to_vec(),
            };

            let raw = tokio::task::spawn_blocking(move || db.get(lookup_key))
                .await
                .map_err(|e| GratError::CacheError(format!("Cache task panicked: {e}")))?
                .map_err(|e| GratError::CacheError(format!("Sled get failed: {e}")))?;

            let Some(bytes) = raw else {
                return Ok(None);
            };

            let value = bincode::deserialize(&bytes).map_err(|e| GratError::CacheDeserializationError {
                key,
                reason: e.to_string(),
            })?;

            Ok(Some(value))
        }
    }

    pub async fn remove(&self, key: &str, ledger_sequence: Option<u32>) -> GratResult<()> {
        let db = Arc::clone(&self.db);
        let key = key.to_owned();

        async move {
            let lookup_key = match ledger_sequence {
                Some(sequence) => Self::ledger_key(&key, sequence),
                None => key.as_bytes().to_vec(),
            };

            tokio::task::spawn_blocking(move || {
                db.remove(lookup_key)
                    .map_err(|e| GratError::CacheError(format!("Sled remove failed: {e}")))?;
                if let Some(sequence) = ledger_sequence {
                    let index = db.open_tree(b"ledger_index")
                        .map_err(|e| GratError::CacheError(format!("Failed to open ledger index: {e}")))?;
                    index
                        .remove(Self::ledger_key(&key, sequence))
                        .map_err(|e| GratError::CacheError(format!("Failed to remove ledger index entry: {e}")))?;
                }
                Ok::<(), GratError>(())
            })
            .await
            .map_err(|e| GratError::CacheError(format!("Cache task panicked: {e}")))??;

            Ok(())
        }
    }

    pub async fn prune_older_than(&self, ledger_sequence: u32) -> GratResult<()> {
        let db = Arc::clone(&self.db);

        async move {
            tokio::task::spawn_blocking(move || {
                let ledger_index = db.open_tree(b"ledger_index")
                    .map_err(|e| GratError::CacheError(format!("Failed to open ledger index: {e}")))?;

                let mut keys_to_remove = Vec::new();
                for seq in 0..ledger_sequence {
                    let prefix = seq.to_be_bytes();
                    for entry in ledger_index.scan_prefix(prefix) {
                        let (index_key, _) = entry.map_err(|e| {
                            GratError::CacheError(format!("Sled prefix scan failed: {e}"))
                        })?;
                        keys_to_remove.push(index_key.to_vec());
                    }
                }

                for index_key in keys_to_remove {
                    db.remove(index_key.clone())
                        .map_err(|e| GratError::CacheError(format!("Sled remove failed: {e}")))?;
                    ledger_index
                        .remove(index_key)
                        .map_err(|e| GratError::CacheError(format!("Failed to remove ledger index entry: {e}")))?;
                }

                Ok::<(), GratError>(())
            })
            .await
            .map_err(|e| GratError::CacheError(format!("Cache task panicked: {e}")))??;

            Ok(())
        }
    }

    /// Flushes any buffered writes to disk. Sled batches and lazily flushes
    /// writes for performance, so callers that need durability guarantees
    /// (e.g. before a restart) should await this explicitly.
    pub async fn flush(&self) -> GratResult<()> {
        self.db
            .flush_async()
            .await
            .map_err(|e| GratError::CacheError(format!("Sled flush failed: {e}")))?;
        Ok(())
    }
}

impl CacheProvider for DiskCache {
    fn get<V>(&self, key: &str) -> impl Future<Output = GratResult<Option<V>>> + Send
    where
        V: DeserializeOwned + Send,
    {
        self.get(key, None)
    }

    fn put<V>(&self, key: &str, value: &V) -> impl Future<Output = GratResult<()>> + Send
    where
        V: Serialize + Sync,
    {
        self.put(key, value, None)
    }

    fn remove(&self, key: &str) -> impl Future<Output = GratResult<()>> + Send {
        self.remove(key, None)
    }

    fn clear(&self) -> impl Future<Output = GratResult<()>> + Send {
        let db = Arc::clone(&self.db);

        async move {
            tokio::task::spawn_blocking(move || db.clear())
                .await
                .map_err(|e| GratError::CacheError(format!("Cache task panicked: {e}")))?
                .map_err(|e| GratError::CacheError(format!("Sled clear failed: {e}")))?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        id: u32,
        name: String,
    }

    #[tokio::test]
    async fn test_put_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        let value = Sample {
            id: 1,
            name: "wasm-blob".to_string(),
        };
        cache.put("key1", &value, None).await.unwrap();

        let fetched: Option<Sample> = cache.get("key1", None).await.unwrap();
        assert_eq!(fetched, Some(value));
    }

    #[tokio::test]
    async fn test_put_overwrites_existing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        cache.put("key1", &1u32, None).await.unwrap();
        cache.put("key1", &2u32, None).await.unwrap();

        assert_eq!(cache.get::<u32>("key1", None).await.unwrap(), Some(2));
    }

    #[tokio::test]
    async fn test_cache_miss_returns_ok_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        let fetched: Option<Sample> = cache.get("missing", None).await.unwrap();
        assert_eq!(fetched, None);
    }

    #[tokio::test]
    async fn test_remove() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        cache.put("key1", &42u32, None).await.unwrap();
        cache.remove("key1", None).await.unwrap();

        let fetched: Option<u32> = cache.get("key1", None).await.unwrap();
        assert_eq!(fetched, None);
    }

    #[tokio::test]
    async fn test_remove_of_missing_key_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        cache.remove("never-existed", None).await.unwrap();
    }

    #[tokio::test]
    async fn test_clear() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        cache.put("key1", &1u32, None).await.unwrap();
        cache.put("key2", &2u32, None).await.unwrap();
        cache.clear().await.unwrap();

        assert_eq!(cache.get::<u32>("key1", None).await.unwrap(), None);
        assert_eq!(cache.get::<u32>("key2", None).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_persistence_across_reopen() {
        let dir = tempfile::tempdir().unwrap();

        {
            let cache = DiskCache::new(dir.path()).unwrap();
            cache.put("durable", &"value".to_string(), None).await.unwrap();
            cache.flush().await.unwrap();
        }

        let reopened = DiskCache::new(dir.path()).unwrap();
        let fetched: Option<String> = reopened.get("durable", None).await.unwrap();
        assert_eq!(fetched, Some("value".to_string()));
    }

    #[tokio::test]
    async fn test_concurrent_put_get() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        let mut handles = Vec::new();
        for i in 0..20u32 {
            let cache = cache.clone();
            handles.push(tokio::spawn(async move {
                let key = format!("key-{i}");
                cache.put(&key, &i, None).await.unwrap();
                let fetched: Option<u32> = cache.get(&key, None).await.unwrap();
                assert_eq!(fetched, Some(i));
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_deserialize_type_mismatch_returns_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        // bincode is not self-describing, so a mismatch only reliably surfaces
        // as an error when the stored bytes are too short for the target type
        // (here, a bare u32 has no bytes left for `Sample`'s trailing String).
        cache.put("key1", &1u32, None).await.unwrap();

        let err = cache.get::<Sample>("key1", None).await.unwrap_err();
        assert!(matches!(err, GratError::CacheDeserializationError { .. }));
    }

    #[tokio::test]
    async fn test_put_get_with_ledger_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        cache
            .put("entry", &42u32, Some(101u32))
            .await
            .unwrap();

        assert_eq!(cache.get::<u32>("entry", Some(101)).await.unwrap(), Some(42));
        assert_eq!(cache.get::<u32>("entry", Some(100)).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_prune_older_than_removes_ledger_scoped_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path()).unwrap();

        cache.put("old", &1u32, Some(10)).await.unwrap();
        cache.put("recent", &2u32, Some(20)).await.unwrap();
        cache.put("newer", &3u32, Some(30)).await.unwrap();

        cache.prune_older_than(20).await.unwrap();

        assert_eq!(cache.get::<u32>("old", Some(10)).await.unwrap(), None);
        assert_eq!(cache.get::<u32>("recent", Some(20)).await.unwrap(), Some(2));
        assert_eq!(cache.get::<u32>("newer", Some(30)).await.unwrap(), Some(3));
    }
}
