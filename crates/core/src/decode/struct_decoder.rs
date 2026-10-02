//! Contract-spec-aware decoding for Soroban struct values.
//!
//! Soroban encodes a `#[contracttype] struct` as an `ScVal::Map` whose keys are
//! `ScSymbol`s naming the Rust fields. Without the contract spec the only
//! sensible reading of such a map is a generic key/value dictionary, which
//! loses the declared field names, the declared field order, and the types of
//! the values. This module resolves a map back to the struct it was built from
//! so the rendered JSON matches the original Rust definition.
//!
//! Two things happen that the generic renderer cannot do:
//!
//! * **Layout lookup.** A caller that holds a bare `ScVal` — a storage entry, a
//!   decoded event payload, a nested value — has no `ScSpecTypeDef` to decode
//!   it with. [`StructDecoder::lookup`] recovers the expected struct by
//!   matching the map's symbol keys against the field names declared in the
//!   `ContractSpec`.
//! * **Shape checking.** A payload that does not match its struct is a real
//!   signal: a field the contract expects is missing, or the value carries a
//!   key the contract never declared (a renamed field, a stale cached struct,
//!   a contract upgraded past the payload). Silently filling gaps with `null`
//!   or dropping unknown keys hides both. [`StructDecoder::decode_checked`]
//!   reports them via [`StructDecodeReport`] instead.

use crate::decode::return_decoder::ReturnValueDecoder;
use crate::spec::decoder::{ContractSpec, ContractStructDef};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use stellar_xdr::curr::{ScMap, ScVal};

/// Difference between a decoded struct payload and the struct it claims to be.
///
/// Every field named here is reported rather than dropped or nulled, because
/// each one means the payload was not produced by the contract version that
/// defines this struct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructDecodeReport {
    /// Name of the struct whose layout the payload was matched against.
    pub struct_name: String,

    /// Declared fields absent from the payload.
    ///
    /// Rendered as `null` in the output so the result still has the shape of
    /// the Rust struct.
    pub missing_fields: Vec<String>,

    /// Payload keys that the struct does not declare.
    ///
    /// Still rendered, after the declared fields, so decoding is lossless.
    pub unexpected_keys: Vec<String>,
}

impl StructDecodeReport {
    /// Whether the payload matched the struct exactly.
    pub fn is_exact(&self) -> bool {
        self.missing_fields.is_empty() && self.unexpected_keys.is_empty()
    }
}

/// Decodes a contract struct using the field names from its contract spec.
#[derive(Debug, Clone, Copy, Default)]
pub struct StructDecoder;

impl StructDecoder {
    /// Creates a struct decoder.
    pub const fn new() -> Self {
        Self
    }

