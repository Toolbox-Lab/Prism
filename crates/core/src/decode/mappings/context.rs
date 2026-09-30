use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextErrorDetail {
    pub code: u32,
    pub name: &'static str,
    pub summary: &'static str,
    pub severity: Severity,
}

pub const CONTEXT_ERROR_DETAILS: &[ContextErrorDetail] = &[
    ContextErrorDetail {
        code: 0,
        name: "UnknownError",
        summary: "Host internal error: an unexpected Soroban runtime error occurred — this may be a platform bug, not a contract bug.",
        severity: Severity::Error,
    },
    ContextErrorDetail {
        code: 6,
        name: "InvalidAction",
        summary: "A host function was called in an execution context where that action is not valid; this usually points to contract code using the environment at the wrong time.",
        severity: Severity::Error,
    },
    ContextErrorDetail {
        code: 7,
        name: "InternalError",
        summary: "The Soroban host hit an unexpected internal state; this points to the execution environment rather than normal contract logic.",
        severity: Severity::Fatal,
    },
];

pub fn lookup(code: u32) -> Option<&'static ContextErrorDetail> {
    CONTEXT_ERROR_DETAILS
        .iter()
        .find(|detail| detail.code == code)
}
