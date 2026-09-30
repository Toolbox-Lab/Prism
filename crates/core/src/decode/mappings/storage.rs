use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageErrorDetail {
    pub code: u32,

    pub name: &'static str,
    /// Short explanation of the failure.
    pub summary: &'static str,

    pub severity: Severity,
}

pub const STORAGE_ERROR_DETAILS: &[StorageErrorDetail] = &[
    StorageErrorDetail {
        code: 0,
        name: "AccessDenied",
        summary: "The contract attempted to access a ledger entry not included in the transaction's footprint.",
        severity: Severity::Error,
    },
    StorageErrorDetail {
        code: 1,
        name: "EntryNotFound",
        summary: "The contract attempted to read a ledger key that does not exist or has been archived.",
        severity: Severity::Error,
    },
    StorageErrorDetail {
        code: 2,
        name: "ExceededLimit",
        summary: "The operation attempted to exceed resource or size limits for ledger data.",
        severity: Severity::Error,
    },
    StorageErrorDetail {
        code: 3,
        name: "InternalError",
        summary: "An internal storage failure occurred. This code alone reveals nothing; check the diagnostic events to get more signal on the underlying issue.",
        severity: Severity::Error,
    },
    StorageErrorDetail {
        code: 4,
        name: "NearExpiry",
        summary: "The accessed ledger entry is approaching its expiration ledger and will soon become read-only unless extended.",
        severity: Severity::Warning,
    },
];

pub fn lookup(code: u32) -> Option<&'static StorageErrorDetail> {
    STORAGE_ERROR_DETAILS
        .iter()
        .find(|detail| detail.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_internal_error_detail() {
        let detail = lookup(3).expect("internal error detail");
        assert_eq!(detail.name, "InternalError");
        assert!(detail.summary.contains("diagnostic events"));
    }

    #[test]
    fn table_covers_known_storage_codes() {
        assert_eq!(STORAGE_ERROR_DETAILS.len(), 5);
        assert!(lookup(4).is_some());
        assert!(lookup(99).is_none());
    }
}
