use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectErrorDetail {
    pub code: u32,
    pub name: &'static str,
    /// Short explanation of the failure.
    pub summary: &'static str,
    pub severity: Severity,
}

pub const OBJECT_ERROR_DETAILS: &[ObjectErrorDetail] = &[
    ObjectErrorDetail {
        code: 0,
        name: "UnknownError",
        summary: "An unknown or unclassified host object error occurred.",
        severity: Severity::Error,
    },
    ObjectErrorDetail {
        code: 1,
        name: "UnknownReference",
        summary: "The provided object handle does not reference a valid host object.",
        severity: Severity::Error,
    },
    ObjectErrorDetail {
        code: 2,
        name: "UnexpectedType",
        summary: "The host object's type does not match the expected type for the operation.",
        severity: Severity::Error,
    },
    ObjectErrorDetail {
        code: 3,
        name: "ObjectCountExceedsU32Max",
        summary: "The total number of host objects allocated in this transaction has exceeded the maximum capacity.",
        severity: Severity::Error,
    },
    ObjectErrorDetail {
        code: 4,
        name: "ObjectNotExist",
        summary: "The requested host object does not exist.",
        severity: Severity::Error,
    },
    ObjectErrorDetail {
        code: 5,
        name: "VecIndexOutOfBound",
        summary: "An index out of bounds was used when accessing a host vector or byte array.",
        severity: Severity::Error,
    },
    ObjectErrorDetail {
        code: 6,
        name: "ContractHashWrongLength",
        summary: "The provided contract hash or address has an incorrect length.",
        severity: Severity::Error,
    },
];

pub fn lookup(code: u32) -> Option<&'static ObjectErrorDetail> {
    OBJECT_ERROR_DETAILS
        .iter()
        .find(|detail| detail.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_vec_index_out_of_bound_detail() {
        let detail = lookup(5).expect("vec index out of bound detail");
        assert_eq!(detail.name, "VecIndexOutOfBound");
        assert!(
            detail.summary.contains("index out of bounds")
                || detail.summary.contains("index out of bounds")
        );
    }

    #[test]
    fn table_covers_known_object_codes() {
        assert_eq!(OBJECT_ERROR_DETAILS.len(), 7);
        assert!(lookup(99).is_none());
    }
}
