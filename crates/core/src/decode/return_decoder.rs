use crate::decode::enum_decoder::EnumDecoder;
use crate::decode::struct_decoder::StructDecoder;
use crate::spec::decoder::{ContractFunction, ContractSpec, ContractStructDef};
use serde_json::{json, Value};
use stellar_xdr::curr::{ScSpecTypeDef, ScVal};

/// Decoder for contract return values, converting raw `ScVal` structures
/// into typed JSON representations based on contract specifications.
#[derive(Debug, Clone, Default)]
pub struct ReturnValueDecoder;

impl ReturnValueDecoder {
    pub fn new() -> Self {
        Self
    }

    /// Decodes a raw return `ScVal` using the provided target function's return specification
    /// and optional `ContractSpec` (for UDT struct/enum lookups).
    pub fn decode(
        &self,
        val: &ScVal,
        type_def: Option<&ScSpecTypeDef>,
        contract_spec: Option<&ContractSpec>,
    ) -> Value {
        Self::decode_value(val, type_def, contract_spec)
    }

    /// Decodes a raw return `ScVal` into a formatted String (e.g. JSON string or pretty representation).
    pub fn decode_to_string(
        &self,
        val: &ScVal,
        type_def: Option<&ScSpecTypeDef>,
        contract_spec: Option<&ContractSpec>,
    ) -> String {
        let decoded = self.decode(val, type_def, contract_spec);
        match decoded {
            Value::String(s) => s,
            other => serde_json::to_string(&other).unwrap_or_else(|_| other.to_string()),
        }
    }

