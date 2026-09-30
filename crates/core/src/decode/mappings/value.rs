use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorSeverity {
    Critical,
    Error,
    Warning,
    Info,
}

impl From<ErrorSeverity> for Severity {
    fn from(sev: ErrorSeverity) -> Self {
        match sev {
            ErrorSeverity::Critical => Severity::Fatal,
            ErrorSeverity::Error => Severity::Error,
            ErrorSeverity::Warning => Severity::Warning,
            ErrorSeverity::Info => Severity::Info,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueErrorDetail {
    pub code: u32,

    pub name: &'static str,
    /// Short explanation of the failure.
    pub summary: &'static str,

    pub severity: ErrorSeverity,
}

pub const VALUE_ERROR_DETAILS: &[ValueErrorDetail] = &[
    ValueErrorDetail {
        code: 0,
        name: "UnknownError",
        summary: "Invalid value: a host function received an argument of the wrong type or format.",
        severity: ErrorSeverity::Error,
    },
    ValueErrorDetail {
        code: 1,
        name: "UnexpectedType",
        summary: "The provided value is not of the expected type for this operation.",
        severity: ErrorSeverity::Error,
    },
    ValueErrorDetail {
        code: 2,
        name: "UnexpectedSize",
        summary: "The provided value has an unexpected size or length.",
        severity: ErrorSeverity::Error,
    },
    ValueErrorDetail {
        code: 3,
        name: "MissingValue",
        summary: "A required value was missing from the input.",
        severity: ErrorSeverity::Error,
    },
    ValueErrorDetail {
        code: 4,
        name: "InternalError",
        summary: "An internal host error occurred while processing a value.",
        severity: ErrorSeverity::Critical,
    },
    ValueErrorDetail {
        code: 5,
        name: "AddedValue",
        summary: "An unexpected or disallowed value was provided.",
        severity: ErrorSeverity::Error,
    },
    ValueErrorDetail {
        code: 6,
        name: "InvalidInput",
        summary: "Malformed ScVal conversion, out-of-range integer, or otherwise invalid input passed to a host function.",
        severity: ErrorSeverity::Error,
    },
    ValueErrorDetail {
        code: 7,
        name: "AuthenticationError",
        summary: "An authentication-related value was invalid or malformed.",
        severity: ErrorSeverity::Error,
    },
];

pub fn lookup(code: u32) -> Option<&'static ValueErrorDetail> {
    VALUE_ERROR_DETAILS
        .iter()
        .find(|detail| detail.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_invalid_input_detail() {
        let detail = lookup(6).expect("invalid input detail");
        assert_eq!(detail.name, "InvalidInput");
        assert!(detail.summary.contains("Malformed"));
    }

    #[test]
    fn table_covers_known_value_codes() {
        assert_eq!(VALUE_ERROR_DETAILS.len(), 8);
        assert!(lookup(99).is_none());
    }
}
