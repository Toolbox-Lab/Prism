use crate::error::{PrismError, PrismResult};
use serde::{Deserialize, Serialize};
use stellar_xdr::curr::{Limited, Limits, ReadXdr, ScSpecEntry, ScSpecTypeDef};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractErrorEntry {
    pub code: u32,

    pub name: String,

    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractFunction {
    pub name: String,

    pub params: Vec<(String, String)>,

    pub return_type: String,

    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSpec {
    pub errors: Vec<ContractErrorEntry>,

    pub functions: Vec<ContractFunction>,

    pub name: Option<String>,

    pub version: Option<String>,
}

pub fn decode_contract_spec(wasm_bytes: &[u8]) -> PrismResult<ContractSpec> {
    let raw_spec = SpecParser::extract_spec(wasm_bytes)?;

    let mut errors = Vec::new();
    let mut functions = Vec::new();
    let name = None;
    let version = None;

    let mut cursor = std::io::Cursor::new(&raw_spec);
    let mut limited = Limited::new(&mut cursor, Limits::none());
    while let Ok(entry) = ScSpecEntry::read_xdr(&mut limited) {
        match entry {
            ScSpecEntry::FunctionV0(func) => {
                let func_name = func.name.to_string();
                let doc = if func.doc.is_empty() {
                    None
                } else {
                    Some(func.doc.to_string())
                };

                let mut params = Vec::new();
                for input in func.inputs.iter() {
                    let param_name = input.name.to_string();
                    let param_type = format_type_def(&input.type_);
                    params.push((param_name, param_type));
                }

                let return_type = if func.outputs.is_empty() {
                    "Void".to_string()
                } else {
                    format_type_def(&func.outputs[0])
                };

                functions.push(ContractFunction {
                    name: func_name,
                    params,
                    return_type,
                    doc,
                });
            }
            ScSpecEntry::UdtErrorEnumV0(err_enum) => {
                let enum_name = err_enum.name.to_string();
                for case in err_enum.cases.iter() {
                    let case_name = format!("{}::{}", enum_name, case.name);
                    let doc = if case.doc.is_empty() {
                        None
                    } else {
                        Some(case.doc.to_string())
                    };

                    errors.push(ContractErrorEntry {
                        code: case.value,
                        name: case_name,
                        doc,
                    });
                }
            }
            _ => {}
        }
    }

    Ok(ContractSpec {
        errors,
        functions,
        name,
        version,
    })
}

fn format_type_def(type_def: &ScSpecTypeDef) -> String {
    match type_def {
        ScSpecTypeDef::Val => "Val".to_string(),
        ScSpecTypeDef::Bool => "Bool".to_string(),
        ScSpecTypeDef::Void => "Void".to_string(),
        ScSpecTypeDef::Error => "Error".to_string(),
        ScSpecTypeDef::U32 => "U32".to_string(),
        ScSpecTypeDef::I32 => "I32".to_string(),
        ScSpecTypeDef::U64 => "U64".to_string(),
        ScSpecTypeDef::I64 => "I64".to_string(),
        ScSpecTypeDef::Timepoint => "Timepoint".to_string(),
        ScSpecTypeDef::Duration => "Duration".to_string(),
        ScSpecTypeDef::U128 => "U128".to_string(),
        ScSpecTypeDef::I128 => "I128".to_string(),
        ScSpecTypeDef::U256 => "U256".to_string(),
        ScSpecTypeDef::I256 => "I256".to_string(),
        ScSpecTypeDef::Bytes => "Bytes".to_string(),
        ScSpecTypeDef::BytesN(b) => format!("BytesN<{}>", b.n),
        ScSpecTypeDef::String => "String".to_string(),
        ScSpecTypeDef::Symbol => "Symbol".to_string(),
        ScSpecTypeDef::Address => "Address".to_string(),
        ScSpecTypeDef::Option(opt) => format!("Option<{}>", format_type_def(&opt.value_type)),
        ScSpecTypeDef::Result(res) => format!(
            "Result<{}, {}>",
            format_type_def(&res.ok_type),
            format_type_def(&res.error_type)
        ),
        ScSpecTypeDef::Vec(vec) => format!("Vec<{}>", format_type_def(&vec.element_type)),
        ScSpecTypeDef::Map(map) => format!(
            "Map<{}, {}>",
            format_type_def(&map.key_type),
            format_type_def(&map.value_type)
        ),
        ScSpecTypeDef::Tuple(tuple) => {
            let elements: Vec<String> = tuple.value_types.iter().map(format_type_def).collect();
            format!("({})", elements.join(", "))
        }
        ScSpecTypeDef::Udt(udt) => udt.name.to_string(),
    }
}

pub struct SpecParser;

impl SpecParser {
    pub fn extract_spec(wasm_bytes: &[u8]) -> PrismResult<Vec<u8>> {
        let parser = wasmparser::Parser::new(0);
        for payload in parser.parse_all(wasm_bytes) {
            let payload =
                payload.map_err(|e| PrismError::SpecError(format!("WASM parse error: {e}")))?;

            if let wasmparser::Payload::CustomSection(section) = payload {
                if section.name() == "contractspecv0" {
                    return Ok(section.data().to_vec());
                }
            }
        }

        Err(PrismError::SpecError(
            "contractspecv0 custom section not found".into(),
        ))
    }
}

pub fn resolve_error_code(spec: &ContractSpec, error_code: u32) -> Option<&ContractErrorEntry> {
    spec.errors.iter().find(|e| e.code == error_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_error_code_not_found() {
        let spec = ContractSpec {
            errors: vec![ContractErrorEntry {
                code: 1,
                name: "NotFound".to_string(),
                doc: None,
            }],
            functions: Vec::new(),
            name: None,
            version: None,
        };
        assert!(resolve_error_code(&spec, 99).is_none());
        assert!(resolve_error_code(&spec, 1).is_some());
    }

    #[test]
    fn test_extract_spec_success() {
        let mut wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        let section_name = "contractspecv0";
        let section_data = vec![1, 2, 3, 4];

        let mut custom_payload = Vec::new();
        custom_payload.push(section_name.len() as u8);
        custom_payload.extend_from_slice(section_name.as_bytes());
        custom_payload.extend_from_slice(&section_data);

        wasm.push(0);
        wasm.push(custom_payload.len() as u8);
        wasm.extend(custom_payload);

        let result = SpecParser::extract_spec(&wasm).expect("Should find section");
        assert_eq!(result, section_data);
    }

    #[test]
    fn test_extract_spec_not_found() {
        let wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        let result = SpecParser::extract_spec(&wasm);
        assert!(result.is_err());
        match result {
            Err(PrismError::SpecError(msg)) => assert!(msg.contains("not found")),
            _ => panic!("Expected SpecError"),
        }
    }
}
