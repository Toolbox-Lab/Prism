use crate::cache::store::{CacheCategory, CacheStore};
use crate::error::{GratError, GratResult};
use crate::rpc::SorobanRpcClient;
use crate::spec::decoder::{decode_contract_spec, resolve_error_code};
use crate::types::address::Address;
use crate::types::config::NetworkConfig;
use crate::xdr::codec::XdrCodec;
use stellar_xdr::curr::{
    ContractDataDurability, ContractExecutable, Hash, LedgerEntry, LedgerEntryData, LedgerKey,
    LedgerKeyContractCode, LedgerKeyContractData, ScAddress, ScVal,
};

pub struct ContractErrorResolver {
    network: NetworkConfig,
}

impl ContractErrorResolver {
    pub fn new(network: NetworkConfig) -> Self {
        Self { network }
    }

    /// Resolves a numeric contract error code to its human-readable name and doc string.
    /// Gracefully falls back to returning the raw integer as a string if resolution fails.
    pub async fn resolve(&self, contract_id: &str, error_code: u32) -> (String, Option<String>) {
        match self.resolve_inner(contract_id, error_code).await {
            Ok((name, doc)) => (name, doc),
            Err(e) => {
                tracing::warn!(
                    contract_id,
                    error_code,
                    error = %e,
                    "Failed to resolve contract error code, falling back to raw integer"
                );
                (error_code.to_string(), None)
            }
        }
    }

    async fn resolve_inner(
        &self,
        contract_id: &str,
        error_code: u32,
    ) -> GratResult<(String, Option<String>)> {
        // Validate Contract ID
        Address::validate_contract_id(contract_id)?;

        // Try local cache first (skipped when no_cache is set)
        let cache = CacheStore::default_location()?;
        let cache_key = format!("{contract_id}_spec");

        let wasm_bytes = if !self.network.no_cache {
            if let Some(cached) = cache.get(CacheCategory::WasmBlob, &cache_key, None)? {
                cached
            } else {
                // Fetch WASM from the network
                let fetched = self.fetch_wasm_from_network(contract_id).await?;
                // Cache it
                let _ = cache.put(CacheCategory::WasmBlob, &cache_key, &fetched, None);
                fetched
            }
        } else {
            // Bypass cache lookup and always fetch live network data
            let fetched = self.fetch_wasm_from_network(contract_id).await?;
            // Still write fresh data for subsequent cached runs
            let _ = cache.put(CacheCategory::WasmBlob, &cache_key, &fetched, None);
            fetched
        };

        // Parse WASM spec
        let spec = decode_contract_spec(&wasm_bytes)?;

        // Resolve error code
        if let Some(entry) = resolve_error_code(&spec, error_code) {
            Ok((entry.name.clone(), entry.doc.clone()))
        } else {
            Err(GratError::SpecError(format!(
                "Error code {error_code} not found in contract {contract_id} spec"
            )))
        }
    }