    /// Finds the struct whose declared fields match a map's symbol keys.
    ///
    /// A payload is matched on the *set* of keys, so entry order is
    /// irrelevant. Both key spellings are accepted because both occur in
    /// practice: a contract compiled for the current protocol encodes struct
    /// keys as `ScSymbol`, while payloads decoded from older or hand-built
    /// data can carry `ScString` keys.
    ///
    /// Matching is exact on purpose. A struct with a *subset* of keys is
    /// ambiguous — several structs commonly share field names — and guessing
    /// would produce confidently wrong field types. Such a payload is better
    /// rendered as the dictionary it actually is, with
    /// [`StructDecoder::closest`] available to callers that want to inspect
    /// the near miss.
    ///
    /// Ambiguity is resolved deterministically: if more than one declared
    /// struct has the same field set, the first in spec order wins, because
    /// `ContractSpec::structs` preserves the contract's own declaration order.
    pub fn lookup<'a>(
        &self,
        entries: &ScMap,
        spec: &'a ContractSpec,
    ) -> Option<&'a ContractStructDef> {
        let keys = symbol_keys(entries)?;
        spec.structs
            .iter()
            .find(|struct_def| field_name_set(struct_def) == keys)
    }

    /// Finds the struct that best describes a map's keys.
    ///
    /// Unlike [`StructDecoder::lookup`], this accepts partial and extra keys
    /// and reports why a payload deviates from the chosen struct, which is what
    /// makes it useful for explaining a mismatch.
    ///
    /// Prefers the struct sharing the most keys with the payload; ties are
    /// broken in spec order. `None` means the payload shares no field name
    /// with any declared struct, so it is not a struct value at all.
    pub fn closest<'a>(
        &self,
        entries: &ScMap,
        spec: &'a ContractSpec,
    ) -> Option<(&'a ContractStructDef, StructDecodeReport)> {
        let keys = symbol_keys(entries)?;
        let mut best: Option<(&ContractStructDef, usize)> = None;

        for struct_def in &spec.structs {
            let declared = field_name_set(struct_def);
            let shared = keys.intersection(&declared).count();
            if shared == 0 {
                continue;
            }
            // Strictly-greater keeps the first struct in spec order when two
            // share a count, so the result does not depend on iteration luck.
            if best.is_none_or(|(_, best_shared)| shared > best_shared) {
                best = Some((struct_def, shared));
            }
        }

        best.map(|(struct_def, _)| (struct_def, self.check(struct_def, entries)))
    }

    /// Compares a map against a struct definition without decoding it.
    pub fn check(&self, struct_def: &ContractStructDef, entries: &ScMap) -> StructDecodeReport {
        let mut report = StructDecodeReport {
            struct_name: struct_def.name.clone(),
            ..StructDecodeReport::default()
        };

        for field in &struct_def.fields {
            if !entries
                .iter()
                .any(|entry| key_matches(&entry.key, &field.name))
            {
                report.missing_fields.push(field.name.clone());
            }
        }

        let declared = field_name_set(struct_def);
        for entry in entries.iter() {
            if let Some(key) = symbol_key(&entry.key) {
                if !declared.contains(&key) {
                    report.unexpected_keys.push(key);
                }
            }
        }
        report.unexpected_keys.sort();
        report
    }

    /// Decodes a struct value, reporting any deviation from the definition.
    ///
    /// Handles both on-wire encodings a struct can arrive in: a symbol-keyed
    /// `Map` (what a contract returns today) and a positional `Vec` (the
    /// encoding used for struct *arguments*). Anything else falls back to the
    /// generic renderer, since there is nothing to map onto the fields.
    ///
    /// The value is decoded without the report; use [`StructDecoder::decode`]
    /// when the shape check is not needed.
    #[must_use]
    pub fn decode_checked(
        &self,
        val: &ScVal,
        struct_def: &ContractStructDef,
        spec: &ContractSpec,
    ) -> (Value, StructDecodeReport) {
        match val {
            ScVal::Map(Some(entries)) => (
                Self::decode_map(entries, struct_def, spec),
                self.check(struct_def, entries),
            ),
            ScVal::Vec(Some(values)) => (
                Self::decode_vec(values, struct_def, spec),
                StructDecodeReport {
                    struct_name: struct_def.name.clone(),
                    missing_fields: struct_def
                        .fields
                        .iter()
                        .skip(values.len())
                        .map(|field| field.name.clone())
                        .collect(),
                    unexpected_keys: Vec::new(),
                },
            ),
            _ => (
                ReturnValueDecoder::decode_dynamic(val),
                StructDecodeReport {
                    struct_name: struct_def.name.clone(),
                    ..StructDecodeReport::default()
                },
            ),
        }
    }

    /// Decodes a struct value into an object keyed by declared field name.
    ///
    /// Every declared field is always present, in declaration order, so the
    /// output has the same shape as the Rust struct even when a field is
    /// missing from the payload. Keys the struct does not declare are rendered
    /// after them rather than dropped, which keeps decoding lossless and makes
    /// an unexpected key visible instead of erased.
    #[must_use]
    pub fn decode(
        &self,
        val: &ScVal,
        struct_def: &ContractStructDef,
        spec: &ContractSpec,
    ) -> Value {
        match val {
            ScVal::Map(Some(entries)) => Self::decode_map(entries, struct_def, spec),
            ScVal::Vec(Some(values)) => Self::decode_vec(values, struct_def, spec),
            _ => ReturnValueDecoder::decode_dynamic(val),
        }
    }

    fn decode_map(entries: &ScMap, struct_def: &ContractStructDef, spec: &ContractSpec) -> Value {
        let mut object = Map::with_capacity(entries.len());

        for field in &struct_def.fields {
            let value = entries
                .iter()
                .find(|entry| key_matches(&entry.key, &field.name))
                .map_or(Value::Null, |entry| {
                    ReturnValueDecoder::decode_value(
                        &entry.val,
                        field.type_def.as_ref(),
                        Some(spec),
                    )
                });
            object.insert(field.name.clone(), value);
        }

        // Preserve undeclared keys: a value that carries one is telling us it
        // came from a contract version we do not have a struct for, and
        // dropping it would erase the evidence.
        let declared: BTreeSet<&str> = struct_def.fields.iter().map(|f| f.name.as_str()).collect();
        for entry in entries.iter() {
            let Some(key) = symbol_key(&entry.key) else {
                continue;
            };
            if declared.contains(key.as_str()) {
                continue;
            }
            object.insert(
                key,
                ReturnValueDecoder::decode_value(&entry.val, None, Some(spec)),
            );
        }

        Value::Object(object)
    }

    fn decode_vec(
        values: &stellar_xdr::curr::ScVec,
        struct_def: &ContractStructDef,
        spec: &ContractSpec,
    ) -> Value {
        let mut object = Map::with_capacity(struct_def.fields.len());

        for (index, field) in struct_def.fields.iter().enumerate() {
            let value = values.get(index).map_or(Value::Null, |item| {
                ReturnValueDecoder::decode_value(item, field.type_def.as_ref(), Some(spec))
            });
            object.insert(field.name.clone(), value);
        }

        Value::Object(object)
    }
}

