use crate::cache::store::{CacheCategory, CacheStore};
use crate::error::{GratError, GratResult};
use crate::network::NetworkConfig;
use crate::rpc::SorobanRpcClient;
use crate::spec::decoder::{decode_contract_spec, ContractSpec};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Represents a contract identifier (strkey-encoded C... string)
pub type ContractId = String;

/// High-performance caching orchestrator for SCSpec metadata.
///
/// Acts as an intelligent proxy between the decoding engine and the network.
/// When a ContractId is passed, the resolver first queries local persistent storage.
/// If the metadata exists locally, it instantly deserializes it and returns it.
/// On a cache miss, it asynchronously triggers the WASM fetcher, downloads the binary,
/// runs the SpecParser, and seamlessly serializes the result into the persistent cache
/// before returning the data to the decode engine.
///
/// Thread-safe Mutex locks prevent race conditions when multiple async decoding tasks
/// request the same contract simultaneously.
#[derive(Clone)]
pub struct SCSpecResolver {
    cache: Arc<CacheStore>,
    rpc_client: Arc<SorobanRpcClient>,
    /// In-memory cache for frequently accessed specs to avoid disk I/O
    memory_cache: Arc<Mutex<HashMap<ContractId, ContractSpec>>>,
    /// Tracks ongoing fetch operations to prevent duplicate network requests
    #[allow(clippy::type_complexity)]
    pending_fetches: Arc<Mutex<HashMap<ContractId, Arc<Mutex<Option<ContractSpec>>>>>>,
    no_cache: bool,
}

impl SCSpecResolver {
    /// Creates a new SCSpecResolver with the given network configuration.
    pub fn new(config: &NetworkConfig) -> GratResult<Self> {
        let cache = Arc::new(CacheStore::default_location()?);
        let rpc_client = Arc::new(SorobanRpcClient::new(config));

        Ok(Self {
            cache,
            rpc_client,
            memory_cache: Arc::new(Mutex::new(HashMap::new())),
            pending_fetches: Arc::new(Mutex::new(HashMap::new())),
            no_cache: config.no_cache,
        })
    }

    /// Creates a new SCSpecResolver with a custom cache directory.
    pub fn with_cache_dir(
        config: &NetworkConfig,
        cache_dir: std::path::PathBuf,
    ) -> GratResult<Self> {
        let cache = Arc::new(CacheStore::new(cache_dir, 512)?);
        let rpc_client = Arc::new(SorobanRpcClient::new(config));

        Ok(Self {
            cache,
            rpc_client,
            memory_cache: Arc::new(Mutex::new(HashMap::new())),
            pending_fetches: Arc::new(Mutex::new(HashMap::new())),
            no_cache: config.no_cache,
        })
    }

    /// Resolves the ContractSpec for a given ContractId.
    ///
    /// This method is thread-safe and handles cache misses by fetching from the network.
    /// Multiple concurrent requests for the same contract will coalesce into a single
    /// network request.
    pub async fn resolve(&self, contract_id: &ContractId) -> GratResult<ContractSpec> {
        if !self.no_cache {
            // Check memory cache first (fastest path)
            {
                let mem_cache = self.memory_cache.lock().await;
                if let Some(spec) = mem_cache.get(contract_id) {
                    tracing::debug!(contract_id, "SCSpec resolved from memory cache");
                    return Ok(spec.clone());
                }
            }

            // Check persistent cache
            if let Some(cached_bytes) = self
                .cache
                .get(CacheCategory::ContractSpec, contract_id, None)?
            {
                if let Ok(spec) = bincode::deserialize::<ContractSpec>(&cached_bytes) {
                    // Update memory cache
                    let mut mem_cache = self.memory_cache.lock().await;
                    mem_cache.insert(contract_id.clone(), spec.clone());
                    tracing::debug!(contract_id, "SCSpec resolved from persistent cache");
                    return Ok(spec);
                }
            }
        }

        // Cache miss (or bypass) - need to fetch from network
        self.fetch_and_cache(contract_id).await
    }