    /// Decodes a function's return value directly given a target `ContractFunction`.
    pub fn decode_function_return(
        &self,
        val: &ScVal,
        func: &ContractFunction,
        contract_spec: Option<&ContractSpec>,
    ) -> Value {
        self.decode(val, func.return_type_def.as_ref(), contract_spec)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn decode_value(
        val: &ScVal,
        type_def: Option<&ScSpecTypeDef>,
        contract_spec: Option<&ContractSpec>,
    ) -> Value {
        let Some(td) = type_def else {
            return Self::decode_dynamic(val);
        };

        match td {
            ScSpecTypeDef::Void => Value::Null,
            ScSpecTypeDef::Val => Self::decode_dynamic(val),
            ScSpecTypeDef::Bool => match val {
                ScVal::Bool(b) => json!(*b),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::U32 => match val {
                ScVal::U32(u) => json!(*u),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::I32 => match val {
                ScVal::I32(i) => json!(*i),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::U64 => match val {
                ScVal::U64(u) => json!(*u),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::I64 => match val {
                ScVal::I64(i) => json!(*i),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Timepoint => match val {
                ScVal::Timepoint(t) => json!(t.0),
                ScVal::U64(u) => json!(*u),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Duration => match val {
                ScVal::Duration(d) => json!(d.0),
                ScVal::U64(u) => json!(*u),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::U128 => match val {
                ScVal::U128(u) => {
                    let num = (u128::from(u.hi) << 64) | u128::from(u.lo);
                    json!(num.to_string())
                }
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::I128 => match val {
                ScVal::I128(i) => {
                    #[allow(clippy::cast_possible_wrap)]
                    let lo = u128::from(i.lo) as i128;
                    let num = (i128::from(i.hi) << 64) | lo;
                    json!(num.to_string())
                }
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::U256 => match val {
                ScVal::U256(u) => {
                    json!(format!(
                        "0x{:016x}{:016x}{:016x}{:016x}",
                        u.hi_hi, u.hi_lo, u.lo_hi, u.lo_lo
                    ))
                }
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::I256 => match val {
                ScVal::I256(i) => {
                    json!(format!(
                        "0x{:016x}{:016x}{:016x}{:016x}",
                        i.hi_hi, i.hi_lo, i.lo_hi, i.lo_lo
                    ))
                }
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Bytes | ScSpecTypeDef::BytesN(_) => match val {
                ScVal::Bytes(b) => {
                    let hex_str: String = b.iter().fold(String::new(), |mut acc, byte| {
                        let _ = std::fmt::Write::write_fmt(&mut acc, format_args!("{byte:02x}"));
                        acc
                    });
                    json!(format!("0x{hex_str}"))
                }
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::String => match val {
                ScVal::String(s) => json!(s.to_string()),
                ScVal::Symbol(s) => json!(s.to_string()),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Symbol => match val {
                ScVal::Symbol(s) => json!(s.to_string()),
                ScVal::String(s) => json!(s.to_string()),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Address => match val {
                ScVal::Address(addr) => {
                    json!(addr.to_string())
                }
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Error => match val {
                ScVal::Error(e) => json!(format!("{e:?}")),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Option(opt_spec) => match val {
                ScVal::Void => Value::Null,
                ScVal::Vec(Some(v)) if v.is_empty() => Value::Null,
                ScVal::Vec(Some(v)) if v.len() == 1 => {
                    Self::decode_value(&v[0], Some(&opt_spec.value_type), contract_spec)
                }
                other => Self::decode_value(other, Some(&opt_spec.value_type), contract_spec),
            },
            ScSpecTypeDef::Result(res_spec) => match val {
                ScVal::Vec(Some(v)) if v.len() == 2 => match &v[0] {
                    ScVal::Symbol(sym) if sym.to_string() == "Ok" => {
                        let ok_val =
                            Self::decode_value(&v[1], Some(&res_spec.ok_type), contract_spec);
                        json!({ "Ok": ok_val })
                    }
                    ScVal::Symbol(sym) if sym.to_string() == "Err" => {
                        let err_val =
                            Self::decode_value(&v[1], Some(&res_spec.error_type), contract_spec);
                        json!({ "Err": err_val })
                    }
                    _ => Self::decode_dynamic(val),
                },
                ScVal::Error(e) => json!({ "Err": format!("{e:?}") }),
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Vec(vec_spec) => match val {
                ScVal::Vec(Some(v)) => {
                    let items: Vec<Value> = v
                        .iter()
                        .map(|item| {
                            Self::decode_value(item, Some(&vec_spec.element_type), contract_spec)
                        })
                        .collect();
                    Value::Array(items)
                }
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Map(map_spec) => match val {
                ScVal::Map(Some(m)) => {
                    let mut obj = serde_json::Map::new();
                    let mut can_be_obj = true;
                    let mut pairs = Vec::new();

                    for entry in m.iter() {
                        let key_val =
                            Self::decode_value(&entry.key, Some(&map_spec.key_type), contract_spec);
                        let val_val = Self::decode_value(
                            &entry.val,
                            Some(&map_spec.value_type),
                            contract_spec,
                        );

                        if let Value::String(ref k_str) = key_val {
                            obj.insert(k_str.clone(), val_val);
                        } else {
                            can_be_obj = false;
                            pairs.push(json!([key_val, val_val]));
                        }
                    }

                    if can_be_obj {
                        Value::Object(obj)
                    } else {
                        Value::Array(pairs)
                    }
                }
                _ => Self::decode_dynamic(val),
            },
            ScSpecTypeDef::Tuple(tuple_spec) => match val {
                ScVal::Vec(Some(v)) => {
                    match TupleDecoder.decode_vec(
                        v,
                        &tuple_spec.value_types,
                        |item, type_def| {
                            Self::decode_value(item, Some(type_def), contract_spec)
                        },
                    ) {
                        Ok(items) => Value::Array(items),
                        Err(error) => json!({ "error": error.to_string() }),
                    }
                }
                _ => json!({ "error": "expected an SCVec for tuple decoding" }),
            },
            ScSpecTypeDef::Udt(udt_spec) => {
                let udt_name = udt_spec.name.to_string();
                if let Some(cs) = contract_spec {
                    // 1. Check structs
                    if let Some(struct_def) = cs.structs.iter().find(|s| s.name == udt_name) {
                        return Self::decode_struct(val, struct_def, cs);
                    }
                    // 2. Check enums
                    if let Some(enum_def) = cs.enums.iter().find(|e| e.name == udt_name) {
                        return EnumDecoder::new().decode(val, enum_def);
                    }
                    // 3. Check unions
                    if let Some(union_def) = cs.unions.iter().find(|u| u.name == udt_name) {
                        return Self::decode_union(val, union_def, cs);
                    }
                }
                // Fallback to dynamic decoding if UDT definition not in spec
                Self::decode_dynamic(val)
            }
        }
    }

    fn decode_struct(val: &ScVal, struct_def: &ContractStructDef, cs: &ContractSpec) -> Value {
        StructDecoder::new().decode(val, struct_def, cs)
    }

    fn decode_union(
        val: &ScVal,
        union_def: &crate::spec::decoder::ContractUnionDef,
        cs: &ContractSpec,
    ) -> Value {
        match val {
            ScVal::Symbol(sym) => json!(sym.to_string()),
            ScVal::Vec(Some(v)) if !v.is_empty() => {
                let variant_name = match &v[0] {
                    ScVal::Symbol(s) => s.to_string(),
                    ScVal::String(s) => s.to_string(),
                    _ => return Self::decode_dynamic(val),
                };

                let matching_case = union_def.cases.iter().find(|c| c.name == variant_name);
                match matching_case {
                    Some(case) => {
                        if let Some(ref type_defs) = case.value_types {
                            let payload: Vec<Value> = v[1..]
                                .iter()
                                .enumerate()
                                .map(|(i, item)| {
                                    Self::decode_value(item, type_defs.get(i), Some(cs))
                                })
                                .collect();
                            if payload.len() == 1 {
                                json!({ variant_name: payload[0].clone() })
                            } else {
                                json!({ variant_name: payload })
                            }
                        } else if let Some(ref fields) = case.fields {
                            let mut map_obj = serde_json::Map::new();
                            let items = &v[1..];
                            for (i, field) in fields.iter().enumerate() {
                                let field_val = match items.get(i) {
                                    Some(item) => {
                                        Self::decode_value(item, field.type_def.as_ref(), Some(cs))
                                    }
                                    None => Value::Null,
                                };
                                map_obj.insert(field.name.clone(), field_val);
                            }
                            json!({ variant_name: Value::Object(map_obj) })
                        } else {
                            json!(variant_name)
                        }
                    }
                    None => json!({ variant_name: Self::decode_dynamic(&v[1]) }),
                }
            }
            _ => Self::decode_dynamic(val),
        }
    }

    /// Dynamic fallback to decode any `ScVal` into structured JSON without type specifications.
    pub fn decode_dynamic(val: &ScVal) -> Value {
        match val {
            ScVal::Void => Value::Null,
            ScVal::Bool(b) => json!(*b),
            ScVal::U32(u) => json!(*u),
            ScVal::I32(i) => json!(*i),
            ScVal::U64(u) => json!(*u),
            ScVal::I64(i) => json!(*i),
            ScVal::Timepoint(t) => json!(t.0),
            ScVal::Duration(d) => json!(d.0),
            ScVal::U128(u) => {
                let num = (u128::from(u.hi) << 64) | u128::from(u.lo);
                json!(num.to_string())
            }
            ScVal::I128(i) => {
                #[allow(clippy::cast_possible_wrap)]
                let lo = u128::from(i.lo) as i128;
                let num = (i128::from(i.hi) << 64) | lo;
                json!(num.to_string())
            }
            ScVal::U256(u) => {
                json!(format!(
                    "0x{:016x}{:016x}{:016x}{:016x}",
                    u.hi_hi, u.hi_lo, u.lo_hi, u.lo_lo
                ))
            }
            ScVal::I256(i) => {
                json!(format!(
                    "0x{:016x}{:016x}{:016x}{:016x}",
                    i.hi_hi, i.hi_lo, i.lo_hi, i.lo_lo
                ))
            }
            ScVal::Bytes(b) => {
                let hex_str: String = b.iter().fold(String::new(), |mut acc, byte| {
                    let _ = std::fmt::Write::write_fmt(&mut acc, format_args!("{byte:02x}"));
                    acc
                });
                json!(format!("0x{hex_str}"))
            }
            ScVal::String(s) => json!(s.to_string()),
            ScVal::Symbol(s) => json!(s.to_string()),
            ScVal::Address(addr) => {
                json!(addr.to_string())
            }
            ScVal::Error(e) => json!(format!("{e:?}")),
            ScVal::Vec(Some(v)) => {
                let items: Vec<Value> = v.iter().map(Self::decode_dynamic).collect();
                Value::Array(items)
            }
            ScVal::Map(Some(m)) => {
                let mut obj = serde_json::Map::new();
                let mut all_string_keys = true;
                let mut pairs = Vec::new();

                for entry in m.iter() {
                    let k = Self::decode_dynamic(&entry.key);
                    let v = Self::decode_dynamic(&entry.val);
                    if let Value::String(ref k_str) = k {
                        obj.insert(k_str.clone(), v);
                    } else {
                        all_string_keys = false;
                        pairs.push(json!([k, v]));
                    }
                }

                if all_string_keys {
                    Value::Object(obj)
                } else {
                    Value::Array(pairs)
                }
            }
            _ => json!(format!("{val:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::decoder::{ContractStructDef, ContractStructField};
    use stellar_xdr::curr::{ScString, ScSymbol};

    #[test]
    fn test_primitive_return_decoding() {
        let decoder = ReturnValueDecoder::new();
        assert_eq!(
            decoder.decode(&ScVal::U32(42), Some(&ScSpecTypeDef::U32), None),
            json!(42)
        );
        assert_eq!(
            decoder.decode(&ScVal::Bool(true), Some(&ScSpecTypeDef::Bool), None),
            json!(true)
        );
        assert_eq!(
            decoder.decode(&ScVal::Void, Some(&ScSpecTypeDef::Void), None),
            Value::Null
        );
    }

    #[test]
    fn test_struct_return_decoding_map_encoded() {
        let decoder = ReturnValueDecoder::new();

        let struct_def = ContractStructDef {
            name: "User".to_string(),
            fields: vec![
                ContractStructField {
                    name: "id".to_string(),
                    type_name: "U64".to_string(),
                    doc: None,
                    type_def: Some(ScSpecTypeDef::U64),
                },
                ContractStructField {
                    name: "name".to_string(),
                    type_name: "String".to_string(),
                    doc: None,
                    type_def: Some(ScSpecTypeDef::String),
                },
            ],
            doc: None,
        };

        let contract_spec = ContractSpec {
            errors: vec![],
            functions: vec![],
            structs: vec![struct_def],
            name: None,
            version: None,
            metadata: crate::spec::metadata::ContractMetadata::default(),
            enums: vec![],
            unions: vec![],
        };

        let map_val = ScVal::Map(Some(
            vec![
                stellar_xdr::curr::ScMapEntry {
                    key: ScVal::Symbol(ScSymbol("id".try_into().unwrap())),
                    val: ScVal::U64(100),
                },
                stellar_xdr::curr::ScMapEntry {
                    key: ScVal::Symbol(ScSymbol("name".try_into().unwrap())),
                    val: ScVal::String(ScString("Alice".try_into().unwrap())),
                },
            ]
            .try_into()
            .unwrap(),
        ));

        let decoded = decoder.decode(
            &map_val,
            Some(&ScSpecTypeDef::Udt(stellar_xdr::curr::ScSpecTypeUdt {
                name: "User".try_into().unwrap(),
            })),
            Some(&contract_spec),
        );

        assert_eq!(decoded, json!({ "id": 100, "name": "Alice" }));
    }

    #[test]
    fn test_result_return_decoding() {
        let decoder = ReturnValueDecoder::new();

        let ok_val = ScVal::Vec(Some(
            vec![
                ScVal::Symbol(ScSymbol("Ok".try_into().unwrap())),
                ScVal::U32(200),
            ]
            .try_into()
            .unwrap(),
        ));

        let res_spec = ScSpecTypeDef::Result(Box::new(stellar_xdr::curr::ScSpecTypeResult {
            ok_type: Box::new(ScSpecTypeDef::U32),
            error_type: Box::new(ScSpecTypeDef::U32),
        }));

        let decoded = decoder.decode(&ok_val, Some(&res_spec), None);
        assert_eq!(decoded, json!({ "Ok": 200 }));
    }

    #[test]
    fn test_dynamic_fallback() {
        let val = ScVal::Symbol(ScSymbol("hello".try_into().unwrap()));
        assert_eq!(ReturnValueDecoder::decode_dynamic(&val), json!("hello"));
    }
}