    async fn fetch_wasm_from_network(&self, contract_id: &str) -> GratResult<Vec<u8>> {
        let rpc_client = SorobanRpcClient::new(&self.network);
        let address = Address::from_contract_id(contract_id)?;

        let mut contract_bytes = [0u8; 32];
        contract_bytes.copy_from_slice(&address.bytes);

        // Step 1: Fetch ContractData LedgerEntry to find the WASM hash
        let instance_key = LedgerKey::ContractData(LedgerKeyContractData {
            contract: ScAddress::Contract(Hash(contract_bytes)),
            key: ScVal::LedgerKeyContractInstance,
            durability: ContractDataDurability::Persistent,
        });

        let base64_key = instance_key.to_xdr_base64()?;
        let response = rpc_client.get_ledger_entries(&[base64_key]).await?;

        let entries = response
            .get("entries")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                GratError::ContractNotFound(format!("Invalid response for contract {contract_id}"))
            })?;

        if entries.is_empty() {
            return Err(GratError::ContractNotFound(contract_id.to_string()));
        }

        let entry_val = &entries[0];
        let xdr_base64 = entry_val
            .get("xdr")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                GratError::SpecError(format!(
                    "Missing XDR in response for contract {contract_id}"
                ))
            })?;

        let ledger_entry = LedgerEntry::from_xdr_base64(xdr_base64)?;

        let wasm_hash = match ledger_entry.data {
            LedgerEntryData::ContractData(contract_data) => match contract_data.val {
                ScVal::ContractInstance(instance) => match instance.executable {
                    ContractExecutable::Wasm(wasm_hash) => wasm_hash,
                    ContractExecutable::StellarAsset => {
                        return Err(GratError::SpecError(
                            "Contract is not a WASM executable".to_string(),
                        ))
                    }
                },
                _ => {
                    return Err(GratError::SpecError(
                        "Ledger entry value is not a ContractInstance".to_string(),
                    ))
                }
            },
            _ => {
                return Err(GratError::SpecError(
                    "Ledger entry is not ContractData".to_string(),
                ))
            }
        };

        // Step 2: Fetch the ContractCode LedgerEntry using wasm_hash
        let code_key = LedgerKey::ContractCode(LedgerKeyContractCode { hash: wasm_hash });

        let base64_code_key = code_key.to_xdr_base64()?;
        let code_response = rpc_client.get_ledger_entries(&[base64_code_key]).await?;

        let code_entries = code_response
            .get("entries")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                GratError::ContractNotFound(format!(
                    "Invalid response for contract code {contract_id}"
                ))
            })?;

        if code_entries.is_empty() {
            return Err(GratError::ContractNotFound(contract_id.to_string()));
        }

        let code_entry_val = &code_entries[0];
        let code_xdr_base64 = code_entry_val
            .get("xdr")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                GratError::SpecError(format!(
                    "Missing XDR in response for contract code {contract_id}"
                ))
            })?;

        let code_ledger_entry = LedgerEntry::from_xdr_base64(code_xdr_base64)?;

        match code_ledger_entry.data {
            LedgerEntryData::ContractCode(code_entry) => Ok(code_entry.code.to_vec()),
            _ => Err(GratError::SpecError(
                "Ledger entry is not a contract code entry".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        Limits, ScSpecEntry, ScSpecUdtErrorEnumCaseV0, ScSpecUdtErrorEnumV0, WriteXdr,
    };

    #[tokio::test]
    async fn test_resolve_from_cache_success() {
        // Construct a mock error enum spec entry
        let error_enum = ScSpecUdtErrorEnumV0 {
            doc: "Error documentation".try_into().unwrap(),
            lib: "".try_into().unwrap(),
            name: "MyError".try_into().unwrap(),
            cases: vec![ScSpecUdtErrorEnumCaseV0 {
                doc: "Some doc".try_into().unwrap(),
                name: "Failed".try_into().unwrap(),
                value: 1042,
            }]
            .try_into()
            .unwrap(),
        };
        let entry = ScSpecEntry::UdtErrorEnumV0(error_enum);
        let entry_bytes = entry.to_xdr(Limits::none()).unwrap();

        // Construct mock WASM payload
        let mut wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        let section_name = "contractspecv0";
        let section_data = entry_bytes;

        let mut custom_payload = Vec::new();
        custom_payload.push(section_name.len() as u8);
        custom_payload.extend_from_slice(section_name.as_bytes());
        custom_payload.extend_from_slice(&section_data);

        wasm.push(0);
        wasm.push(custom_payload.len() as u8);
        wasm.extend(custom_payload);

        let contract_id_str = stellar_strkey::Contract([7u8; 32]).to_string();
        let contract_id = &contract_id_str;
        let cache = CacheStore::default_location().unwrap();
        let cache_key = format!("{contract_id}_spec");
        cache
            .put(CacheCategory::WasmBlob, &cache_key, &wasm, None)
            .unwrap();

        let resolver = ContractErrorResolver::new(NetworkConfig::testnet());
        let (name, doc) = resolver.resolve(contract_id, 1042).await;
        assert_eq!(name, "MyError::Failed");
        assert_eq!(doc, Some("Some doc".to_string()));

        let (name_fb, doc_fb) = resolver.resolve(contract_id, 9999).await;
        assert_eq!(name_fb, "9999");
        assert_eq!(doc_fb, None);

        // Cleanup cache
        let _ = cache.remove(CacheCategory::WasmBlob, &cache_key, None);
    }
}
