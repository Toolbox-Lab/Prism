use anyhow::{anyhow, Result};
use serde_json::Value;
use std::str::FromStr;
use stellar_xdr::curr::{
    ContractExecutable, Hash, Int128Parts, ScAddress, ScBytes, ScContractInstance, ScError,
    ScErrorCode, ScMap, ScMapEntry, ScNonceKey, ScString, ScVal, ScVec, StringM, UInt128Parts,
};

const MAX_SCVAL_DEPTH: usize = 100;

/// Parses a `serde_json::Value` back into an `ScVal`.
pub fn json_to_scval(value: &Value) -> Result<ScVal> {
    convert(value, 0)
}

fn convert(value: &Value, depth: usize) -> Result<ScVal> {
    if depth > MAX_SCVAL_DEPTH {
        return Err(anyhow!(
            "max recursion depth ({}) exceeded",
            MAX_SCVAL_DEPTH
        ));
    }

    match value {
        Value::Null => Ok(ScVal::Void),
        Value::Bool(b) => Ok(ScVal::Bool(*b)),
        Value::Number(num) => {
            if let Some(i) = num.as_i64() {
                if i >= 0 && i <= u32::MAX as i64 {
                    Ok(ScVal::U32(i as u32))
                } else if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    Ok(ScVal::I32(i as i32))
                } else if i < 0 {
                    Ok(ScVal::I64(i))
                } else {
                    Ok(ScVal::U64(i as u64))
                }
            } else if let Some(u) = num.as_u64() {
                if u <= u32::MAX as u64 {
                    Ok(ScVal::U32(u as u32))
                } else {
                    Ok(ScVal::U64(u))
                }
            } else {
                Err(anyhow!(
                    "unsupported float or out of bounds number: {}",
                    num
                ))
            }
        }
        Value::String(s) => convert_string(s),
        Value::Array(arr) => {
            // Check if this is the lossless map fallback: [{"key": ..., "value": ...}, ...]
            let is_lossless_map = !arr.is_empty()
                && arr.iter().all(|v| {
                    v.as_object().map_or(false, |obj| {
                        obj.len() == 2 && obj.contains_key("key") && obj.contains_key("value")
                    })
                });

            if is_lossless_map {
                let mut entries = Vec::with_capacity(arr.len());
                for item in arr {
                    let obj = item.as_object().unwrap();
                    let key = convert(obj.get("key").unwrap(), depth + 1)?;
                    let val = convert(obj.get("value").unwrap(), depth + 1)?;
                    entries.push(ScMapEntry { key, val });
                }
                return Ok(ScVal::Map(Some(ScMap(
                    entries.try_into().map_err(|_| anyhow!("map too large"))?,
                ))));
            }

            // Normal Array
            let mut items = Vec::with_capacity(arr.len());
            for item in arr {
                items.push(convert(item, depth + 1)?);
            }
            Ok(ScVal::Vec(Some(ScVec(
                items.try_into().map_err(|_| anyhow!("array too large"))?,
            ))))
        }
        Value::Object(obj) => {
            if obj.contains_key("__truncated__") {
                return Err(anyhow!("cannot convert truncated JSON back to ScVal"));
            }

            // Check for explicit markers
            if obj.contains_key("type") && obj.contains_key("code") && obj.len() == 2 {
                if let Ok(err) = parse_error(obj) {
                    return Ok(ScVal::Error(err));
                }
            }

            if obj.contains_key("nonce") && obj.len() == 1 {
                if let Some(nonce_val) = obj.get("nonce").and_then(|v| v.as_i64()) {
                    return Ok(ScVal::LedgerKeyNonce(ScNonceKey { nonce: nonce_val }));
                }
            }

            if obj.contains_key("executable") && obj.contains_key("storage") {
                if let Ok(instance) = parse_contract_instance(obj, depth) {
                    return Ok(ScVal::ContractInstance(instance));
                }
            }

            // Normal Map with stringified keys
            let mut entries = Vec::with_capacity(obj.len());
            for (k, v) in obj {
                let key_val = convert_string(k)?;
                let val_val = convert(v, depth + 1)?;
                entries.push(ScMapEntry {
                    key: key_val,
                    val: val_val,
                });
            }
            Ok(ScVal::Map(Some(ScMap(
                entries.try_into().map_err(|_| anyhow!("map too large"))?,
            ))))
        }
    }
}

fn convert_string(s: &str) -> Result<ScVal> {
    if s == "LedgerKeyContractInstance" {
        return Ok(ScVal::LedgerKeyContractInstance);
    }
    if let Some(hex) = s.strip_prefix("0x") {
        if let Ok(bytes) = hex::decode(hex) {
            return Ok(ScVal::Bytes(ScBytes(
                bytes.try_into().map_err(|_| anyhow!("bytes too large"))?,
            )));
        }
    }
    if s.starts_with('G') && s.len() == 56 {
        if let Ok(pubkey) = stellar_strkey::ed25519::PublicKey::from_string(s) {
            return Ok(ScVal::Address(ScAddress::Account(
                stellar_xdr::curr::AccountId(stellar_xdr::curr::PublicKey::PublicKeyTypeEd25519(
                    stellar_xdr::curr::Uint256(pubkey.0),
                )),
            )));
        }
    }
    if s.starts_with('C') && s.len() == 56 {
        if let Ok(contract) = stellar_strkey::Contract::from_string(s) {
            return Ok(ScVal::Address(ScAddress::Contract(Hash(contract.0))));
        }
    }

    // Try parsing large numbers (u128, i128). We skip 256-bit parsing here as a simplification,
    // falling back to String if they exceed u128.
    if let Ok(u) = u128::from_str(s) {
        if u > u64::MAX as u128 {
            let hi = (u >> 64) as u64;
            let lo = u as u64;
            return Ok(ScVal::U128(UInt128Parts { hi, lo }));
        }
    }
    if let Ok(i) = i128::from_str(s) {
        if i < i64::MIN as i128 || i > i64::MAX as i128 {
            let hi = (i >> 64) as i64;
            let lo = (i & 0xFFFFFFFFFFFFFFFF) as u64;
            return Ok(ScVal::I128(Int128Parts { hi, lo }));
        }
    }

    // Default to String. (Could be Symbol, but we can't infer that purely from a JSON string).
    Ok(ScVal::String(ScString(
        StringM::try_from(s.as_bytes().to_vec()).map_err(|_| anyhow!("string too long"))?,
    )))
}

