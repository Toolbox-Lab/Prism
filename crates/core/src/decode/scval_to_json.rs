//! Recursive `SCVal` -> `serde_json::Value` conversion.
//!
//! Soroban passes every argument, storage entry, and return value as an
//! [`ScVal`]: a recursively nested XDR union that can represent anything from
//! a bare `u32` to a `Map` of `Vec`s of binary blobs. Frontend and Web3
//! tooling cannot reasonably consume raw XDR or Rust `Debug` dumps, so this
//! module renders any `ScVal` into plain JSON.

use crate::decode::auth::scaddress_to_strkey;
use crate::decode::return_decoder::ReturnValueDecoder;
use crate::decode::struct_decoder::StructDecoder;
use crate::spec::decoder::ContractSpec;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use stellar_xdr::curr::{
    ContractExecutable, Int128Parts, Int256Parts, ScContractInstance, ScError, ScErrorCode, ScMap,
    ScSpecTypeDef, ScVal, UInt128Parts, UInt256Parts,
};

/// Maximum `ScVal` nesting depth the converter will descend into.
///
/// `ScVal::Vec`/`ScVal::Map` are recursive, so a hostile or corrupt
/// transaction can encode a value nested thousands of layers deep. XDR
/// decoding in this crate does not itself cap nesting depth, so without this
/// guard a single malicious payload could blow the call stack while being
/// converted for display. 100 layers comfortably covers any legitimate
/// contract state while staying far short of a stack overflow.
const MAX_SCVAL_DEPTH: usize = 100;

/// Convert an [`ScVal`] into a readable [`serde_json::Value`].
///
/// The conversion never fails and never panics: unsupported combinations
/// simply degrade to a best-effort JSON representation, and nesting beyond
/// [`MAX_SCVAL_DEPTH`] is truncated in place rather than recursed into.
pub fn scval_to_json(val: &ScVal) -> Value {
    convert(val, 0)
}

/// Convert an [`ScVal`] into JSON using a contract type definition.
///
/// Unlike [`scval_to_json`], this entry point can resolve UDTs, including
/// simple integer enums and complex symbol-headed enum values, when the
/// corresponding definitions are present in `contract_spec`.
pub fn scval_to_json_with_spec(
    val: &ScVal,
    type_def: Option<&ScSpecTypeDef>,
    contract_spec: Option<&ContractSpec>,
) -> Value {
    ReturnValueDecoder::new().decode(val, type_def, contract_spec)
}

/// Convert an [`ScVal`] into JSON, resolving struct values via layout lookup.
///
/// [`scval_to_json`] has no type information, so every `ScVal::Map` is rendered
/// as a generic dictionary and the declared field names, order and types of a
/// `#[contracttype] struct` are all lost. This entry point intercepts
/// `ScVal::Map` and asks [`StructDecoder`] which struct in `contract_spec` the
/// keys match, rendering the value as that struct.
///
/// A map that does not match a declared struct is rendered exactly as
/// [`scval_to_json`] would render it, so a plain `Map` value, a
/// `ContractInstance`'s storage, or a payload from a newer contract than the
/// spec describes all keep their current output.
pub fn scval_to_json_with_contract_spec(val: &ScVal, contract_spec: &ContractSpec) -> Value {
    convert_with_spec(val, 0, contract_spec)
}

fn convert_with_spec(val: &ScVal, depth: usize, contract_spec: &ContractSpec) -> Value {
    if depth > MAX_SCVAL_DEPTH {
        return depth_exceeded_marker();
    }

    // Intercept maps before the generic renderer sees them, so a struct value
    // is decoded with its field types and the rest is untouched.
    if let ScVal::Map(Some(entries)) = val {
        let decoder = StructDecoder::new();
        if let Some(struct_def) = decoder.lookup(entries, contract_spec) {
            return decoder.decode(val, struct_def, contract_spec);
        }
    }

    match val {
        ScVal::Vec(Some(items)) => Value::Array(
            items
                .iter()
                .map(|item| convert_with_spec(item, depth + 1, contract_spec))
                .collect(),
        ),
        ScVal::Map(Some(entries)) => {
            scmap_to_json_with_spec(entries, depth + 1, Some(contract_spec))
        }
        ScVal::ContractInstance(instance) => {
            contract_instance_to_json_with_spec(instance, depth, Some(contract_spec))
        }
        other => convert(other, depth),
    }
}

