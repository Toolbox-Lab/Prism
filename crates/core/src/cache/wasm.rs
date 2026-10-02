use crate::cache::provider::CacheProvider;
use crate::error::GratResult;
use sha2::{Digest, Sha256};

/// Specialized cache for compiled WebAssembly modules.
/// Uses a cryptographic hash (SHA-256) of the original Wasm bytecode as the key
/// to uniquely identify and retrieve the compiled module.
pub struct WasmCache<P: CacheProvider> {
    provider: P,
}

impl<P: CacheProvider> WasmCache<P> {
    /// Creates a new WasmCache wrapping the given cache provider.
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Computes the SHA-256 hash of the given bytecode to be used as a cache key.
    pub fn hash_bytecode(bytecode: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytecode);
        hex::encode(hasher.finalize())
    }

    /// Retrieves a cached compiled Wasm module for the given original bytecode.
    /// Uses the hash of the bytecode as the lookup key.
    pub async fn get(&self, bytecode: &[u8]) -> GratResult<Option<Vec<u8>>> {
        let key = Self::hash_bytecode(bytecode);
        self.provider.get(&key).await
    }

    /// Stores a compiled Wasm module into the cache, using the hash of the
    /// original bytecode as the key.
    pub async fn put(&self, bytecode: &[u8], compiled_module: &[u8]) -> GratResult<()> {
        let key = Self::hash_bytecode(bytecode);
        self.provider.put(&key, &compiled_module.to_vec()).await
    }

    /// Clears the underlying cache.
    pub async fn clear(&self) -> GratResult<()> {
        self.provider.clear().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::provider::CacheProvider;
    use crate::error::GratResult;
    use serde::{de::DeserializeOwned, Serialize};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemoryCacheDouble {
        entries: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl InMemoryCacheDouble {
        fn new() -> Self {
            Self::default()
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
                    let value = serde_json::from_slice(&bytes).unwrap();
                    Ok(Some(value))
                }
                None => Ok(None),
            }
        }

        async fn put<V>(&self, key: &str, value: &V) -> GratResult<()>
        where
            V: Serialize + Sync,
        {
            let encoded = serde_json::to_vec(value).unwrap();
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

    #[tokio::test]
    async fn test_wasm_cache_roundtrip() {
        let provider = InMemoryCacheDouble::new();
        let cache = WasmCache::new(provider);

        let bytecode = b"dummy wasm module";
        let compiled = b"compiled native code";

        // Should be empty initially
        assert_eq!(cache.get(bytecode).await.unwrap(), None);

        // Put into cache
        cache.put(bytecode, compiled).await.unwrap();

        // Should retrieve exactly the compiled bytecode
        let retrieved = cache.get(bytecode).await.unwrap();
        assert_eq!(retrieved, Some(compiled.to_vec()));
    }

    #[tokio::test]
    async fn test_wasm_cache_hash_consistency() {
        let bytecode = b"test bytes";
        let hash1 = WasmCache::<InMemoryCacheDouble>::hash_bytecode(bytecode);
        let hash2 = WasmCache::<InMemoryCacheDouble>::hash_bytecode(bytecode);
        assert_eq!(hash1, hash2);

        let hash3 = WasmCache::<InMemoryCacheDouble>::hash_bytecode(b"other bytes");
        assert_ne!(hash1, hash3);
    }
}