fn parse_error(obj: &serde_json::Map<String, Value>) -> Result<ScError> {
    let err_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let code_val = obj.get("code").unwrap();

    let parse_code = |c: &Value| -> Result<ScErrorCode> {
        let code_str = c.as_str().ok_or_else(|| anyhow!("code must be string"))?;
        match code_str {
            "UnexpectedType" => Ok(ScErrorCode::UnexpectedType),
            "UnexpectedSize" => Ok(ScErrorCode::UnexpectedSize),
            "MissingValue" => Ok(ScErrorCode::MissingValue),
            "InternalError" => Ok(ScErrorCode::InternalError),
            "ExceededLimit" => Ok(ScErrorCode::ExceededLimit),
            "InvalidAction" => Ok(ScErrorCode::InvalidAction),
            "InvalidInput" => Ok(ScErrorCode::InvalidInput),
            _ => Err(anyhow!("unknown error code variant {}", code_str)),
        }
    };

    match err_type {
        "Contract" => {
            let code = code_val
                .as_u64()
                .ok_or_else(|| anyhow!("Contract code must be u32"))? as u32;
            Ok(ScError::Contract(code))
        }
        "WasmVm" => Ok(ScError::WasmVm(parse_code(code_val)?)),
        "Context" => Ok(ScError::Context(parse_code(code_val)?)),
        "Storage" => Ok(ScError::Storage(parse_code(code_val)?)),
        "Object" => Ok(ScError::Object(parse_code(code_val)?)),
        "Crypto" => Ok(ScError::Crypto(parse_code(code_val)?)),
        "Events" => Ok(ScError::Events(parse_code(code_val)?)),
        "Budget" => Ok(ScError::Budget(parse_code(code_val)?)),
        "Value" => Ok(ScError::Value(parse_code(code_val)?)),
        "Auth" => Ok(ScError::Auth(parse_code(code_val)?)),
        _ => Err(anyhow!("Unknown ScError type: {}", err_type)),
    }
}

fn parse_contract_instance(
    obj: &serde_json::Map<String, Value>,
    depth: usize,
) -> Result<ScContractInstance> {
    let exec_obj = obj
        .get("executable")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow!("executable must be object"))?;

    let exec_type = exec_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let executable = if exec_type == "Wasm" {
        let hash_str = exec_obj
            .get("wasmHash")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let hex = hash_str.strip_prefix("0x").unwrap_or(hash_str);
        let bytes = hex::decode(hex)?;
        ContractExecutable::Wasm(Hash(
            bytes.try_into().map_err(|_| anyhow!("invalid hash size"))?,
        ))
    } else if exec_type == "StellarAsset" {
        ContractExecutable::StellarAsset
    } else {
        return Err(anyhow!("Unknown executable type"));
    };

    let storage_val = obj.get("storage").unwrap();
    let storage = if storage_val.is_null() {
        None
    } else {
        match convert(storage_val, depth + 1)? {
            ScVal::Map(Some(m)) => Some(m),
            _ => return Err(anyhow!("storage must map to ScMap")),
        }
    };

    Ok(ScContractInstance {
        executable,
        storage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::scval_to_json;

    #[test]
    fn test_roundtrip_primitives() {
        let cases = vec![
            ScVal::Void,
            ScVal::Bool(true),
            ScVal::Bool(false),
            ScVal::U32(42),
            ScVal::I32(-7),
            ScVal::U64(u64::MAX),
            ScVal::I64(i64::MIN),
            ScVal::String(ScString(StringM::try_from(b"hello".to_vec()).unwrap())),
            ScVal::Bytes(ScBytes(vec![0xDE, 0xAD, 0xBE, 0xEF].try_into().unwrap())),
        ];

        for case in cases {
            let json = scval_to_json(&case);
            let parsed = json_to_scval(&json).expect("should parse");

            // For strings, scval_to_json might have been passed a Symbol, but json_to_scval will parse it back as String.
            // Since we pass ScVal::String above, it matches exactly.
            assert_eq!(parsed, case, "failed on {:?}", case);
        }
    }

    #[test]
    fn test_lossless_map_roundtrip() {
        // ScVal::U32(7) and ScVal::String("7") both stringify to the JSON key "7".
        let map = ScVal::Map(Some(ScMap(
            vec![
                ScMapEntry {
                    key: ScVal::U32(7),
                    val: ScVal::String(ScString(
                        StringM::try_from(b"from_number".to_vec()).unwrap(),
                    )),
                },
                ScMapEntry {
                    key: ScVal::String(ScString(StringM::try_from(b"7".to_vec()).unwrap())),
                    val: ScVal::String(ScString(
                        StringM::try_from(b"from_string".to_vec()).unwrap(),
                    )),
                },
            ]
            .try_into()
            .unwrap(),
        )));

        let json = scval_to_json(&map);
        // This should be the fallback array mode
        assert!(json.is_array());

        let parsed = json_to_scval(&json).unwrap();
        assert_eq!(parsed, map);
    }
}
