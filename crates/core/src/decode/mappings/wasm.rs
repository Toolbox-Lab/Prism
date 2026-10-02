use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmErrorDetail {
    pub code: u32,
    pub name: &'static str,
    /// Short explanation of the WebAssembly execution failure.
    pub summary: &'static str,
    pub severity: Severity,
}

pub const WASM_ERROR_DETAILS: &[WasmErrorDetail] = &[
    WasmErrorDetail {
        code: 0,
        name: "InvalidModule",
        summary: "Invalid WASM module: the contract bytecode failed validation — recompile with a compatible Soroban SDK version.",
        severity: Severity::Error,
    },
    WasmErrorDetail {
        code: 1,
        name: "Unreachable",
        summary: "WASM trap: the contract hit an `unreachable` instruction, usually from a Rust panic or a failed assertion.",
        severity: Severity::Error,
    },
    WasmErrorDetail {
        code: 2,
        name: "MemoryAccessOutOfBounds",
        summary: "WASM trap: the contract read or wrote linear memory outside its valid bounds (out-of-bounds memory access).",
        severity: Severity::Error,
    },
    WasmErrorDetail {
        code: 3,
        name: "TableAccessOutOfBounds",
        summary: "WASM trap: an indirect call referenced a function-table index that is out of bounds.",
        severity: Severity::Error,
    },
    WasmErrorDetail {
        code: 4,
        name: "IndirectCallTypeMismatch",
        summary: "WASM trap: an indirect call reached a function whose signature did not match the call site.",
        severity: Severity::Error,
    },
    WasmErrorDetail {
        code: 5,
        name: "IntegerDivisionByZero",
        summary: "WASM trap: the contract performed an integer division or remainder with a divisor of zero.",
        severity: Severity::Error,
    },
    WasmErrorDetail {
        code: 6,
        name: "IntegerOverflow",
        summary: "WASM trap: an integer arithmetic operation overflowed the range of its type.",
        severity: Severity::Error,
    },
    WasmErrorDetail {
        code: 7,
        name: "InvalidConversionToInt",
        summary: "WASM trap: a float-to-integer conversion was NaN or fell outside the target integer range.",
        severity: Severity::Error,
    },
    WasmErrorDetail {
        code: 8,
        name: "StackOverflow",
        summary: "WASM trap: call recursion exceeded the VM stack-depth limit (stack overflow).",
        severity: Severity::Error,
    },
];

pub fn lookup(code: u32) -> Option<&'static WasmErrorDetail> {
    WASM_ERROR_DETAILS.iter().find(|detail| detail.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_out_of_bounds_memory_detail() {
        let detail = lookup(2).expect("memory access out of bounds detail");
        assert_eq!(detail.name, "MemoryAccessOutOfBounds");
        assert!(detail.summary.contains("out-of-bounds memory access"));
    }

    #[test]
    fn lookup_returns_invalid_module_for_code_zero() {
        let detail = lookup(0).expect("invalid module detail");
        assert_eq!(detail.name, "InvalidModule");
    }

    #[test]
    fn table_covers_known_wasm_codes() {
        assert_eq!(WASM_ERROR_DETAILS.len(), 9);
        assert!(lookup(99).is_none());
    }

    #[test]
    fn codes_are_contiguous_and_unique() {
        for (i, detail) in WASM_ERROR_DETAILS.iter().enumerate() {
            assert_eq!(detail.code as usize, i, "codes should be contiguous from 0");
        }
    }
}
