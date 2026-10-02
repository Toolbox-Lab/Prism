use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractErrorDetail {
    pub code: u32,
    pub name: &'static str,
    /// Short explanation of the failure.
    pub summary: &'static str,
    pub severity: Severity,
}

pub const CONTRACT_ERROR_DETAILS: &[ContractErrorDetail] = &[
    ContractErrorDetail {
        code: 0,
        name: "ContractError",
        summary: "Contract error: the contract's own logic rejected this call — run with --resolve to map the code to its name.",
        severity: Severity::Error,
    },
    ContractErrorDetail {
        code: 1,
        name: "InternalError",
        summary: "An internal protocol implementation error occurred (e.g. invalid ledger state).",
        severity: Severity::Error,
    },
    ContractErrorDetail {
        code: 2,
        name: "OperationNotSupportedError",
        summary: "The operation is not supported (e.g. calling clawback on an asset without clawback enabled).",
        severity: Severity::Error,
    },
    ContractErrorDetail {
        code: 3,
        name: "AlreadyInitializedError",
        summary: "The contract instance has already been initialized and cannot be re-initialized.",
        severity: Severity::Error,
    },
    ContractErrorDetail {
        code: 6,
        name: "AccountMissingError",
        summary: "An account involved in the transaction does not exist on the network.",
        severity: Severity::Error,
    },
];

pub fn lookup(code: u32) -> Option<&'static ContractErrorDetail> {
    CONTRACT_ERROR_DETAILS
        .iter()
        .find(|detail| detail.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_contract_error_detail() {
        let detail = lookup(0).expect("contract error detail");
        assert_eq!(detail.name, "ContractError");
    }

    #[test]
    fn table_covers_known_contract_codes() {
        assert_eq!(CONTRACT_ERROR_DETAILS.len(), 5);
        assert!(lookup(99).is_none());
    }
}