fn depth_exceeded_marker() -> Value {
    json!({
        "__truncated__": true,
        "reason": format!("max recursion depth ({MAX_SCVAL_DEPTH}) exceeded"),
    })
}

#[allow(clippy::too_many_lines)]
fn convert(val: &ScVal, depth: usize) -> Value {
    if depth > MAX_SCVAL_DEPTH {
        return depth_exceeded_marker();
    }

    match val {
        ScVal::Bool(b) => json!(b),
        ScVal::Void | ScVal::Vec(None) | ScVal::Map(None) => Value::Null,
        ScVal::Error(err) => scerror_to_json(err),
        ScVal::U32(v) => json!(v),
        ScVal::I32(v) => json!(v),
        ScVal::U64(v) => json!(v),
        ScVal::I64(v) => json!(v),
        ScVal::Timepoint(t) => json!(t.0),
        ScVal::Duration(d) => json!(d.0),
        ScVal::U128(parts) => json!(u128_to_decimal(parts)),
        ScVal::I128(parts) => json!(i128_to_decimal(parts)),
        ScVal::U256(parts) => json!(u256_to_decimal(parts)),
        ScVal::I256(parts) => json!(i256_to_decimal(parts)),
        ScVal::Bytes(bytes) => json!(bytes_to_hex(bytes.as_ref())),
        ScVal::String(s) => json!(String::from_utf8_lossy(s.as_ref()).into_owned()),
        ScVal::Symbol(sym) => json!(String::from_utf8_lossy(sym.as_ref()).into_owned()),
        ScVal::Vec(Some(items)) => {
            Value::Array(items.iter().map(|item| convert(item, depth + 1)).collect())
        }
        ScVal::Map(Some(entries)) => scmap_to_json(entries, depth + 1),
        ScVal::Address(address) => json!(scaddress_to_strkey(address)),
        ScVal::ContractInstance(instance) => contract_instance_to_json(instance, depth),
        ScVal::LedgerKeyContractInstance => json!("LedgerKeyContractInstance"),
        ScVal::LedgerKeyNonce(nonce_key) => json!({ "nonce": nonce_key.nonce }),
    }
}

/// Renders an `ScMap` as JSON, preferring a plain `{key: value}` object.
///
/// `ScVal` keys are arbitrary (numbers, bools, nested containers, ...) but a
/// `serde_json::Value::Object` only accepts string keys, so each key is
/// stringified: plain JSON strings (symbols, strings, addresses, ...) are
/// used as-is, everything else falls back to its compact JSON text (e.g. a
/// `U32(7)` key becomes `"7"`). That stringification is lossy — distinct
/// `ScVal` keys such as `U32(7)` and `String("7")`, or `Bool(true)` and
/// `String("true")`, can collide on the same JSON key. To avoid silently
/// dropping entries on collision, if *any* two keys in the map stringify to
/// the same value, the whole map instead renders as a lossless
/// `[{"key": ..., "value": ...}, ...]` array, where both `key` and `value`
/// are full recursive JSON (not stringified).
fn scmap_to_json(entries: &ScMap, depth: usize) -> Value {
    scmap_to_json_with_spec(entries, depth, None)
}

/// Renders an `ScMap` as JSON, optionally decoding struct payloads.
///
/// See [`scval_to_json_with_contract_spec`] for why a spec-aware map render is
/// useful; a map that matches no declared struct is rendered generically
/// either way.
fn scmap_to_json_with_spec(
    entries: &ScMap,
    depth: usize,
    contract_spec: Option<&ContractSpec>,
) -> Value {
    let mut converted: Vec<(String, Value, Value)> = Vec::with_capacity(entries.len());
    for entry in entries.iter() {
        let key_json = convert(&entry.key, depth);
        let key_string = key_string_from_value(&key_json);
        let value_json = match contract_spec {
            Some(spec) => convert_with_spec(&entry.val, depth, spec),
            None => convert(&entry.val, depth),
        };
        converted.push((key_string, key_json, value_json));
    }

    let mut seen = HashSet::with_capacity(converted.len());
    let has_collision = converted
        .iter()
        .any(|(key, _, _)| !seen.insert(key.clone()));

    if has_collision {
        Value::Array(
            converted
                .into_iter()
                .map(|(_, key, value)| json!({ "key": key, "value": value }))
                .collect(),
        )
    } else {
        let mut obj = Map::with_capacity(converted.len());
        for (key, _, value) in converted {
            obj.insert(key, value);
        }
        Value::Object(obj)
    }
}

