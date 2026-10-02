use serde_json::Value;
use std::fmt;
use stellar_xdr::curr::{ScSpecTypeDef, ScVal, ScVec};

/// Errors raised when an `SCVec` does not match a tuple signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TupleDecodeError {
    ExpectedVec,
    LengthMismatch { expected: usize, actual: usize },
    TypeMismatch {
        index: usize,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for TupleDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedVec => write!(f, "expected an SCVec for tuple decoding"),
            Self::LengthMismatch { expected, actual } => write!(
                f,
                "tuple length mismatch: expected {expected} elements, got {actual}"
            ),
            Self::TypeMismatch {
                index,
                expected,
                actual,
            } => write!(
                f,
                "tuple element {index} type mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for TupleDecodeError {}

/// Decodes tuple values in signature order, rejecting vectors with the wrong length.
#[derive(Debug, Clone, Copy, Default)]
pub struct TupleDecoder;

impl TupleDecoder {
    /// Decodes each vector element with its corresponding tuple type definition.
    pub fn decode<F>(
        &self,
        value: &ScVal,
        signature: &[ScSpecTypeDef],
        decode_element: F,
    ) -> Result<Vec<Value>, TupleDecodeError>
    where
        F: FnMut(&ScVal, &ScSpecTypeDef) -> Value,
    {
        let ScVal::Vec(Some(values)) = value else {
            return Err(TupleDecodeError::ExpectedVec);
        };

        self.decode_vec(values, signature, decode_element)
    }

    /// Decodes an already-extracted vector after validating its exact length.
    pub fn decode_vec<F>(
        &self,
        values: &ScVec,
        signature: &[ScSpecTypeDef],
        mut decode_element: F,
    ) -> Result<Vec<Value>, TupleDecodeError>
    where
        F: FnMut(&ScVal, &ScSpecTypeDef) -> Value,
    {
        if values.len() != signature.len() {
            return Err(TupleDecodeError::LengthMismatch {
                expected: signature.len(),
                actual: values.len(),
            });
        }

        let mut decoded = Vec::with_capacity(values.len());
        for (index, (element, type_def)) in values.iter().zip(signature).enumerate() {
            if !matches_type(element, type_def) {
                return Err(TupleDecodeError::TypeMismatch {
                    index,
                    expected: format!("{type_def:?}"),
                    actual: scval_type_name(element).to_string(),
                });
            }
            decoded.push(decode_element(element, type_def));
        }

        Ok(decoded)
    }
}

fn matches_type(value: &ScVal, type_def: &ScSpecTypeDef) -> bool {
    match type_def {
        ScSpecTypeDef::Void => matches!(value, ScVal::Void),
        ScSpecTypeDef::Val => true,
        ScSpecTypeDef::Bool => matches!(value, ScVal::Bool(_)),
        ScSpecTypeDef::U32 => matches!(value, ScVal::U32(_)),
        ScSpecTypeDef::I32 => matches!(value, ScVal::I32(_)),
        ScSpecTypeDef::U64 => matches!(value, ScVal::U64(_)),
        ScSpecTypeDef::I64 => matches!(value, ScVal::I64(_)),
        ScSpecTypeDef::Timepoint => matches!(value, ScVal::Timepoint(_) | ScVal::U64(_)),
        ScSpecTypeDef::Duration => matches!(value, ScVal::Duration(_) | ScVal::U64(_)),
        ScSpecTypeDef::U128 => matches!(value, ScVal::U128(_)),
        ScSpecTypeDef::I128 => matches!(value, ScVal::I128(_)),
        ScSpecTypeDef::U256 => matches!(value, ScVal::U256(_)),
        ScSpecTypeDef::I256 => matches!(value, ScVal::I256(_)),
        ScSpecTypeDef::Bytes => matches!(value, ScVal::Bytes(_)),
        ScSpecTypeDef::BytesN(spec) => {
            matches!(value, ScVal::Bytes(bytes) if bytes.len() == spec.n as usize)
        }
        ScSpecTypeDef::String => matches!(value, ScVal::String(_) | ScVal::Symbol(_)),
        ScSpecTypeDef::Symbol => matches!(value, ScVal::Symbol(_) | ScVal::String(_)),
        ScSpecTypeDef::Address => matches!(value, ScVal::Address(_)),
        ScSpecTypeDef::Error => matches!(value, ScVal::Error(_)),
        ScSpecTypeDef::Option(spec) => match value {
            ScVal::Void => true,
            ScVal::Vec(Some(values)) if values.is_empty() => true,
            ScVal::Vec(Some(values)) if values.len() == 1 => {
                matches_type(&values[0], &spec.value_type)
            }
            _ => matches_type(value, &spec.value_type),
        },
        ScSpecTypeDef::Result(spec) => match value {
            ScVal::Error(_) => true,
            ScVal::Vec(Some(values)) if values.len() == 2 => match &values[0] {
                ScVal::Symbol(symbol) if symbol.to_string() == "Ok" => {
                    matches_type(&values[1], &spec.ok_type)
                }
                ScVal::Symbol(symbol) if symbol.to_string() == "Err" => {
                    matches_type(&values[1], &spec.error_type)
                }
                _ => false,
            },
            _ => false,
        },
        ScSpecTypeDef::Vec(spec) => match value {
            ScVal::Vec(Some(values)) => values
                .iter()
                .all(|element| matches_type(element, &spec.element_type)),
            _ => false,
        },
        ScSpecTypeDef::Map(spec) => match value {
            ScVal::Map(Some(entries)) => entries.iter().all(|entry| {
                matches_type(&entry.key, &spec.key_type)
                    && matches_type(&entry.val, &spec.value_type)
            }),
            _ => false,
        },
        ScSpecTypeDef::Tuple(spec) => match value {
            ScVal::Vec(Some(values)) if values.len() == spec.value_types.len() => values
                .iter()
                .zip(&spec.value_types)
                .all(|(element, element_type)| matches_type(element, element_type)),
            _ => false,
        },
        // UDT encodings depend on their definitions in ContractSpec, which the element
        // decoder resolves after this generic structural validation.
        ScSpecTypeDef::Udt(_) => true,
    }
}

fn scval_type_name(value: &ScVal) -> &'static str {
    match value {
        ScVal::Bool(_) => "Bool",
        ScVal::Void => "Void",
        ScVal::U32(_) => "U32",
        ScVal::I32(_) => "I32",
        ScVal::U64(_) => "U64",
        ScVal::I64(_) => "I64",
        ScVal::Timepoint(_) => "Timepoint",
        ScVal::Duration(_) => "Duration",
        ScVal::U128(_) => "U128",
        ScVal::I128(_) => "I128",
        ScVal::U256(_) => "U256",
        ScVal::I256(_) => "I256",
        ScVal::Bytes(_) => "Bytes",
        ScVal::String(_) => "String",
        ScVal::Symbol(_) => "Symbol",
        ScVal::Address(_) => "Address",
        ScVal::Error(_) => "Error",
        ScVal::Vec(_) => "Vec",
        ScVal::Map(_) => "Map",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scvec(values: Vec<ScVal>) -> ScVal {
        ScVal::Vec(Some(values.try_into().unwrap()))
    }

    #[test]
    fn decodes_elements_using_their_sequential_signature_types() {
        let decoder = TupleDecoder;
        let value = scvec(vec![ScVal::U32(7), ScVal::Bool(true)]);
        let signature = [ScSpecTypeDef::U32, ScSpecTypeDef::Bool];

        let decoded = decoder
            .decode(&value, &signature, |element, type_def| {
                json!({ "type": format!("{type_def:?}"), "value": format!("{element:?}") })
            })
            .unwrap();

        assert_eq!(decoded[0]["type"], "U32");
        assert_eq!(decoded[1]["type"], "Bool");
    }

    #[test]
    fn rejects_vectors_with_a_different_length() {
        let decoder = TupleDecoder;
        let value = scvec(vec![ScVal::U32(7)]);
        let signature = [ScSpecTypeDef::U32, ScSpecTypeDef::Bool];

        assert_eq!(
            decoder.decode(&value, &signature, |_, _| Value::Null),
            Err(TupleDecodeError::LengthMismatch {
                expected: 2,
                actual: 1,
            })
        );
    }

    #[test]
    fn rejects_elements_that_do_not_match_their_signature_type() {
        let value = scvec(vec![ScVal::Bool(true)]);
        let signature = [ScSpecTypeDef::U32];

        assert_eq!(
            TupleDecoder.decode(&value, &signature, |_, _| Value::Null),
            Err(TupleDecodeError::TypeMismatch {
                index: 0,
                expected: "U32".to_string(),
                actual: "Bool".to_string(),
            })
        );
    }

    #[test]
    fn rejects_non_vector_values() {
        assert_eq!(
            TupleDecoder.decode(&ScVal::U32(7), &[], |_, _| Value::Null),
            Err(TupleDecodeError::ExpectedVec)
        );
    }
}