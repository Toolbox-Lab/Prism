//! Contract-specific error resolver.
//!
//! Fetches WASM bytecode from the ledger, parses contractspecv0 metadata,
//! and maps numeric error codes to named enum variants.

use crate::spec::decoder;
use crate::types::config::NetworkConfig;
use crate::types::error::{PrismError, PrismResult};
use crate::types::report::ContractErrorInfo;

/// Resolve a contract-specific error code to its named variant.
///
/// # Process
/// 1. Fetch the contract's WASM bytecode from the ledger
/// 2. Parse the `contractspecv0` metadata from WASM custom sections
/// 3. Find the error enum definition  
/// 4. Map the numeric code to the variant name and doc comment
pub async fn resolve(
    contract_id: &str,
    error_code: u32,
    network: &NetworkConfig,
) -> PrismResult<ContractErrorInfo> {
    // 1. Check cache first
    let cache = crate::cache::store::CacheStore::default_location()?;
    let cache_key = format!("{contract_id}_spec");

    // 2. Fetch WASM if not cached
    let wasm_bytes = if let Some(cached) =
        cache.get(crate::cache::store::CacheCategory::WasmBlob, &cache_key)?
    {
        cached
    } else {
        let wasm = fetch_contract_wasm(contract_id, network).await?;
        let _ = cache.put(
            crate::cache::store::CacheCategory::WasmBlob,
            &cache_key,
            &wasm,
        );
        wasm
    };

    // 3. Parse contract spec
    let spec = decoder::decode_contract_spec(&wasm_bytes)?;

    // 4. Resolve the error code
    let error_entry = decoder::resolve_error_code(&spec, error_code);

    Ok(ContractErrorInfo {
        contract_id: contract_id.to_string(),
        error_code,
        error_name: error_entry.map(|e| e.name.clone()),
        doc_comment: error_entry.and_then(|e| e.doc.clone()),
    })
}

/// Fetch a contract's WASM bytecode from the Soroban RPC.
async fn fetch_contract_wasm(contract_id: &str, network: &NetworkConfig) -> PrismResult<Vec<u8>> {
    let rpc = crate::network::rpc::RpcClient::new(network.clone());

    // TODO: Build the contract code ledger key and fetch via getLedgerEntries
    let _result = rpc.get_ledger_entries(&[contract_id.to_string()]).await?;

    // TODO: Extract WASM bytes from the ledger entry
    Err(PrismError::ContractNotFound(format!(
        "WASM fetch not yet implemented for {contract_id}"
    )))
}
