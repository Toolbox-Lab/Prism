//! Extraction of user supplied Soroban contract metadata.

use crate::error::{GratError, GratResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use stellar_xdr::curr::{Limited, Limits, ReadXdr, ScSymbol, ScVal};

const METADATA_SECTION: &str = "contractmetav0";
const METADATA_XDR_MAX_LEN: usize = 1024 * 1024;

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
        let start = limited.inner().position();
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
        if limited.inner().position() <= start {
            break;
        }
        metadata.entries.insert(key, value);
    }
    Ok(metadata)
}