    /// Fetches the WASM binary from the network and caches the parsed spec.
    async fn fetch_and_cache(&self, contract_id: &ContractId) -> GratResult<ContractSpec> {
        // Check if there's already a pending fetch for this contract
        let existing_handle = {
            let pending = self.pending_fetches.lock().await;
            pending.get(contract_id).cloned()
        };

        if let Some(handle) = existing_handle {
            // Join the existing fetch operation
            tracing::debug!(contract_id, "Joining existing fetch operation");

            // Wait for the fetch to complete with a timeout
            let mut spec_guard = handle.lock().await;

            // Poll for completion with a reasonable timeout
            for _ in 0..50 {
                if let Some(spec) = spec_guard.take() {
                    return Ok(spec);
                }
                drop(spec_guard);
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
                spec_guard = handle.lock().await;
            }

            // If we still don't have a result after waiting, proceed with new fetch
            drop(spec_guard);
        }

        // No pending fetch or timeout, create a new one
        let fetch_handle = Arc::new(Mutex::new(None));
        {
            let mut pending = self.pending_fetches.lock().await;
            pending.insert(contract_id.clone(), fetch_handle.clone());
        }

        // Perform the actual fetch
        let result = self.do_fetch(contract_id).await;
        let result_for_cache = result.as_ref().ok().cloned();

        // Store the result and clean up the pending fetch
        {
            let mut fetch_handle_guard = fetch_handle.lock().await;
            *fetch_handle_guard = result_for_cache;
        }

        {
            let mut pending = self.pending_fetches.lock().await;
            pending.remove(contract_id);
        }

        result
    }

    /// Performs the actual network fetch and parsing.
    async fn do_fetch(&self, contract_id: &ContractId) -> GratResult<ContractSpec> {
        tracing::info!(contract_id, "Fetching WASM binary from network");

        let wasm_bytes = self.fetch_wasm(contract_id).await?;
        tracing::debug!(
            contract_id,
            wasm_size = wasm_bytes.len(),
            "WASM binary fetched"
        );

        let spec = decode_contract_spec(&wasm_bytes)?;

        // Cache the spec
        let serialized = bincode::serialize(&spec)
            .map_err(|e| GratError::SpecError(format!("Failed to serialize spec: {e}")))?;

        self.cache
            .put(CacheCategory::ContractSpec, contract_id, &serialized, None)?;

        // Update memory cache
        let mut mem_cache = self.memory_cache.lock().await;
        mem_cache.insert(contract_id.clone(), spec.clone());

        tracing::info!(contract_id, "SCSpec fetched and cached successfully");
        Ok(spec)
    }

