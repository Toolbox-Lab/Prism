//! `TypeResolver` — translates `SCSpecTypeDef` XDR values into human-readable
//! type-name strings.
//!
//! Soroban smart contracts encode every parameter and return type using the
//! [`stellar_xdr::curr::ScSpecTypeDef`] enum, which models the full abstract
//! syntax tree of contract types (primitives, generics, named user-defined
//! types, etc.).  `TypeResolver` traverses that tree recursively and produces
//! the idiomatic string representation used in diagnostics and error messages.
//!
//! # Examples
//!
//! ```rust
//! use grat_core::spec::type_resolver::TypeResolver;
//! use stellar_xdr::curr::{ScSpecTypeDef, ScSpecTypeMap, ScSpecTypeVec};
//!
//! // Simple base type
//! let name = TypeResolver::resolve(&ScSpecTypeDef::I128).unwrap();
//! assert_eq!(name, "i128");
//!
//! // Generic: Vec<u32>
//! let vec_type = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
//!     element_type: Box::new(ScSpecTypeDef::U32),
//! }));
//! assert_eq!(TypeResolver::resolve(&vec_type).unwrap(), "Vec<u32>");
//! ```

use crate::error::{GratError, GratResult};
use stellar_xdr::curr::ScSpecTypeDef;

/// Utility for translating [`ScSpecTypeDef`] values into developer-readable
/// type-name strings.
///
/// All methods are stateless; use [`TypeResolver::resolve`] directly.
pub struct TypeResolver;

