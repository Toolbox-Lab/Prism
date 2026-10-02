//! [`CacheProvider`]: the shared contract every cache backend (in-memory, disk,
//! Wasm-specific, ...) implements, so the rest of the codebase can depend on
//! "a cache" without caring which storage mechanism backs it.

use crate::error::GratResult;

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;

/// Unified contract for caching backends (in-memory, disk, or Wasm-specific),
/// allowing them to be substituted without changing consuming logic.
///
/// Methods return their futures via `impl Future<..> + Send` (RPITIT) rather
/// than `async fn`, since a plain `async fn` in a trait does not guarantee
/// the returned future is [`Send`] — callers on a multi-threaded executor
/// (e.g. spawning cache lookups onto `tokio::spawn`) need that guarantee.
/// This also avoids the boxing/allocation overhead of `#[async_trait]`.
///
/// A cache miss is not an error: [`CacheProvider::get`] resolves to
/// `Ok(None)`. Implementations should reserve `GratError::CacheMiss` (and
/// the other dedicated `Cache*` variants on `GratError`) for APIs that
/// build on top of this trait and need a hard failure on a missing key.
pub trait CacheProvider: Send + Sync {
    /// Fetches and deserializes the value stored under `key`.
    ///
    /// Returns `Ok(None)` on a cache miss — never an error. Errors are
    /// reserved for backend failures (I/O, deserialization, etc.).
    fn get<V>(&self, key: &str) -> impl Future<Output = GratResult<Option<V>>> + Send
    where
        V: DeserializeOwned + Send;

    /// Serializes `value` and stores it under `key`, overwriting any
    /// existing entry.
    ///
    /// Returns `GratError::CacheCapacityExceeded` if the entry does not
    /// fit within the backend's configured size limit.
    fn put<V>(&self, key: &str, value: &V) -> impl Future<Output = GratResult<()>> + Send
    where
        V: Serialize + Sync;

    /// Removes the entry stored under `key`, if any. Removing a key that is
    /// not present is not an error.
    fn remove(&self, key: &str) -> impl Future<Output = GratResult<()>> + Send;

    /// Removes all entries from the cache.
    fn clear(&self) -> impl Future<Output = GratResult<()>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GratError;
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Hand-rolled in-memory test double for [`CacheProvider`].
    ///
    /// Not a production backend (that's #403/#404/#405) — it exists purely
    /// as a conformance target so the trait's contract (miss => `Ok(None)`,
    /// put overwrites, remove/clear behavior) has a test suite that any real
    /// backend can be run against by copying these cases.
    #[derive(Default)]
    struct InMemoryCacheDouble {
        entries: Mutex<HashMap<String, Vec<u8>>>,
        max_entry_size: Option<usize>,
    }

    impl InMemoryCacheDouble {
        fn new() -> Self {
            Self::default()
        }

        fn with_max_entry_size(max_entry_size: usize) -> Self {
            Self {
                entries: Mutex::new(HashMap::new()),
                max_entry_size: Some(max_entry_size),
            }
        }
    }

    impl CacheProvider for InMemoryCacheDouble {
        async fn get<V>(&self, key: &str) -> GratResult<Option<V>>
        where
            V: DeserializeOwned + Send,
        {
            let bytes = self.entries.lock().unwrap().get(key).cloned();
            match bytes {
                Some(bytes) => {
                    let value = serde_json::from_slice(&bytes).map_err(|e| {
                        GratError::CacheDeserializationError {
                            key: key.to_string(),
                            reason: e.to_string(),
                        }
                    })?;
                    Ok(Some(value))
                }
                None => Ok(None),
            }
        }

        async fn put<V>(&self, key: &str, value: &V) -> GratResult<()>
        where
            V: Serialize + Sync,
        {
            let encoded =
                serde_json::to_vec(value).map_err(|e| GratError::CacheSerializationError {
                    key: key.to_string(),
                    reason: e.to_string(),
                })?;

            if let Some(limit) = self.max_entry_size {
                if encoded.len() > limit {
                    return Err(GratError::CacheCapacityExceeded {
                        key: key.to_string(),
                        entry_size: encoded.len() as u64,
                        limit: limit as u64,
                    });
                }
            }

            self.entries
                .lock()
                .unwrap()
                .insert(key.to_string(), encoded);
            Ok(())
        }

        async fn remove(&self, key: &str) -> GratResult<()> {
            self.entries.lock().unwrap().remove(key);
            Ok(())
        }

        async fn clear(&self) -> GratResult<()> {
            self.entries.lock().unwrap().clear();
            Ok(())
        }
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        id: u32,
        name: String,
    }

    #[tokio::test]
    async fn get_after_put_roundtrips() {
        let cache = InMemoryCacheDouble::new();
        let value = Sample {
            id: 1,
            name: "wasm-blob".to_string(),
        };

        cache.put("key1", &value).await.unwrap();
        let fetched: Option<Sample> = cache.get("key1").await.unwrap();

        assert_eq!(fetched, Some(value));
    }

    #[tokio::test]
    async fn put_overwrites_existing_entry() {
        let cache = InMemoryCacheDouble::new();

        cache.put("key1", &1u32).await.unwrap();
        cache.put("key1", &2u32).await.unwrap();

        assert_eq!(cache.get::<u32>("key1").await.unwrap(), Some(2));
    }

    #[tokio::test]
    async fn miss_returns_ok_none_not_an_error() {
        let cache = InMemoryCacheDouble::new();

        let fetched: Option<Sample> = cache.get("missing").await.unwrap();

        assert_eq!(fetched, None);
    }

    #[tokio::test]
    async fn remove_deletes_entry() {
        let cache = InMemoryCacheDouble::new();
        cache.put("key1", &42u32).await.unwrap();

        cache.remove("key1").await.unwrap();

        assert_eq!(cache.get::<u32>("key1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn remove_of_missing_key_is_not_an_error() {
        let cache = InMemoryCacheDouble::new();

        cache.remove("never-existed").await.unwrap();
    }

    #[tokio::test]
    async fn clear_removes_all_entries() {
        let cache = InMemoryCacheDouble::new();
        cache.put("key1", &1u32).await.unwrap();
        cache.put("key2", &2u32).await.unwrap();

        cache.clear().await.unwrap();

        assert_eq!(cache.get::<u32>("key1").await.unwrap(), None);
        assert_eq!(cache.get::<u32>("key2").await.unwrap(), None);
    }

    #[tokio::test]
    async fn put_over_capacity_returns_typed_error() {
        let cache = InMemoryCacheDouble::with_max_entry_size(4);

        let err = cache
            .put("key1", &"this value is far too long to fit".to_string())
            .await
            .unwrap_err();

        assert!(matches!(err, GratError::CacheCapacityExceeded { .. }));
    }
}
