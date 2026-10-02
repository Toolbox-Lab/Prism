//! [`MemoryCache`]: a [`CacheProvider`] backed by [`dashmap`], a concurrent,
//! sharded hash map. Values are serialized with `bincode` into `Arc`-wrapped
//! byte slices, so lookups only clone an atomic reference count instead of
//! copying (or locking around) the payload — no `Mutex<HashMap>` contention,
//! and no latency spikes under high-frequency read/write workloads.
//!
//! Unlike [`crate::cache::disk::DiskCache`], entries are **not** persistent:
//! everything lives in the process heap and is dropped on exit.

use crate::cache::provider::CacheProvider;
use crate::error::{GratError, GratResult};

use dashmap::DashMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::sync::Arc;

/// Type-erased serialized payload shared across lookups.
///
/// `Arc<[u8]>` (a fat pointer straight into the allocation) is preferred over
/// `Arc<Vec<u8>>` to avoid an extra indirection on every read.
type SharedBytes = Arc<[u8]>;

/// High-frequency, low-latency in-memory cache backed by a [`DashMap`].
///
/// `DashMap` shards its buckets internally, so concurrent operations on
/// distinct keys never contend on a global lock (unlike `Mutex<HashMap>`),
/// while single-key operations remain as fast as a plain `HashMap`.
///
/// The cache is cheaply [`Clone`]-able: all handles share the same map, so it
/// can be freely moved into spawned tasks and concurrent consumers.
///
/// Keys are typed as [`String`] to satisfy the [`CacheProvider`] contract;
/// entries are stored serialized (`bincode`) and deserialized on `get`,
/// mirroring the disk backend's wire format.
#[derive(Clone, Default)]
pub struct MemoryCache {
    entries: Arc<DashMap<String, SharedBytes>>,
}

impl MemoryCache {
    /// Creates an empty in-memory cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of entries currently stored in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl CacheProvider for MemoryCache {
    fn get<V>(&self, key: &str) -> impl Future<Output = GratResult<Option<V>>> + Send
    where
        V: DeserializeOwned + Send,
    {
        // Only an atomic refcount bump happens while the shard lock is held;
        // deserialization (potentially expensive) runs on the local copy
        // after the guard is dropped.
        let bytes = self.entries.get(key).map(|entry| Arc::clone(entry.value()));

        async move {
            let Some(bytes) = bytes else {
                return Ok(None);
            };

            let value =
                bincode::deserialize(&bytes).map_err(|e| GratError::CacheDeserializationError {
                    key: key.to_string(),
                    reason: e.to_string(),
                })?;

            Ok(Some(value))
        }
    }

    fn put<V>(&self, key: &str, value: &V) -> impl Future<Output = GratResult<()>> + Send
    where
        V: Serialize + Sync,
    {
        let key = key.to_owned();
        let encoded = bincode::serialize(value).map_err(|e| GratError::CacheSerializationError {
            key: key.clone(),
            reason: e.to_string(),
        });

        async move {
            let encoded = encoded?;
            self.entries
                .insert(key, Arc::from(encoded.into_boxed_slice()));
            Ok(())
        }
    }

    fn remove(&self, key: &str) -> impl Future<Output = GratResult<()>> + Send {
        self.entries.remove(key);

        std::future::ready(Ok(()))
    }

    fn clear(&self) -> impl Future<Output = GratResult<()>> + Send {
        // `retain` with an always-false predicate drops every entry while
        // keeping the map (and its shards) allocated for reuse.
        self.entries.retain(|_, _| false);

        std::future::ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GratError;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        id: u32,
        name: String,
    }

    #[tokio::test]
    async fn test_put_get_roundtrip() {
        let cache = MemoryCache::new();

        let value = Sample {
            id: 1,
            name: "wasm-blob".to_string(),
        };
        cache.put("key1", &value).await.unwrap();

        let fetched: Option<Sample> = cache.get("key1").await.unwrap();
        assert_eq!(fetched, Some(value));
    }

    #[tokio::test]
    async fn test_put_overwrites_existing_entry() {
        let cache = MemoryCache::new();

        cache.put("key1", &1u32).await.unwrap();
        cache.put("key1", &2u32).await.unwrap();

        assert_eq!(cache.get::<u32>("key1").await.unwrap(), Some(2));
    }

    #[tokio::test]
    async fn test_cache_miss_returns_ok_none() {
        let cache = MemoryCache::new();

        let fetched: Option<Sample> = cache.get("missing").await.unwrap();
        assert_eq!(fetched, None);
    }

