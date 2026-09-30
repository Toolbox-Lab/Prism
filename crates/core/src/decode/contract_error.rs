use crate::error::{PrismError, PrismResult};
use crate::spec::decoder;
use crate::types::address::Address;
use crate::types::config::NetworkConfig;
use crate::types::report::ContractErrorInfo;

pub async fn resolve(
    contract_id: &str,
    error_code: u32,
    ctx: &crate::decode::decode_context::DecodeContext,
) -> PrismResult<ContractErrorInfo> {
    resolve_with_network(contract_id, error_code, &ctx.network).await
}

async fn resolve_with_network(
    contract_id: &str,
    error_code: u32,
    network: &NetworkConfig,
) -> PrismResult<ContractErrorInfo> {
    Address::validate_contract_id(contract_id)?;

    let cache = crate::cache::store::CacheStore::default_location()?;
    let cache_key = format!("{contract_id}_spec");

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

    let spec = decoder::decode_contract_spec(&wasm_bytes)?;

    let error_entry = decoder::resolve_error_code(&spec, error_code);

    Ok(ContractErrorInfo {
        contract_id: contract_id.to_string(),
        error_code,
        error_name: error_entry.map(|e| e.name.clone()),
        doc_comment: error_entry.and_then(|e| e.doc.clone()),
    })
}

async fn fetch_contract_wasm(contract_id: &str, network: &NetworkConfig) -> PrismResult<Vec<u8>> {
    let rpc = crate::rpc::SorobanRpcClient::new(network);

    let _result = rpc.get_ledger_entries(&[contract_id.to_string()]).await?;

    Err(PrismError::ContractNotFound(format!(
        "WASM fetch not yet implemented for {contract_id}"
    )))
}