fn key_string_from_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn scerror_to_json(err: &ScError) -> Value {
    fn code_str(code: ScErrorCode) -> String {
        format!("{code:?}")
    }

    match err {
        ScError::Contract(code) => json!({ "type": "Contract", "code": code }),
        ScError::WasmVm(code) => json!({ "type": "WasmVm", "code": code_str(*code) }),
        ScError::Context(code) => json!({ "type": "Context", "code": code_str(*code) }),
        ScError::Storage(code) => json!({ "type": "Storage", "code": code_str(*code) }),
        ScError::Object(code) => json!({ "type": "Object", "code": code_str(*code) }),
        ScError::Crypto(code) => json!({ "type": "Crypto", "code": code_str(*code) }),
        ScError::Events(code) => json!({ "type": "Events", "code": code_str(*code) }),
        ScError::Budget(code) => json!({ "type": "Budget", "code": code_str(*code) }),
        ScError::Value(code) => json!({ "type": "Value", "code": code_str(*code) }),
        ScError::Auth(code) => json!({ "type": "Auth", "code": code_str(*code) }),
    }
}

fn contract_instance_to_json(instance: &ScContractInstance, depth: usize) -> Value {
    contract_instance_to_json_with_spec(instance, depth, None)
}

fn contract_instance_to_json_with_spec(
    instance: &ScContractInstance,
    depth: usize,
    contract_spec: Option<&ContractSpec>,
) -> Value {
    let executable = match &instance.executable {
        ContractExecutable::Wasm(hash) => {
            json!({ "type": "Wasm", "wasmHash": bytes_to_hex(&hash.0) })
        }
        ContractExecutable::StellarAsset => json!({ "type": "StellarAsset" }),
    };

    let storage = match &instance.storage {
        Some(entries) => scmap_to_json_with_spec(entries, depth + 1, contract_spec),
        None => Value::Null,
    };

    json!({ "executable": executable, "storage": storage })
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn u128_to_decimal(parts: &UInt128Parts) -> String {
    ((u128::from(parts.hi) << 64) | u128::from(parts.lo)).to_string()
}

fn i128_to_decimal(parts: &Int128Parts) -> String {
    #[allow(clippy::cast_possible_wrap)]
    let lo = u128::from(parts.lo) as i128;
    (((i128::from(parts.hi)) << 64) | lo).to_string()
}

/// Big-endian 256-bit limbs, most-significant first.
type U256Limbs = [u64; 4];

/// Renders an unsigned 256-bit integer (as four big-endian `u64` limbs) to a
/// decimal string using schoolbook long division by `10^18` chunks. There is
/// no native 256-bit integer type in std, and pulling in a bignum dependency
/// for a debug-only formatter isn't worth it, so this hand-rolls the division.
fn u256_limbs_to_decimal(mut limbs: U256Limbs) -> String {
    const CHUNK_DIV: u128 = 1_000_000_000_000_000_000; // 10^18

    if limbs == [0, 0, 0, 0] {
        return "0".to_string();
    }

    let mut chunks: Vec<u64> = Vec::new();
    while limbs != [0, 0, 0, 0] {
        let mut rem: u128 = 0;
        let mut next = [0u64; 4];
        for (i, limb) in limbs.iter().enumerate() {
            let cur = (rem << 64) | u128::from(*limb);
            next[i] = (cur / CHUNK_DIV) as u64;
            rem = cur % CHUNK_DIV;
        }
        chunks.push(rem as u64);
        limbs = next;
    }

    let mut out = String::new();
    for (i, chunk) in chunks.iter().rev().enumerate() {
        if i == 0 {
            out.push_str(&chunk.to_string());
        } else {
            out.push_str(&format!("{chunk:018}"));
        }
    }
    out
}

/// Two's-complement negation of a 256-bit integer stored as big-endian limbs.
fn negate_u256(limbs: U256Limbs) -> U256Limbs {
    let mut out = [0u64; 4];
    let mut carry: u128 = 1;
    for i in (0..4).rev() {
        let sum = u128::from(!limbs[i]) + carry;
        out[i] = sum as u64;
        carry = sum >> 64;
    }
    out
}

fn u256_to_decimal(parts: &UInt256Parts) -> String {
    u256_limbs_to_decimal([parts.hi_hi, parts.hi_lo, parts.lo_hi, parts.lo_lo])
}

fn i256_to_decimal(parts: &Int256Parts) -> String {
    let negative = parts.hi_hi < 0;
    #[allow(clippy::cast_sign_loss)]
    let limbs: U256Limbs = [parts.hi_hi as u64, parts.hi_lo, parts.lo_hi, parts.lo_lo];
    let magnitude_limbs = if negative { negate_u256(limbs) } else { limbs };
    let magnitude = u256_limbs_to_decimal(magnitude_limbs);

    if negative && magnitude != "0" {
        format!("-{magnitude}")
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::decoder::{ContractSpec, ContractStructDef, ContractStructField};
    use stellar_xdr::curr::{
        Duration, Hash, ScAddress, ScBytes, ScErrorCode, ScMap, ScMapEntry, ScNonceKey,
        ScSpecTypeDef, ScString, ScSymbol, ScVec, StringM, TimePoint,
    };

    fn sym(s: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(StringM::try_from(s.as_bytes().to_vec()).unwrap()))
    }

    fn spec_field(name: &str, type_def: ScSpecTypeDef) -> ContractStructField {
        ContractStructField {
            name: name.to_string(),
            type_name: "T".to_string(),
            doc: None,
            type_def: Some(type_def),
        }
    }

    fn spec_with_structs(structs: Vec<ContractStructDef>) -> ContractSpec {
        ContractSpec {
            errors: vec![],
            functions: vec![],
            structs,
            name: None,
            version: None,
            metadata: crate::spec::metadata::ContractMetadata::default(),
            enums: vec![],
            unions: vec![],
        }
    }

    fn counter_spec() -> ContractSpec {
        spec_with_structs(vec![ContractStructDef {
            name: "Counter".to_string(),
            fields: vec![
                spec_field("count", ScSpecTypeDef::U64),
                spec_field("label", ScSpecTypeDef::String),
            ],
            doc: None,
        }])
    }

    fn sym_map(entries: Vec<(&str, ScVal)>) -> ScVal {
        ScVal::Map(Some(
            entries
                .into_iter()
                .map(|(key, val)| ScMapEntry { key: sym(key), val })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        ))
    }

    #[test]
    fn spec_aware_render_resolves_a_map_to_its_declared_struct() {
        // Without a spec this is an anonymous `{count, label}` dictionary; with
        // one, the same bytes render as the declared struct with typed fields.
        let val = sym_map(vec![("label", sym("hits")), ("count", ScVal::U64(3))]);

        assert_eq!(scval_to_json(&val), json!({ "label": "hits", "count": 3 }));
        assert_eq!(
            scval_to_json_with_contract_spec(&val, &counter_spec()),
            json!({ "count": 3, "label": "hits" })
        );
    }

    #[test]
    fn spec_aware_render_leaves_a_non_struct_map_alone() {
        // A genuine `Map` value with non-symbol keys must keep its generic
        // rendering rather than be coerced into some struct.
        let val = ScVal::Map(Some(
            vec![ScMapEntry {
                key: ScVal::U32(1),
                val: ScVal::Bool(true),
            }]
            .try_into()
            .unwrap(),
        ));

        let spec = counter_spec();
        assert_eq!(
            scval_to_json_with_contract_spec(&val, &spec),
            scval_to_json(&val)
        );
    }

    #[test]
    fn spec_aware_render_recurses_into_vecs_and_maps() {
        let inner = sym_map(vec![("count", ScVal::U64(1)), ("label", sym("a"))]);
        let val = ScVal::Vec(Some(ScVec(vec![inner.clone(), inner].try_into().unwrap())));

        assert_eq!(
            scval_to_json_with_contract_spec(&val, &counter_spec()),
            json!([
                { "count": 1, "label": "a" },
                { "count": 1, "label": "a" }
            ])
        );
    }

    #[test]
    fn spec_aware_render_resolves_structs_inside_contract_storage() {
        let instance = ScVal::ContractInstance(ScContractInstance {
            executable: ContractExecutable::StellarAsset,
            storage: Some(
                vec![ScMapEntry {
                    key: sym("state"),
                    val: sym_map(vec![("count", ScVal::U64(9)), ("label", sym("total"))]),
                }]
                .try_into()
                .unwrap(),
            ),
        });

        assert_eq!(
            scval_to_json_with_contract_spec(&instance, &counter_spec())["storage"]["state"],
            json!({ "count": 9, "label": "total" })
        );
    }

    #[test]
    fn spec_aware_render_respects_the_recursion_limit() {
        let mut current = sym_map(vec![("count", ScVal::U64(1)), ("label", sym("deep"))]);
        for _ in 0..(MAX_SCVAL_DEPTH + 50) {
            current = ScVal::Vec(Some(ScVec(vec![current].try_into().unwrap())));
        }

        let result = scval_to_json_with_contract_spec(&current, &counter_spec());

        let mut node = &result;
        for _ in 0..(MAX_SCVAL_DEPTH + 50) {
            if node.get("__truncated__").is_some() {
                return;
            }
            match node.as_array().and_then(|a| a.first()) {
                Some(next) => node = next,
                None => break,
            }
        }
        panic!("expected truncation marker in nested spec-aware output");
    }

    #[test]
    fn primitives_map_directly() {
        assert_eq!(scval_to_json(&ScVal::Bool(true)), json!(true));
        assert_eq!(scval_to_json(&ScVal::Void), Value::Null);
        assert_eq!(scval_to_json(&ScVal::U32(42)), json!(42));
        assert_eq!(scval_to_json(&ScVal::I32(-7)), json!(-7));
        assert_eq!(scval_to_json(&ScVal::U64(u64::MAX)), json!(u64::MAX));
        assert_eq!(scval_to_json(&ScVal::I64(i64::MIN)), json!(i64::MIN));
    }

    #[test]
    fn timepoint_and_duration_unwrap_to_numbers() {
        assert_eq!(scval_to_json(&ScVal::Timepoint(TimePoint(100))), json!(100));
        assert_eq!(scval_to_json(&ScVal::Duration(Duration(50))), json!(50));
    }

    #[test]
    fn symbol_and_string_become_utf8() {
        assert_eq!(scval_to_json(&sym("hello")), json!("hello"));
        assert_eq!(
            scval_to_json(&ScVal::String(ScString(
                StringM::try_from(b"world".to_vec()).unwrap()
            ))),
            json!("world")
        );
    }

    #[test]
    fn bytes_become_hex_string() {
        let bytes = ScVal::Bytes(ScBytes(vec![0xDE, 0xAD, 0xBE, 0xEF].try_into().unwrap()));
        assert_eq!(scval_to_json(&bytes), json!("0xdeadbeef"));
    }

    #[test]
    fn vec_recurses_into_array() {
        let v = ScVal::Vec(Some(ScVec(
            vec![ScVal::U32(1), ScVal::U32(2), sym("three")]
                .try_into()
                .unwrap(),
        )));
        assert_eq!(scval_to_json(&v), json!([1, 2, "three"]));
    }

    #[test]
    fn empty_vec_and_absent_vec_are_distinguishable() {
        assert_eq!(
            scval_to_json(&ScVal::Vec(Some(ScVec(vec![].try_into().unwrap())))),
            json!([])
        );
        assert_eq!(scval_to_json(&ScVal::Vec(None)), Value::Null);
    }

    #[test]
    fn map_recurses_into_object_with_string_keys() {
        let m = ScVal::Map(Some(ScMap(
            vec![
                ScMapEntry {
                    key: sym("name"),
                    val: sym("grat"),
                },
                ScMapEntry {
                    key: ScVal::U32(7),
                    val: ScVal::Bool(true),
                },
            ]
            .try_into()
            .unwrap(),
        )));

        let result = scval_to_json(&m);
        assert_eq!(result["name"], json!("grat"));
        assert_eq!(result["7"], json!(true));
    }

    #[test]
    fn map_with_colliding_stringified_keys_falls_back_to_entries_array() {
        // ScVal::U32(7) and ScVal::String("7") both stringify to the JSON
        // key "7". Silently `obj.insert`-ing both would drop one entry, so
        // the whole map must fall back to a lossless entries array instead.
        let m = ScVal::Map(Some(ScMap(
            vec![
                ScMapEntry {
                    key: ScVal::U32(7),
                    val: ScVal::Symbol(ScSymbol(
                        StringM::try_from(b"from_number".to_vec()).unwrap(),
                    )),
                },
                ScMapEntry {
                    key: ScVal::String(ScString(StringM::try_from(b"7".to_vec()).unwrap())),
                    val: ScVal::Symbol(ScSymbol(
                        StringM::try_from(b"from_string".to_vec()).unwrap(),
                    )),
                },
            ]
            .try_into()
            .unwrap(),
        )));

        let result = scval_to_json(&m);
        let entries = result.as_array().expect("collision falls back to array");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["key"], json!(7));
        assert_eq!(entries[0]["value"], json!("from_number"));
        assert_eq!(entries[1]["key"], json!("7"));
        assert_eq!(entries[1]["value"], json!("from_string"));
    }

    #[test]
    fn non_colliding_map_is_still_rendered_as_object() {
        let m = ScVal::Map(Some(ScMap(
            vec![
                ScMapEntry {
                    key: ScVal::U32(1),
                    val: ScVal::Bool(true),
                },
                ScMapEntry {
                    key: ScVal::U32(2),
                    val: ScVal::Bool(false),
                },
            ]
            .try_into()
            .unwrap(),
        )));

        let result = scval_to_json(&m);
        assert!(result.is_object());
        assert_eq!(result["1"], json!(true));
        assert_eq!(result["2"], json!(false));
    }

    #[test]
    fn address_renders_as_strkey() {
        let contract = ScVal::Address(ScAddress::Contract(Hash([7u8; 32])));
        let rendered = scval_to_json(&contract);
        assert!(rendered.as_str().unwrap().starts_with('C'));
    }

    #[test]
    fn ledger_key_variants_render() {
        assert_eq!(
            scval_to_json(&ScVal::LedgerKeyContractInstance),
            json!("LedgerKeyContractInstance")
        );
        assert_eq!(
            scval_to_json(&ScVal::LedgerKeyNonce(ScNonceKey { nonce: 5 })),
            json!({ "nonce": 5 })
        );
    }

    #[test]
    fn error_variant_renders_type_and_code() {
        let err = ScVal::Error(ScError::Contract(4));
        assert_eq!(
            scval_to_json(&err),
            json!({ "type": "Contract", "code": 4 })
        );

        let host_err = ScVal::Error(ScError::Storage(ScErrorCode::MissingValue));
        assert_eq!(
            scval_to_json(&host_err),
            json!({ "type": "Storage", "code": "MissingValue" })
        );
    }

    #[test]
    fn u128_round_trips_to_decimal_string() {
        let max = ScVal::U128(UInt128Parts {
            hi: u64::MAX,
            lo: u64::MAX,
        });
        assert_eq!(
            scval_to_json(&max),
            json!("340282366920938463463374607431768211455")
        );
    }

    #[test]
    fn i128_round_trips_to_decimal_string() {
        let min = ScVal::I128(Int128Parts {
            hi: i64::MIN,
            lo: 0,
        });
        assert_eq!(
            scval_to_json(&min),
            json!("-170141183460469231731687303715884105728")
        );
    }

    #[test]
    fn u256_max_round_trips_to_decimal_string() {
        let max = ScVal::U256(UInt256Parts {
            hi_hi: u64::MAX,
            hi_lo: u64::MAX,
            lo_hi: u64::MAX,
            lo_lo: u64::MAX,
        });
        assert_eq!(
            scval_to_json(&max),
            json!("115792089237316195423570985008687907853269984665640564039457584007913129639935")
        );
    }

    #[test]
    fn u256_mixed_limbs_round_trip() {
        let v = ScVal::U256(UInt256Parts {
            hi_hi: 1,
            hi_lo: 0,
            lo_hi: 0,
            lo_lo: 1,
        });
        assert_eq!(
            scval_to_json(&v),
            json!("6277101735386680763835789423207666416102355444464034512897")
        );

        let v2 = ScVal::U256(UInt256Parts {
            hi_hi: 0,
            hi_lo: 12345,
            lo_hi: 67890,
            lo_lo: 111,
        });
        assert_eq!(
            scval_to_json(&v2),
            json!("4200785819638985332707708983909320029634671")
        );
    }

    #[test]
    fn i256_min_round_trips_to_decimal_string() {
        let min = ScVal::I256(Int256Parts {
            hi_hi: i64::MIN,
            hi_lo: 0,
            lo_hi: 0,
            lo_lo: 0,
        });
        assert_eq!(
            scval_to_json(&min),
            json!("-57896044618658097711785492504343953926634992332820282019728792003956564819968")
        );
    }

    #[test]
    fn i256_zero_has_no_negative_sign() {
        let zero = ScVal::I256(Int256Parts {
            hi_hi: 0,
            hi_lo: 0,
            lo_hi: 0,
            lo_lo: 0,
        });
        assert_eq!(scval_to_json(&zero), json!("0"));
    }

    #[test]
    fn deeply_nested_vec_does_not_overflow_and_is_truncated() {
        let mut current = ScVal::U32(1);
        for _ in 0..(MAX_SCVAL_DEPTH + 50) {
            current = ScVal::Vec(Some(ScVec(vec![current].try_into().unwrap())));
        }

        let result = scval_to_json(&current);

        // Walk down the array chain until we hit the truncation marker; this
        // must terminate well before we'd run out of stack.
        let mut node = &result;
        let mut found_marker = false;
        for _ in 0..(MAX_SCVAL_DEPTH + 50) {
            if node.get("__truncated__").is_some() {
                found_marker = true;
                break;
            }
            match node.as_array().and_then(|a| a.first()) {
                Some(next) => node = next,
                None => break,
            }
        }
        assert!(found_marker, "expected truncation marker in nested output");
    }

    #[test]
    fn contract_instance_renders_executable_and_storage() {
        let instance = ScVal::ContractInstance(ScContractInstance {
            executable: ContractExecutable::Wasm(Hash([1u8; 32])),
            storage: Some(ScMap(
                vec![ScMapEntry {
                    key: sym("k"),
                    val: ScVal::U32(9),
                }]
                .try_into()
                .unwrap(),
            )),
        });

        let result = scval_to_json(&instance);
        assert_eq!(result["executable"]["type"], json!("Wasm"));
        assert!(result["executable"]["wasmHash"]
            .as_str()
            .unwrap()
            .starts_with("0x"));
        assert_eq!(result["storage"]["k"], json!(9));
    }

    #[test]
    fn contract_instance_storage_collision_falls_back_to_entries_array() {
        let instance = ScVal::ContractInstance(ScContractInstance {
            executable: ContractExecutable::StellarAsset,
            storage: Some(ScMap(
                vec![
                    ScMapEntry {
                        key: ScVal::U32(1),
                        val: ScVal::U32(100),
                    },
                    ScMapEntry {
                        key: ScVal::String(ScString(StringM::try_from(b"1".to_vec()).unwrap())),
                        val: ScVal::U32(200),
                    },
                ]
                .try_into()
                .unwrap(),
            )),
        });

        let result = scval_to_json(&instance);
        let storage = result["storage"]
            .as_array()
            .expect("colliding storage keys fall back to array");
        assert_eq!(storage.len(), 2);
    }
}
