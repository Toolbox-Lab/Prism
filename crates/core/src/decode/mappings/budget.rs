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
pub struct BudgetErrorDetail {
    pub code: u32,

    pub name: &'static str,
    /// Short explanation of the failure.
    pub summary: &'static str,

    pub severity: ErrorSeverity,
}

pub const BUDGET_ERROR_DETAILS: &[BudgetErrorDetail] = &[
    BudgetErrorDetail {
        code: 0,
        name: "CPUExceeded",
        summary: "CPU budget exceeded: the transaction ran out of CPU instructions before completing execution.",
        severity: ErrorSeverity::Critical,
    },
    BudgetErrorDetail {
        code: 8,
        name: "ExceededLimit",
        summary: "The transaction exceeded an allocated resource limit.",
        severity: ErrorSeverity::Error,
    },
    BudgetErrorDetail {
        code: 1,
        name: "InsufficientInstructions",
        summary: "The transaction did not have enough CPU instructions allocated.",
        severity: ErrorSeverity::Critical,
    },
    BudgetErrorDetail {
        code: 2,
        name: "InsufficientMemory",
        summary: "The transaction did not have enough memory allocated.",
        severity: ErrorSeverity::Critical,
    },
];

pub fn lookup(code: u32) -> Option<&'static BudgetErrorDetail> {
    BUDGET_ERROR_DETAILS
        .iter()
        .find(|detail| detail.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_cpu_exceeded_detail() {
        let detail = lookup(0).expect("cpu exceeded detail");
        assert_eq!(detail.name, "CPUExceeded");
        assert!(detail.summary.contains("CPU instructions"));
    }

    #[test]
    fn table_covers_known_budget_codes() {
        assert_eq!(BUDGET_ERROR_DETAILS.len(), 4);
        assert!(lookup(99).is_none());
    }
}