/// The symbol keys of a map, or `None` if any key is not a symbol.
///
/// A struct value has only symbol keys. A map that mixes in `U32` or other
/// scalar keys is a real `Map` value, and coercing it to a struct shape would
/// misrepresent it.
fn symbol_keys(entries: &ScMap) -> Option<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for entry in entries.iter() {
        keys.insert(symbol_key(&entry.key)?);
    }
    Some(keys)
}

fn field_name_set(struct_def: &ContractStructDef) -> BTreeSet<String> {
    struct_def
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect()
}

fn symbol_key(key: &ScVal) -> Option<String> {
    match key {
        ScVal::Symbol(symbol) => Some(symbol.to_string()),
        ScVal::String(string) => Some(string.to_string()),
        _ => None,
    }
}

fn key_matches(key: &ScVal, field_name: &str) -> bool {
    symbol_key(key).is_some_and(|key| key == field_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::decoder::{ContractStructDef, ContractStructField};
    use stellar_xdr::curr::{ScMapEntry, ScSpecTypeDef, ScString, ScSymbol, ScVec, StringM};

    fn field(name: &str, type_def: ScSpecTypeDef) -> ContractStructField {
        ContractStructField {
            name: name.to_string(),
            type_name: "T".to_string(),
            doc: None,
            type_def: Some(type_def),
        }
    }

    fn struct_def(name: &str, fields: Vec<ContractStructField>) -> ContractStructDef {
        ContractStructDef {
            name: name.to_string(),
            fields,
            doc: None,
        }
    }

    fn spec_with(structs: Vec<ContractStructDef>) -> ContractSpec {
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

    fn user_spec() -> ContractSpec {
        spec_with(vec![struct_def(
            "User",
            vec![
                field("id", ScSpecTypeDef::U64),
                field("name", ScSpecTypeDef::String),
                field("active", ScSpecTypeDef::Bool),
            ],
        )])
    }

    fn sym(value: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(
            StringM::try_from(value.as_bytes().to_vec()).unwrap(),
        ))
    }

    fn map(entries: Vec<(&str, ScVal)>) -> ScVal {
        ScVal::Map(Some(
            entries
                .into_iter()
                .map(|(key, val)| ScMapEntry { key: sym(key), val })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        ))
    }

    fn str_val(value: &str) -> ScVal {
        ScVal::String(ScString(
            StringM::try_from(value.as_bytes().to_vec()).unwrap(),
        ))
    }

    #[test]
    fn lookup_matches_a_map_by_its_field_names() {
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let val = map(vec![
            ("id", ScVal::U64(7)),
            ("name", str_val("Alice")),
            ("active", ScVal::Bool(true)),
        ]);

        let ScVal::Map(Some(entries)) = &val else {
            unreachable!("fixture is a map")
        };

        let found = decoder
            .lookup(entries, &spec)
            .expect("User is declared with these fields");
        assert_eq!(found.name, "User");
    }

    #[test]
    fn lookup_ignores_entry_order() {
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let val = map(vec![
            ("active", ScVal::Bool(false)),
            ("id", ScVal::U64(1)),
            ("name", str_val("Bob")),
        ]);

        let ScVal::Map(Some(entries)) = &val else {
            unreachable!("fixture is a map")
        };

        assert!(decoder.lookup(entries, &spec).is_some());
    }

    #[test]
    fn lookup_declines_a_partial_match_because_it_is_ambiguous() {
        // The payload carries a subset of `User`'s fields. Reading it as a
        // `User` would type `active` as null rather than reveal that the
        // payload is from a different shape, and another struct with the same
        // two names could just as well be meant, so `lookup` declines.
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let val = map(vec![("id", ScVal::U64(1)), ("name", str_val("x"))]);
        let ScVal::Map(Some(entries)) = &val else {
            unreachable!("fixture is a map")
        };

        assert!(decoder.lookup(entries, &spec).is_none());
        let (found, report) = decoder.closest(entries, &spec).expect("shares field names");
        assert_eq!(found.name, "User");
        assert_eq!(report.missing_fields, vec!["active".to_string()]);
        assert!(report.unexpected_keys.is_empty());
    }

    #[test]
    fn lookup_resolves_duplicate_field_sets_in_spec_order() {
        // Two structs with identical field sets are indistinguishable by
        // layout, so the winner must be deterministic: first declared wins.
        let decoder = StructDecoder::new();
        let fields = || {
            vec![
                field("id", ScSpecTypeDef::U64),
                field("name", ScSpecTypeDef::String),
            ]
        };
        let spec = spec_with(vec![
            struct_def("First", fields()),
            struct_def("Second", fields()),
        ]);

        let val = map(vec![("id", ScVal::U64(1)), ("name", str_val("x"))]);
        let ScVal::Map(Some(entries)) = &val else {
            unreachable!("fixture is a map")
        };

        assert_eq!(
            decoder.lookup(entries, &spec).expect("exact match").name,
            "First"
        );
        assert_eq!(
            decoder
                .closest(entries, &spec)
                .expect("shares names")
                .0
                .name,
            "First"
        );
    }

    #[test]
    fn lookup_skips_maps_with_non_symbol_keys() {
        // `U32(1)` is a legitimate Map key; a map using one is not a struct.
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let val = ScVal::Map(Some(
            vec![
                ScMapEntry {
                    key: sym("id"),
                    val: ScVal::U64(1),
                },
                ScMapEntry {
                    key: ScVal::U32(1),
                    val: ScVal::Bool(true),
                },
                ScMapEntry {
                    key: sym("active"),
                    val: ScVal::Bool(true),
                },
            ]
            .try_into()
            .unwrap(),
        ));
        let ScVal::Map(Some(entries)) = &val else {
            unreachable!("fixture is a map")
        };

        assert!(decoder.lookup(entries, &spec).is_none());
    }

    #[test]
    fn decode_renders_declared_fields_in_declaration_order() {
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let user = spec.structs[0].clone();
        let val = map(vec![
            ("name", str_val("Alice")),
            ("active", ScVal::Bool(true)),
            ("id", ScVal::U64(7)),
        ]);

        assert_eq!(
            decoder.decode(&val, &user, &spec),
            serde_json::json!({ "id": 7, "name": "Alice", "active": true })
        );
    }

    #[test]
    fn decode_flags_a_missing_field_and_still_keeps_the_struct_shape() {
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let user = spec.structs[0].clone();
        let val = map(vec![("id", ScVal::U64(7))]);

        let (decoded, report) = decoder.decode_checked(&val, &user, &spec);

        assert_eq!(
            decoded,
            serde_json::json!({ "id": 7, "name": null, "active": null })
        );
        assert_eq!(
            report.missing_fields,
            vec!["name".to_string(), "active".to_string()]
        );
        assert!(report.unexpected_keys.is_empty());
        assert!(!report.is_exact());
    }

    #[test]
    fn decode_keeps_an_unexpected_key_instead_of_dropping_it() {
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let user = spec.structs[0].clone();
        let val = map(vec![
            ("id", ScVal::U64(7)),
            ("name", str_val("Alice")),
            ("active", ScVal::Bool(true)),
            ("legacy_score", ScVal::U32(3)),
        ]);

        let (decoded, report) = decoder.decode_checked(&val, &user, &spec);

        assert_eq!(decoded["legacy_score"], serde_json::json!(3));
        assert_eq!(report.unexpected_keys, vec!["legacy_score".to_string()]);
        assert!(report.missing_fields.is_empty());
    }

    #[test]
    fn decode_accepts_string_keys_as_well_as_symbols() {
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let user = spec.structs[0].clone();
        let val = ScVal::Map(Some(
            vec![
                ScMapEntry {
                    key: str_val("id"),
                    val: ScVal::U64(7),
                },
                ScMapEntry {
                    key: str_val("name"),
                    val: str_val("Alice"),
                },
                ScMapEntry {
                    key: str_val("active"),
                    val: ScVal::Bool(true),
                },
            ]
            .try_into()
            .unwrap(),
        ));

        let (decoded, report) = decoder.decode_checked(&val, &user, &spec);
        assert_eq!(
            decoded,
            serde_json::json!({ "id": 7, "name": "Alice", "active": true })
        );
        assert!(report.is_exact());
    }

    #[test]
    fn decode_handles_the_positional_vec_encoding() {
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let user = spec.structs[0].clone();
        let val = ScVal::Vec(Some(ScVec(
            vec![ScVal::U64(7), str_val("Alice"), ScVal::Bool(true)]
                .try_into()
                .unwrap(),
        )));

        let (decoded, report) = decoder.decode_checked(&val, &user, &spec);
        assert_eq!(
            decoded,
            serde_json::json!({ "id": 7, "name": "Alice", "active": true })
        );
        assert!(report.is_exact());
    }

    #[test]
    fn decode_falls_back_when_the_value_is_not_a_struct_encoding() {
        let decoder = StructDecoder::new();
        let spec = user_spec();
        let user = spec.structs[0].clone();

        assert_eq!(
            decoder.decode(&ScVal::U32(1), &user, &spec),
            serde_json::json!(1)
        );
    }

    #[test]
    fn decode_types_field_values_from_the_struct_definition() {
        // `balance` is declared u128, so it renders as a decimal string rather
        // than the hex the generic renderer would produce for a raw ScVal.
        let decoder = StructDecoder::new();
        let spec = spec_with(vec![struct_def(
            "Wallet",
            vec![field("balance", ScSpecTypeDef::U128)],
        )]);
        let wallet = spec.structs[0].clone();
        let val = map(vec![(
            "balance",
            ScVal::U128(stellar_xdr::curr::UInt128Parts { hi: 0, lo: 1000 }),
        )]);

        assert_eq!(
            decoder.decode(&val, &wallet, &spec),
            serde_json::json!({ "balance": "1000" })
        );
    }

    #[test]
    fn decode_resolves_nested_struct_fields() {
        let decoder = StructDecoder::new();
        let spec = spec_with(vec![
            struct_def(
                "User",
                vec![
                    field("id", ScSpecTypeDef::U64),
                    field("name", ScSpecTypeDef::String),
                    field("active", ScSpecTypeDef::Bool),
                ],
            ),
            struct_def(
                "Owner",
                vec![
                    field("agent", nested_udt("User")),
                    field("since", ScSpecTypeDef::U32),
                ],
            ),
        ]);
        let owner = spec.structs[1].clone();
        let val = map(vec![
            (
                "agent",
                map(vec![
                    ("id", ScVal::U64(7)),
                    ("name", str_val("Alice")),
                    ("active", ScVal::Bool(true)),
                ]),
            ),
            ("since", ScVal::U32(2026)),
        ]);

        assert_eq!(
            decoder.decode(&val, &owner, &spec),
            serde_json::json!({
                "agent": { "id": 7, "name": "Alice", "active": true },
                "since": 2026
            })
        );
    }

    fn nested_udt(name: &str) -> ScSpecTypeDef {
        ScSpecTypeDef::Udt(stellar_xdr::curr::ScSpecTypeUdt {
            name: StringM::try_from(name.as_bytes().to_vec()).unwrap(),
        })
    }
}
