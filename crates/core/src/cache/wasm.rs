use crate::cache::provider::{
    cache_key, get_if_enabled, ledger_cache_key, put_if_enabled, CacheProvider, SPEC_CACHE_PREFIX,
    WASM_CACHE_PREFIX,
};
use crate::error::GratResult;
use sha2::{Digest, Sha256};
use stellar_xdr::curr::ScSpecEntry;

/// Specialized cache for raw contract WebAssembly bytecode.
///
/// Entries are keyed by contract ID or by the SHA-256 hash of the bytecode, so
/// repeated inspections of the same contract avoid re-downloading the blob.
pub struct WasmCache<P: CacheProvider> {
    provider: P,
    no_cache: bool,
}

impl<P: CacheProvider> WasmCache<P> {
    /// Creates a cache that reads from and writes to `provider`.
    pub fn new(provider: P) -> Self {
        Self::with_bypass(provider, false)
    }

    /// Creates a cache that bypasses reads and writes when `no_cache` is set.
    pub fn with_bypass(provider: P, no_cache: bool) -> Self {
        Self { provider, no_cache }
    }

    /// Returns `true` when the CLI `--no-cache` flag is in effect.
    pub fn is_bypassed(&self) -> bool {
        self.no_cache
    }

    /// Computes the SHA-256 hash of the given bytecode to be used as a cache key.
    pub fn hash_bytecode(bytecode: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytecode);
        hex::encode(hasher.finalize())
    }

    fn entry_key(key: &str) -> String {
        cache_key(WASM_CACHE_PREFIX, key)
    }

    /// Retrieves raw contract bytecode by contract ID or WASM hash.
    pub async fn get_wasm(&self, key: &str) -> GratResult<Option<Vec<u8>>> {
        get_if_enabled(&self.provider, self.no_cache, &Self::entry_key(key)).await
    }

    /// Stores raw contract bytecode under a contract ID or WASM hash.
    pub async fn store_wasm(&self, key: &str, bytecode: &[u8]) -> GratResult<()> {
        put_if_enabled(
            &self.provider,
            self.no_cache,
            &Self::entry_key(key),
            &bytecode.to_vec(),
        )
        .await
    }

    /// Retrieves a module previously stored under the bytecode's hash.
    pub async fn get(&self, bytecode: &[u8]) -> GratResult<Option<Vec<u8>>> {
        self.get_wasm(&Self::hash_bytecode(bytecode)).await
    }

    /// Stores a module under the hash of the original bytecode.
    pub async fn put(&self, bytecode: &[u8], compiled_module: &[u8]) -> GratResult<()> {
        self.store_wasm(&Self::hash_bytecode(bytecode), compiled_module)
            .await
    }

    /// Clears the underlying cache provider.
    pub async fn clear(&self) -> GratResult<()> {
        if self.no_cache {
            return Ok(());
        }
        self.provider.clear().await
    }
}

/// Cache for decoded `contractspecv0` sections.
///
/// Stores the decoded `Vec<ScSpecEntry>` rather than raw WASM so repeated
/// inspections skip both the download and the XDR/custom-section decoding.
pub struct ContractSpecCache<P: CacheProvider> {
    provider: P,
    no_cache: bool,
}

impl<P: CacheProvider> ContractSpecCache<P> {
    pub fn new(provider: P) -> Self {
        Self::with_bypass(provider, false)
    }

    pub fn with_bypass(provider: P, no_cache: bool) -> Self {
        Self { provider, no_cache }
    }

    pub fn is_bypassed(&self) -> bool {
        self.no_cache
    }

    fn entry_key(key: &str) -> String {
        cache_key(SPEC_CACHE_PREFIX, key)
    }

    /// Retrieves decoded spec entries by contract ID or WASM hash.
    pub async fn get_spec(&self, key: &str) -> GratResult<Option<Vec<ScSpecEntry>>> {
        get_if_enabled(&self.provider, self.no_cache, &Self::entry_key(key)).await
    }

    /// Stores decoded spec entries under a contract ID or WASM hash.
    pub async fn store_spec(&self, key: &str, entries: &[ScSpecEntry]) -> GratResult<()> {
        put_if_enabled(
            &self.provider,
            self.no_cache,
            &Self::entry_key(key),
            &entries.to_vec(),
        )
        .await
    }

    pub async fn get(&self, key: &str) -> GratResult<Option<Vec<ScSpecEntry>>> {
        self.get_spec(key).await
    }

    pub async fn put(&self, key: &str, entries: &[ScSpecEntry]) -> GratResult<()> {
        self.store_spec(key, entries).await
    }

    pub async fn clear(&self) -> GratResult<()> {
        if self.no_cache {
            return Ok(());
        }
        self.provider.clear().await
    }
}

/// Cache for ledger entry snapshots taken during replay.
///
/// Entries are indexed by ledger sequence and ledger key, so a snapshot for a
/// historical ledger is never confused with the same key at another sequence.
pub struct LedgerEntryCache<P: CacheProvider> {
    provider: P,
    no_cache: bool,
}

impl<P: CacheProvider> LedgerEntryCache<P> {
    pub fn new(provider: P) -> Self {
        Self::with_bypass(provider, false)
    }

    pub fn with_bypass(provider: P, no_cache: bool) -> Self {
        Self { provider, no_cache }
    }

    pub fn is_bypassed(&self) -> bool {
        self.no_cache
    }

    /// Retrieves a snapshot by ledger sequence and ledger key.
    pub async fn get_entry(&self, ledger_sequence: u32, key: &str) -> GratResult<Option<Vec<u8>>> {
        get_if_enabled(
            &self.provider,
            self.no_cache,
            &ledger_cache_key(ledger_sequence, key),
        )
        .await
    }

    /// Stores a snapshot by ledger sequence and ledger key.
    pub async fn store_entry(
        &self,
        ledger_sequence: u32,
        key: &str,
        entry: &[u8],
    ) -> GratResult<()> {
        put_if_enabled(
            &self.provider,
            self.no_cache,
            &ledger_cache_key(ledger_sequence, key),
            &entry.to_vec(),
        )
        .await
    }

    pub async fn get(&self, ledger_sequence: u32, key: &str) -> GratResult<Option<Vec<u8>>> {
        self.get_entry(ledger_sequence, key).await
    }

    pub async fn put(&self, ledger_sequence: u32, key: &str, entry: &[u8]) -> GratResult<()> {
        self.store_entry(ledger_sequence, key, entry).await
    }

    pub async fn remove_entry(&self, ledger_sequence: u32, key: &str) -> GratResult<()> {
        if self.no_cache {
            return Ok(());
        }
        self.provider
            .remove(&ledger_cache_key(ledger_sequence, key))
            .await
    }

    pub async fn clear(&self) -> GratResult<()> {
        if self.no_cache {
            return Ok(());
        }
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
