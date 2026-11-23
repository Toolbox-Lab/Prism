//! Extraction of user supplied Soroban contract metadata.

use crate::cache::provider::{
    cache_key, get_if_enabled, put_if_enabled, CacheProvider, METADATA_CACHE_PREFIX,
};
use crate::cache::wasm::ContractSpecCache;
use crate::error::{GratError, GratResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use stellar_xdr::curr::{Limited, Limits, ReadXdr, ScSpecEntry, ScSymbol, ScVal};

const METADATA_SECTION: &str = "contractmetav0";
const METADATA_XDR_MAX_LEN: usize = 1024 * 1024;
const SPEC_XDR_MAX_DEPTH: u32 = 256;
const SPEC_XDR_MAX_LEN: usize = 16 * 1024 * 1024;

/// User supplied key/value metadata embedded in a contract WASM module.
///
/// Metadata values are strings in the Soroban contract metadata convention.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractMetadata {
    pub entries: BTreeMap<String, String>,
}

pub(crate) fn is_empty(metadata: &ContractMetadata) -> bool {
    metadata.entries.is_empty()
}

impl ContractMetadata {
    pub fn version(&self) -> Option<String> {
        self.entries.get("version").cloned()
    }
}

/// Find `contractmetav0` and decode its stream of XDR `SCSymbol`/`SCVal` pairs.
pub fn extract_contract_metadata(wasm_bytes: &[u8]) -> GratResult<ContractMetadata> {
    let parser = wasmparser::Parser::new(0);
    let data = parser
        .parse_all(wasm_bytes)
        .filter_map(Result::ok)
        .find_map(|payload| match payload {
            wasmparser::Payload::CustomSection(section) if section.name() == METADATA_SECTION => {
                Some(section.data().to_vec())
            }
            _ => None,
        })
        .ok_or_else(|| {
            GratError::SpecError(format!("{METADATA_SECTION} custom section not found"))
        })?;

    if data.len() > METADATA_XDR_MAX_LEN {
        return Err(GratError::SpecError(
            "contract metadata section exceeds size limit".into(),
        ));
    }

    let cursor = std::io::Cursor::new(data);
    let mut limited = Limited::new(
        cursor,
        Limits {
            depth: 64,
            len: METADATA_XDR_MAX_LEN,
        },
    );
    let mut metadata = ContractMetadata::default();
    loop {
        let start = limited.inner.position();
        let key = match ScSymbol::read_xdr(&mut limited) {
            Ok(key) => key.to_string(),
            Err(_) => break,
        };
        let value = match ScVal::read_xdr(&mut limited) {
            Ok(ScVal::String(value)) => String::from_utf8_lossy(value.as_ref()).into_owned(),
            Ok(ScVal::Symbol(value)) => value.to_string(),
            // Preserve the stream boundary and tolerate future value variants.
            Ok(_) => continue,
            Err(_) => break,
        };
        if limited.inner.position() <= start {
            break;
        }
        metadata.entries.insert(key, value);
    }
    Ok(metadata)
}

/// Decodes the XDR stream of a `contractspecv0` section into spec entries.
pub fn decode_spec_entries(raw_spec: &[u8]) -> GratResult<Vec<ScSpecEntry>> {
    let cursor = std::io::Cursor::new(raw_spec);
    let mut limited = Limited::new(
        cursor,
        Limits {
            depth: SPEC_XDR_MAX_DEPTH,
            len: SPEC_XDR_MAX_LEN,
        },
    );

    let mut entries = Vec::new();
    while let Ok(entry) = ScSpecEntry::read_xdr(&mut limited) {
        entries.push(entry);
    }
    Ok(entries)
}

/// Returns decoded `contractspecv0` entries from the cache when available,
/// otherwise extracts and decodes them from `wasm_bytes` and caches the result.
pub async fn extract_contract_spec_entries<P: CacheProvider>(
    spec_cache: &ContractSpecCache<P>,
    key: &str,
    wasm_bytes: &[u8],
) -> GratResult<Vec<ScSpecEntry>> {
    if let Some(entries) = spec_cache.get_spec(key).await? {
        return Ok(entries);
    }

    let raw_spec = crate::spec::decoder::SpecParser::extract_spec(wasm_bytes).unwrap_or_default();
    let entries = decode_spec_entries(&raw_spec)?;
    spec_cache.store_spec(key, &entries).await?;
    Ok(entries)
}

/// Returns contract metadata from the cache when available, otherwise parses
/// the `contractmetav0` section and caches the decoded result.
pub async fn extract_contract_metadata_cached<P: CacheProvider>(
    provider: &P,
    no_cache: bool,
    key: &str,
    wasm_bytes: &[u8],
) -> GratResult<ContractMetadata> {
    let metadata_key = cache_key(METADATA_CACHE_PREFIX, key);
    if let Some(metadata) =
        get_if_enabled::<P, ContractMetadata>(provider, no_cache, &metadata_key).await?
    {
        return Ok(metadata);
    }

    let metadata = extract_contract_metadata(wasm_bytes)?;
    put_if_enabled(provider, no_cache, &metadata_key, &metadata).await?;
    Ok(metadata)
}