impl TypeResolver {
    /// Recursively resolve `type_def` into a human-readable type string.
    ///
    /// Returns `Ok(String)` for every supported variant, and
    /// `Err(GratError::SpecError)` for deprecated or unsupported protocol
    /// variants that should not appear in modern Soroban contracts.
    ///
    /// # Errors
    ///
    /// Returns [`GratError::SpecError`] when the type definition contains a
    /// variant that is no longer part of the live Soroban protocol (e.g.
    /// `Ledger`, `Account`) or is structurally invalid.
    pub fn resolve(type_def: &ScSpecTypeDef) -> GratResult<String> {
        match type_def {
            // ── Primitive / base types ────────────────────────────────────
            ScSpecTypeDef::Val => Ok("Val".to_string()),
            ScSpecTypeDef::Bool => Ok("bool".to_string()),
            ScSpecTypeDef::Void => Ok("void".to_string()),
            ScSpecTypeDef::Error => Ok("Error".to_string()),
            ScSpecTypeDef::U32 => Ok("u32".to_string()),
            ScSpecTypeDef::I32 => Ok("i32".to_string()),
            ScSpecTypeDef::U64 => Ok("u64".to_string()),
            ScSpecTypeDef::I64 => Ok("i64".to_string()),
            ScSpecTypeDef::Timepoint => Ok("Timepoint".to_string()),
            ScSpecTypeDef::Duration => Ok("Duration".to_string()),
            ScSpecTypeDef::U128 => Ok("u128".to_string()),
            ScSpecTypeDef::I128 => Ok("i128".to_string()),
            ScSpecTypeDef::U256 => Ok("U256".to_string()),
            ScSpecTypeDef::I256 => Ok("I256".to_string()),
            ScSpecTypeDef::Bytes => Ok("Bytes".to_string()),
            ScSpecTypeDef::String => Ok("String".to_string()),
            ScSpecTypeDef::Symbol => Ok("Symbol".to_string()),
            ScSpecTypeDef::Address => Ok("Address".to_string()),

            // ── Fixed-length bytes ────────────────────────────────────────
            ScSpecTypeDef::BytesN(b) => Ok(format!("BytesN<{}>", b.n)),

            // ── Generic container types (recursive) ───────────────────────
            ScSpecTypeDef::Option(opt) => {
                let inner = Self::resolve(&opt.value_type)?;
                Ok(format!("Option<{inner}>"))
            }

            ScSpecTypeDef::Vec(vec) => {
                let elem = Self::resolve(&vec.element_type)?;
                Ok(format!("Vec<{elem}>"))
            }

            ScSpecTypeDef::Map(map) => {
                let key = Self::resolve(&map.key_type)?;
                let val = Self::resolve(&map.value_type)?;
                Ok(format!("Map<{key}, {val}>"))
            }

            ScSpecTypeDef::Result(res) => {
                let ok = Self::resolve(&res.ok_type)?;
                let err = Self::resolve(&res.error_type)?;
                Ok(format!("Result<{ok}, {err}>"))
            }

            ScSpecTypeDef::Tuple(tuple) => {
                let parts: GratResult<Vec<String>> = tuple
                    .value_types
                    .iter()
                    .map(Self::resolve)
                    .collect();
                Ok(format!("({})", parts?.join(", ")))
            }

            // ── User-defined / named types ────────────────────────────────
            ScSpecTypeDef::Udt(udt) => Ok(udt.name.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        ScSpecTypeBytesN, ScSpecTypeMap, ScSpecTypeOption, ScSpecTypeResult, ScSpecTypeTuple,
        ScSpecTypeUdt, ScSpecTypeVec, VecM,
    };

    // ── Base type tests ───────────────────────────────────────────────────

    #[test]
    fn resolve_val() {
        assert_eq!(TypeResolver::resolve(&ScSpecTypeDef::Val).unwrap(), "Val");
    }

    #[test]
    fn resolve_bool() {
        assert_eq!(TypeResolver::resolve(&ScSpecTypeDef::Bool).unwrap(), "bool");
    }

    #[test]
    fn resolve_void() {
        assert_eq!(TypeResolver::resolve(&ScSpecTypeDef::Void).unwrap(), "void");
    }

    #[test]
    fn resolve_error() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::Error).unwrap(),
            "Error"
        );
    }

    #[test]
    fn resolve_u32() {
        assert_eq!(TypeResolver::resolve(&ScSpecTypeDef::U32).unwrap(), "u32");
    }

    #[test]
    fn resolve_i32() {
        assert_eq!(TypeResolver::resolve(&ScSpecTypeDef::I32).unwrap(), "i32");
    }

    #[test]
    fn resolve_u64() {
        assert_eq!(TypeResolver::resolve(&ScSpecTypeDef::U64).unwrap(), "u64");
    }

    #[test]
    fn resolve_i64() {
        assert_eq!(TypeResolver::resolve(&ScSpecTypeDef::I64).unwrap(), "i64");
    }

    #[test]
    fn resolve_timepoint() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::Timepoint).unwrap(),
            "Timepoint"
        );
    }

    #[test]
    fn resolve_duration() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::Duration).unwrap(),
            "Duration"
        );
    }

    #[test]
    fn resolve_u128() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::U128).unwrap(),
            "u128"
        );
    }

    #[test]
    fn resolve_i128() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::I128).unwrap(),
            "i128"
        );
    }

    #[test]
    fn resolve_u256() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::U256).unwrap(),
            "U256"
        );
    }

    #[test]
    fn resolve_i256() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::I256).unwrap(),
            "I256"
        );
    }

    #[test]
    fn resolve_bytes() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::Bytes).unwrap(),
            "Bytes"
        );
    }

    #[test]
    fn resolve_string() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::String).unwrap(),
            "String"
        );
    }

    #[test]
    fn resolve_symbol() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::Symbol).unwrap(),
            "Symbol"
        );
    }

    #[test]
    fn resolve_address() {
        assert_eq!(
            TypeResolver::resolve(&ScSpecTypeDef::Address).unwrap(),
            "Address"
        );
    }

    // ── BytesN ───────────────────────────────────────────────────────────

    #[test]
    fn resolve_bytes_n() {
        let t = ScSpecTypeDef::BytesN(Box::new(ScSpecTypeBytesN { n: 32 }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "BytesN<32>");
    }

    #[test]
    fn resolve_bytes_n_zero() {
        let t = ScSpecTypeDef::BytesN(Box::new(ScSpecTypeBytesN { n: 0 }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "BytesN<0>");
    }

    // ── Option ───────────────────────────────────────────────────────────

    #[test]
    fn resolve_option_u32() {
        let t = ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
            value_type: Box::new(ScSpecTypeDef::U32),
        }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "Option<u32>");
    }

    #[test]
    fn resolve_nested_option() {
        // Option<Option<bool>>
        let inner = ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
            value_type: Box::new(ScSpecTypeDef::Bool),
        }));
        let outer = ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
            value_type: Box::new(inner),
        }));
        assert_eq!(
            TypeResolver::resolve(&outer).unwrap(),
            "Option<Option<bool>>"
        );
    }

    // ── Vec ──────────────────────────────────────────────────────────────

    #[test]
    fn resolve_vec_i64() {
        let t = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(ScSpecTypeDef::I64),
        }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "Vec<i64>");
    }

    #[test]
    fn resolve_vec_of_vec() {
        // Vec<Vec<u32>>
        let inner = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(ScSpecTypeDef::U32),
        }));
        let outer = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(inner),
        }));
        assert_eq!(TypeResolver::resolve(&outer).unwrap(), "Vec<Vec<u32>>");
    }

    // ── Map ──────────────────────────────────────────────────────────────

    #[test]
    fn resolve_map_symbol_i128() {
        let t = ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
            key_type: Box::new(ScSpecTypeDef::Symbol),
            value_type: Box::new(ScSpecTypeDef::I128),
        }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "Map<Symbol, i128>");
    }

    #[test]
    fn resolve_map_of_vec_values() {
        // Map<Symbol, Vec<u32>>
        let vec_type = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(ScSpecTypeDef::U32),
        }));
        let t = ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
            key_type: Box::new(ScSpecTypeDef::Symbol),
            value_type: Box::new(vec_type),
        }));
        assert_eq!(
            TypeResolver::resolve(&t).unwrap(),
            "Map<Symbol, Vec<u32>>"
        );
    }

    // ── Result ───────────────────────────────────────────────────────────

    #[test]
    fn resolve_result_u32_error() {
        let t = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
            ok_type: Box::new(ScSpecTypeDef::U32),
            error_type: Box::new(ScSpecTypeDef::Error),
        }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "Result<u32, Error>");
    }

    #[test]
    fn resolve_result_void_error() {
        let t = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
            ok_type: Box::new(ScSpecTypeDef::Void),
            error_type: Box::new(ScSpecTypeDef::Error),
        }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "Result<void, Error>");
    }

    // ── Tuple ────────────────────────────────────────────────────────────

    #[test]
    fn resolve_tuple_two_elements() {
        let types: VecM<ScSpecTypeDef> = vec![ScSpecTypeDef::U32, ScSpecTypeDef::Bool]
            .try_into()
            .unwrap();
        let t = ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple { value_types: types }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "(u32, bool)");
    }

    #[test]
    fn resolve_tuple_three_elements() {
        let types: VecM<ScSpecTypeDef> =
            vec![ScSpecTypeDef::Symbol, ScSpecTypeDef::I128, ScSpecTypeDef::Address]
                .try_into()
                .unwrap();
        let t = ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple { value_types: types }));
        assert_eq!(
            TypeResolver::resolve(&t).unwrap(),
            "(Symbol, i128, Address)"
        );
    }

    #[test]
    fn resolve_empty_tuple() {
        let types: VecM<ScSpecTypeDef> = vec![].try_into().unwrap();
        let t = ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple { value_types: types }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "()");
    }

    // ── UDT ──────────────────────────────────────────────────────────────

    #[test]
    fn resolve_udt() {
        let t = ScSpecTypeDef::Udt(Box::new(ScSpecTypeUdt {
            name: "MyStruct".try_into().unwrap(),
        }));
        assert_eq!(TypeResolver::resolve(&t).unwrap(), "MyStruct");
    }

    // ── Deep nesting ─────────────────────────────────────────────────────

    #[test]
    fn resolve_deeply_nested_map_symbol_vec_result() {
        // Map<Symbol, Vec<Result<u32, Error>>>
        let result_t = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
            ok_type: Box::new(ScSpecTypeDef::U32),
            error_type: Box::new(ScSpecTypeDef::Error),
        }));
        let vec_t = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(result_t),
        }));
        let map_t = ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
            key_type: Box::new(ScSpecTypeDef::Symbol),
            value_type: Box::new(vec_t),
        }));
        assert_eq!(
            TypeResolver::resolve(&map_t).unwrap(),
            "Map<Symbol, Vec<Result<u32, Error>>>"
        );
    }

    // ── Diagnostic message integration ───────────────────────────────────

    #[test]
    fn diagnostic_message_uses_resolved_types() {
        // Simulates the diagnostic: "Expected 'amount' to be i128, got u32"
        let expected_type = ScSpecTypeDef::I128;
        let received_type = ScSpecTypeDef::U32;

        let expected_name = TypeResolver::resolve(&expected_type).unwrap();
        let received_name = TypeResolver::resolve(&received_type).unwrap();

        let msg = format!(
            "Expected parameter 'amount' to be of type '{expected_name}', but received '{received_name}'"
        );
        assert_eq!(
            msg,
            "Expected parameter 'amount' to be of type 'i128', but received 'u32'"
        );
    }
}
