//! Contract-spec-aware decoding for Soroban enum values.
//!
//! Soroban encodes simple enum values as a discriminant (`u32`) or a symbol,
//! while variants carrying data are represented as a vector whose first item
//! is the variant symbol. This module keeps that wire-format knowledge out of
//! the generic `ScVal` renderer and exposes a small reusable decoder.

use crate::decode::scval_to_json::scval_to_json;
use crate::spec::decoder::ContractEnumDef;
use serde_json::{json, Value};
use stellar_xdr::curr::ScVal;

/// Decodes a contract enum using the variant names from its contract spec.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnumDecoder;

impl EnumDecoder {
    /// Creates an enum decoder.
    pub const fn new() -> Self {
        Self
    }

    /// Decodes an enum value into a human-readable JSON representation.
    ///
    /// Unknown discriminants and variant heads are deliberately preserved via
    /// the normal dynamic renderer instead of being silently discarded. A
    /// payload-free variant is rendered as its name; a variant with one or
    /// more payload values is rendered as `{ "Variant": payload }`.
    pub fn decode(&self, val: &ScVal, enum_def: &ContractEnumDef) -> Value {
        match val {
            ScVal::U32(discriminant) => enum_def
                .cases
                .iter()
                .find(|case| case.value == *discriminant)
                .map_or_else(|| json!(*discriminant), |case| json!(case.name)),
            ScVal::I32(discriminant) if *discriminant >= 0 => {
                #[allow(clippy::cast_sign_loss)]
                let discriminant = *discriminant as u32;
                enum_def
                    .cases
                    .iter()
                    .find(|case| case.value == discriminant)
                    .map_or_else(|| json!(*discriminant), |case| json!(case.name))
            }
            ScVal::Symbol(symbol) => json!(symbol.to_string()),
            ScVal::String(string) => json!(string.to_string()),
            ScVal::Vec(Some(values)) if !values.is_empty() => {
                let Some(variant_name) = variant_name(&values[0]) else {
                    return scval_to_json(val);
                };

                if !enum_def.cases.iter().any(|case| case.name == variant_name) {
                    return scval_to_json(val);
                }

                let payload: Vec<Value> = values[1..].iter().map(scval_to_json).collect();
                match payload.as_slice() {
                    [] => json!(variant_name),
                    [value] => json!({ variant_name: value }),
                    _ => json!({ variant_name: payload }),
                }
            }
            _ => scval_to_json(val),
        }
    }
}

fn variant_name(value: &ScVal) -> Option<String> {
    match value {
        ScVal::Symbol(symbol) => Some(symbol.to_string()),
        ScVal::String(string) => Some(string.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::decoder::{ContractEnumCase, ContractEnumDef};
    use stellar_xdr::curr::{ScSymbol, ScVec};

    fn enum_def() -> ContractEnumDef {
        ContractEnumDef {
            name: "Outcome".to_string(),
            cases: vec![
                ContractEnumCase {
                    name: "Success".to_string(),
                    value: 0,
                    doc: None,
                },
                ContractEnumCase {
                    name: "Error".to_string(),
                    value: 1,
                    doc: None,
                },
            ],
            doc: None,
        }
    }

    fn symbol(value: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(value.try_into().unwrap()))
    }

    #[test]
    fn decodes_integer_discriminants_to_variant_names() {
        let decoder = EnumDecoder::new();
        assert_eq!(
            decoder.decode(&ScVal::U32(0), &enum_def()),
            json!("Success")
        );
        assert_eq!(decoder.decode(&ScVal::I32(1), &enum_def()), json!("Error"));
    }

    #[test]
    fn decodes_symbol_head_and_payload() {
        let value = ScVal::Vec(Some(ScVec(
            vec![symbol("Error"), ScVal::U32(42)].try_into().unwrap(),
        )));
        assert_eq!(
            EnumDecoder::new().decode(&value, &enum_def()),
            json!({ "Error": 42 })
        );
    }

    #[test]
    fn decodes_payload_free_symbol_head() {
        let value = ScVal::Vec(Some(ScVec(vec![symbol("Success")].try_into().unwrap())));
        assert_eq!(
            EnumDecoder::new().decode(&value, &enum_def()),
            json!("Success")
        );
    }

    #[test]
    fn preserves_unknown_discriminants_and_heads() {
        let decoder = EnumDecoder::new();
        assert_eq!(decoder.decode(&ScVal::U32(99), &enum_def()), json!(99));
        let unknown = ScVal::Vec(Some(ScVec(vec![symbol("Other")].try_into().unwrap())));
        assert_eq!(decoder.decode(&unknown, &enum_def()), json!(["Other"]));
    }
}
