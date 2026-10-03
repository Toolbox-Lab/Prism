//! Type-aware recursive decoding of Soroban values.

use crate::decode::scval_to_json;
use crate::spec::decoder::{ContractSpec, ContractStructDef};
use serde_json::{json, Map, Value};
use stellar_xdr::curr::{ScSpecTypeDef, ScVal};

/// The type information available while decoding one value.
#[derive(Clone, Copy)]
pub struct TypeRef<'a> {
    pub type_def: &'a ScSpecTypeDef,
    pub contract_spec: Option<&'a ContractSpec>,
}

impl<'a> TypeRef<'a> {
    pub fn new(type_def: &'a ScSpecTypeDef, contract_spec: Option<&'a ContractSpec>) -> Self {
        Self {
            type_def,
            contract_spec,
        }
    }
}

/// Dispatches a value to the decoder for its current type and carries the
/// contract specification into every nested value.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecursiveTypeDecoder;

impl RecursiveTypeDecoder {
    pub fn new() -> Self {
        Self
    }

    pub fn decode<'a>(&self, val: &ScVal, type_ref: Option<TypeRef<'a>>) -> Value {
        let Some(type_ref) = type_ref else {
            return scval_to_json(val, None);
        };
        self.decode_type(val, type_ref.type_def, type_ref.contract_spec, 0)
    }

    fn decode_type(
        &self,
        val: &ScVal,
        type_def: &ScSpecTypeDef,
        contract_spec: Option<&ContractSpec>,
        depth: usize,
    ) -> Value {
        if depth > 100 {
            return scval_to_json(val, None);
        }
        let child = |value: &ScVal, td: &ScSpecTypeDef| {
            self.decode_type(value, td, contract_spec, depth + 1)
        };

        match type_def {
            ScSpecTypeDef::Void => Value::Null,
            ScSpecTypeDef::Val => scval_to_json(val, None),
            ScSpecTypeDef::Bool => match val {
                ScVal::Bool(v) => json!(v),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::U32 => match val {
                ScVal::U32(v) => json!(v),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::I32 => match val {
                ScVal::I32(v) => json!(v),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::U64 => match val {
                ScVal::U64(v) => json!(v),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::I64 => match val {
                ScVal::I64(v) => json!(v),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::Timepoint => match val {
                ScVal::Timepoint(v) => json!(v.0),
                ScVal::U64(v) => json!(v),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::Duration => match val {
                ScVal::Duration(v) => json!(v.0),
                ScVal::U64(v) => json!(v),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::U128
            | ScSpecTypeDef::I128
            | ScSpecTypeDef::U256
            | ScSpecTypeDef::I256 => scval_to_json(val, None),
            ScSpecTypeDef::Bytes
            | ScSpecTypeDef::BytesN(_)
            | ScSpecTypeDef::String
            | ScSpecTypeDef::Symbol
            | ScSpecTypeDef::Address
            | ScSpecTypeDef::Error => scval_to_json(val, None),
            ScSpecTypeDef::Option(spec) => match val {
                ScVal::Void => Value::Null,
                ScVal::Vec(Some(values)) if values.is_empty() => Value::Null,
                ScVal::Vec(Some(values)) if values.len() == 1 => {
                    child(&values[0], &spec.value_type)
                }
                _ => child(val, &spec.value_type),
            },
            ScSpecTypeDef::Result(spec) => match val {
                ScVal::Vec(Some(values)) if values.len() == 2 => {
                    let name = match &values[0] {
                        ScVal::Symbol(v) => v.to_string(),
                        ScVal::String(v) => v.to_string(),
                        _ => return scval_to_json(val, None),
                    };
                    let payload = if name == "Ok" {
                        child(&values[1], &spec.ok_type)
                    } else if name == "Err" {
                        child(&values[1], &spec.error_type)
                    } else {
                        return scval_to_json(val, None);
                    };
                    json!({ (name): payload })
                }
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::Vec(spec) => match val {
                ScVal::Vec(Some(values)) => Value::Array(
                    values
                        .iter()
                        .map(|v| child(v, &spec.element_type))
                        .collect(),
                ),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::Map(spec) => match val {
                ScVal::Map(Some(entries)) => {
                    let mut object = Map::new();
                    let mut pairs = Vec::new();
                    for entry in entries.iter() {
                        let key = child(&entry.key, &spec.key_type);
                        let value = child(&entry.val, &spec.value_type);
                        if let Value::String(key) = key {
                            object.insert(key, value);
                        } else {
                            pairs.push(json!([key, value]));
                        }
                    }
                    if pairs.is_empty() {
                        Value::Object(object)
                    } else {
                        Value::Array(pairs)
                    }
                }
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::Tuple(spec) => match val {
                ScVal::Vec(Some(values)) => Value::Array(
                    values
                        .iter()
                        .enumerate()
                        .map(|(i, value)| {
                            spec.value_types
                                .get(i)
                                .map_or_else(|| scval_to_json(value, None), |td| child(value, td))
                        })
                        .collect(),
                ),
                _ => scval_to_json(val, None),
            },
            ScSpecTypeDef::Udt(spec) => {
                let Some(contract_spec) = contract_spec else {
                    return scval_to_json(val, None);
                };
                let name = spec.name.to_string();
                if let Some(def) = contract_spec.structs.iter().find(|def| def.name == name) {
                    self.decode_struct(val, def, contract_spec, depth)
                } else if let Some(def) = contract_spec.enums.iter().find(|def| def.name == name) {
                    self.decode_enum(val, def)
                } else if let Some(def) = contract_spec.unions.iter().find(|def| def.name == name) {
                    self.decode_union(val, def, contract_spec, depth)
                } else {
                    scval_to_json(val, None)
                }
            }
        }
    }

    fn decode_struct(
        &self,
        val: &ScVal,
        def: &ContractStructDef,
        spec: &ContractSpec,
        depth: usize,
    ) -> Value {
        let mut object = Map::new();
        match val {
            ScVal::Map(Some(entries)) => {
                for field in &def.fields {
                    let entry = entries.iter().find(|entry| match &entry.key {
                        ScVal::Symbol(k) => k.to_string() == field.name,
                        ScVal::String(k) => k.to_string() == field.name,
                        _ => false,
                    });
                    object.insert(
                        field.name.clone(),
                        entry
                            .and_then(|entry| {
                                field.type_def.as_ref().map(|td| {
                                    self.decode_type(&entry.val, td, Some(spec), depth + 1)
                                })
                            })
                            .unwrap_or(Value::Null),
                    );
                }
            }
            ScVal::Vec(Some(values)) => {
                for (i, field) in def.fields.iter().enumerate() {
                    object.insert(
                        field.name.clone(),
                        values
                            .get(i)
                            .and_then(|value| {
                                field
                                    .type_def
                                    .as_ref()
                                    .map(|td| self.decode_type(value, td, Some(spec), depth + 1))
                            })
                            .unwrap_or(Value::Null),
                    );
                }
            }
            _ => return scval_to_json(val, None),
        }
        Value::Object(object)
    }

    fn decode_enum(&self, val: &ScVal, def: &crate::spec::decoder::ContractEnumDef) -> Value {
        match val {
            ScVal::Symbol(v) => json!(v.to_string()),
            ScVal::String(v) => json!(v.to_string()),
            ScVal::U32(v) => def
                .cases
                .iter()
                .find(|case| case.value == *v)
                .map_or_else(|| json!(v), |case| json!(case.name)),
            _ => scval_to_json(val, None),
        }
    }

    fn decode_union(
        &self,
        val: &ScVal,
        def: &crate::spec::decoder::ContractUnionDef,
        spec: &ContractSpec,
        depth: usize,
    ) -> Value {
        let ScVal::Vec(Some(values)) = val else {
            return scval_to_json(val, None);
        };
        let Some(first) = values.first() else {
            return scval_to_json(val, None);
        };
        let name = match first {
            ScVal::Symbol(v) => v.to_string(),
            ScVal::String(v) => v.to_string(),
            _ => return scval_to_json(val, None),
        };
        let Some(case) = def.cases.iter().find(|case| case.name == name) else {
            return scval_to_json(val, None);
        };
        if let Some(types) = &case.value_types {
            let payload: Vec<Value> = values[1..]
                .iter()
                .enumerate()
                .map(|(i, value)| {
                    types.get(i).map_or_else(
                        || scval_to_json(value, None),
                        |td| self.decode_type(value, td, Some(spec), depth + 1),
                    )
                })
                .collect();
            if payload.len() == 1 {
                json!({ (name): payload[0] })
            } else {
                json!({ (name): payload })
            }
        } else {
            json!(name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::decoder::{
        ContractEnumCase, ContractEnumDef, ContractSpec, ContractStructDef, ContractStructField,
    };
    use serde_json::json;
    use stellar_xdr::curr::{ScMapEntry, ScSpecTypeTuple, ScSpecTypeUdt, ScSymbol, StringM};

    fn symbol(value: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(
            StringM::try_from(value.as_bytes().to_vec()).unwrap(),
        ))
    }

    fn map(entries: Vec<(&str, ScVal)>) -> ScVal {
        ScVal::Map(Some(
            entries
                .into_iter()
                .map(|(key, val)| ScMapEntry {
                    key: symbol(key),
                    val,
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        ))
    }

    #[test]
    fn nested_udts_keep_type_context_for_struct_enum_and_tuple() {
        let status = ScSpecTypeDef::Udt(ScSpecTypeUdt {
            name: "Status".try_into().unwrap(),
        });
        let inner = ScSpecTypeDef::Udt(ScSpecTypeUdt {
            name: "Inner".try_into().unwrap(),
        });
        let tuple = ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
            value_types: vec![ScSpecTypeDef::U32, ScSpecTypeDef::U32]
                .try_into()
                .unwrap(),
        }));
        let spec = ContractSpec {
            errors: vec![],
            functions: vec![],
            structs: vec![
                ContractStructDef {
                    name: "Outer".to_string(),
                    fields: vec![ContractStructField {
                        name: "inner".to_string(),
                        type_name: "Inner".to_string(),
                        doc: None,
                        type_def: Some(inner.clone()),
                    }],
                    doc: None,
                },
                ContractStructDef {
                    name: "Inner".to_string(),
                    fields: vec![
                        ContractStructField {
                            name: "status".to_string(),
                            type_name: "Status".to_string(),
                            doc: None,
                            type_def: Some(status.clone()),
                        },
                        ContractStructField {
                            name: "pair".to_string(),
                            type_name: "(U32,U32)".to_string(),
                            doc: None,
                            type_def: Some(tuple),
                        },
                    ],
                    doc: None,
                },
            ],
            enums: vec![ContractEnumDef {
                name: "Status".to_string(),
                cases: vec![ContractEnumCase {
                    name: "Active".to_string(),
                    value: 1,
                    doc: None,
                }],
                doc: None,
            }],
            unions: vec![],
            name: None,
            version: None,
        };
        let value = map(vec![(
            "inner",
            map(vec![
                ("status", ScVal::U32(1)),
                (
                    "pair",
                    ScVal::Vec(Some(vec![ScVal::U32(2), ScVal::U32(3)].try_into().unwrap())),
                ),
            ]),
        )]);
        let outer = ScSpecTypeDef::Udt(ScSpecTypeUdt {
            name: "Outer".try_into().unwrap(),
        });

        let decoded =
            RecursiveTypeDecoder::new().decode(&value, Some(TypeRef::new(&outer, Some(&spec))));
        assert_eq!(
            decoded,
            json!({"inner": {"status": "Active", "pair": [2, 3]}})
        );
    }
}