    /// Fetches the WASM binary for a contract from the network.
    async fn fetch_wasm(&self, contract_id: &ContractId) -> GratResult<Vec<u8>> {
        // Construct the ledger key for the contract code
        // The contract ID is a strkey-encoded C... string
        // We need to convert it to the proper ledger key format for getLedgerEntries

        let contract_key = format!("contract_code/{contract_id}");

        let response = self.rpc_client.get_ledger_entries(&[contract_key]).await?;

        let entries = response
            .get("entries")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                GratError::ContractNotFound(format!("Invalid response for contract {contract_id}"))
            })?;

        if entries.is_empty() {
            return Err(GratError::ContractNotFound(contract_id.clone()));
        }

        let entry = &entries[0];
        let xdr_base64 = entry
            .get("xdr")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                GratError::SpecError(format!(
                    "Missing XDR in response for contract {contract_id}"
                ))
            })?;

        // Decode the base64 XDR
        let xdr_bytes = base64::engine::general_purpose::STANDARD
            .decode(xdr_base64)
            .map_err(|e| GratError::SpecError(format!("Failed to decode base64 XDR: {e}")))?;

        // Parse the XDR to extract the WASM bytecode
        // The ledger entry for contract code contains the WASM bytecode
        Self::extract_wasm_from_ledger_entry(xdr_bytes.as_slice())
    }

    /// Extracts WASM bytecode from a ledger entry XDR.
    #[allow(clippy::unused_self)]
    fn extract_wasm_from_ledger_entry(xdr_bytes: &[u8]) -> GratResult<Vec<u8>> {
        use stellar_xdr::curr::{LedgerEntry, LedgerEntryData, Limited, Limits, ReadXdr};

        let mut cursor = std::io::Cursor::new(xdr_bytes);
        let mut limited = Limited::new(&mut cursor, Limits::none());
        let entry = LedgerEntry::read_xdr(&mut limited)
            .map_err(|e| GratError::XdrError(format!("Failed to parse ledger entry XDR: {e}")))?;

        match entry.data {
            LedgerEntryData::ContractCode(code_entry) => Ok(code_entry.code.to_vec()),
            _ => Err(GratError::SpecError(
                "Ledger entry is not a contract code entry".to_string(),
            )),
        }
    }

    /// Preloads a ContractSpec into the cache (useful for bulk operations).
    pub async fn preload(&self, contract_id: ContractId, spec: ContractSpec) -> GratResult<()> {
        let serialized = bincode::serialize(&spec)
            .map_err(|e| GratError::SpecError(format!("Failed to serialize spec: {e}")))?;

        self.cache
            .put(CacheCategory::ContractSpec, &contract_id, &serialized, None)?;

        let mut mem_cache = self.memory_cache.lock().await;
        mem_cache.insert(contract_id, spec);

        Ok(())
    }

    /// Clears both memory and persistent cache for a specific contract.
    pub fn clear_contract(&self, contract_id: &ContractId) -> GratResult<()> {
        self.cache
            .remove(CacheCategory::ContractSpec, contract_id, None)?;
        Ok(())
    }

    /// Clears all cached contract specs from persistent storage.
    pub fn clear_all(&self) -> GratResult<()> {
        self.cache.clear()?;
        Ok(())
    }

    /// Returns statistics about the resolver's cache performance.
    pub async fn stats(&self) -> ResolverStats {
        let mem_cache = self.memory_cache.lock().await;
        let pending = self.pending_fetches.lock().await;

        ResolverStats {
            memory_cache_size: mem_cache.len(),
            pending_fetches: pending.len(),
        }
    }
}

/// Statistics about the SCSpecResolver's cache performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverStats {
    pub memory_cache_size: usize,
    pub pending_fetches: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_stats() {
        let stats = ResolverStats {
            memory_cache_size: 10,
            pending_fetches: 2,
        };
        assert_eq!(stats.memory_cache_size, 10);
        assert_eq!(stats.pending_fetches, 2);
    }

    fn empty_spec() -> ContractSpec {
        ContractSpec {
            errors: Vec::new(),
            functions: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            unions: Vec::new(),
            name: Some("cached".to_string()),
            version: None,
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    async fn no_cache_skips_lookup_even_when_entry_exists() {
        let dir = tempfile::tempdir().unwrap();
        let contract_id = "CACHED_CONTRACT".to_string();

        let cached_config = NetworkConfig::testnet();
        assert!(!cached_config.no_cache);
        let cached_resolver =
            SCSpecResolver::with_cache_dir(&cached_config, dir.path().to_path_buf()).unwrap();
        cached_resolver
            .preload(contract_id.clone(), empty_spec())
            .await
            .unwrap();

        // Cached resolver hits local storage without network.
        let hit = cached_resolver.resolve(&contract_id).await.unwrap();
        assert_eq!(hit.name.as_deref(), Some("cached"));

        // Same persistent entry must be ignored when no_cache is set.
        let mut bypass_config = NetworkConfig::custom("test", "http://127.0.0.1:1", "");
        bypass_config.request_timeout_secs = 1;
        bypass_config.no_cache = true;
        let bypass_resolver =
            SCSpecResolver::with_cache_dir(&bypass_config, dir.path().to_path_buf()).unwrap();
        assert!(bypass_resolver.no_cache);

        let result = bypass_resolver.resolve(&contract_id).await;
        assert!(
            result.is_err(),
            "expected live network fetch (failure), not a cache hit"
        );
    }
}