    #[tokio::test]
    async fn test_remove() {
        let cache = MemoryCache::new();

        cache.put("key1", &42u32).await.unwrap();
        cache.remove("key1").await.unwrap();

        let fetched: Option<u32> = cache.get("key1").await.unwrap();
        assert_eq!(fetched, None);
    }

    #[tokio::test]
    async fn test_remove_of_missing_key_is_not_an_error() {
        let cache = MemoryCache::new();

        cache.remove("never-existed").await.unwrap();
    }

    #[tokio::test]
    async fn test_clear() {
        let cache = MemoryCache::new();

        cache.put("key1", &1u32).await.unwrap();
        cache.put("key2", &2u32).await.unwrap();
        cache.clear().await.unwrap();

        assert_eq!(cache.get::<u32>("key1").await.unwrap(), None);
        assert_eq!(cache.get::<u32>("key2").await.unwrap(), None);
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_len_and_is_empty_track_entries() {
        let cache = MemoryCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        cache.put("key1", &1u32).await.unwrap();
        cache.put("key2", &2u32).await.unwrap();
        assert_eq!(cache.len(), 2);

        cache.remove("key1").await.unwrap();
        assert_eq!(cache.len(), 1);
    }

    #[tokio::test]
    async fn test_deserialize_type_mismatch_returns_typed_error() {
        let cache = MemoryCache::new();

        // bincode is not self-describing, so a mismatch only reliably surfaces
        // as an error when the stored bytes are too short for the target type
        // (here, a bare u32 has no bytes left for `Sample`'s trailing String).
        cache.put("key1", &1u32).await.unwrap();

        let err = cache.get::<Sample>("key1").await.unwrap_err();
        assert!(matches!(err, GratError::CacheDeserializationError { .. }));
    }

    #[tokio::test]
    async fn test_concurrent_put_get_across_tasks() {
        let cache = MemoryCache::new();

        let mut handles = Vec::new();
        for i in 0..64u32 {
            let cache = cache.clone();
            handles.push(tokio::spawn(async move {
                let key = format!("key-{i}");
                cache.put(&key, &i).await.unwrap();
                let fetched: Option<u32> = cache.get(&key).await.unwrap();
                assert_eq!(fetched, Some(i));
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(cache.len(), 64);
    }

    #[tokio::test]
    async fn test_mixed_concurrent_reads_writes_removes_and_clears() {
        let cache = MemoryCache::new();
        for i in 0..32u32 {
            cache.put(&format!("seed-{i}"), &i).await.unwrap();
        }

        let mut handles = Vec::new();
        for w in 0..8usize {
            let cache = cache.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..128u32 {
                    let key = format!("worker-{w}-{i}");
                    cache.put(&key, &(w as u32, i)).await.unwrap();

                    // Concurrently hit seeded entries (reads) and hammer the
                    // same keys other workers use (writes/removes) to shake
                    // out shard contention and iterator/guard hazards.
                    let _ = cache.get::<u32>("seed-0").await.unwrap();
                    let _ = cache.get::<(u32, u32)>(&key).await.unwrap();

                    if i % 8 == 0 {
                        cache.remove("seed-0").await.unwrap();
                    }
                    if i == 127 {
                        cache.clear().await.unwrap();
                    }
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Every worker cleared at the end; all readers/writers completed
        // without deadlock or panic, and the map reflects the final clear.
        assert!(cache.is_empty());
    }

    #[test]
    fn test_raw_handle_concurrent_threads_beyond_async() {
        // Exercise DashMap's sharded locking from plain OS threads too (the
        // issue's "multiple threads" requirement), not just tokio tasks.
        let cache = MemoryCache::new();
        let cache = Arc::new(cache);

        let mut handles = Vec::new();
        for t in 0..8usize {
            let cache = Arc::clone(&cache);
            handles.push(std::thread::spawn(move || {
                for i in 0..256u32 {
                    let key = format!("t{t}-i{i}");
                    cache.put(&key, &i).now_or_never().unwrap().unwrap();
                    assert_eq!(
                        cache.get::<u32>(&key).now_or_never().unwrap().unwrap(),
                        Some(i)
                    );
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(cache.len(), 8 * 256);
    }

    // Tiny helper so the raw-thread test can drive the RPITIT futures to
    // completion without a runtime; every operation here is synchronous
    // under the hood, so a first poll always resolves.
    trait NowOrNever: Future + Sized {
        fn now_or_never(self) -> Option<Self::Output> {
            let mut fut = Box::pin(self);
            match fut
                .as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
            {
                std::task::Poll::Ready(out) => Some(out),
                std::task::Poll::Pending => None,
            }
        }
    }

    impl<F: Future> NowOrNever for F {}
}
