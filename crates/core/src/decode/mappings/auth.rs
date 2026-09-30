use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthErrorDetail {
    pub code: u32,

    pub name: &'static str,
    /// Short explanation of the failure.
    pub summary: &'static str,

    pub severity: Severity,
}

pub const AUTH_ERROR_DETAILS: &[AuthErrorDetail] = &[
    AuthErrorDetail {
        code: 0,
        name: "InvalidAction",
        summary: "The authorization context is malformed or does not match the current invocation.",
        severity: Severity::Error,
    },
    AuthErrorDetail {
        code: 1,
        name: "InvalidSignature",
        summary: "The auth signature is invalid: Ed25519 accounts usually failed verification, while smart-wallet accounts rejected the payload in __check_auth.",
        severity: Severity::Error,
    },
    AuthErrorDetail {
        code: 2,
        name: "MissingAuth",
        summary: "A required auth entry or signer was not supplied with the transaction.",
        severity: Severity::Error,
    },
    AuthErrorDetail {
        code: 3,
        name: "Forbidden",
        summary: "The caller is not allowed to perform the requested action or cross-contract call.",
        severity: Severity::Error,
    },
    AuthErrorDetail {
        code: 4,
        name: "ExpiredAuth",
        summary: "The auth entry expired before the transaction could be enforced or submitted.",
        severity: Severity::Error,
    },
    AuthErrorDetail {
        code: 5,
        name: "NotAuthorized",
        summary: "The provided auth tree is valid, but it does not grant permission for this invocation.",
        severity: Severity::Error,
    },
];

pub fn lookup(code: u32) -> Option<&'static AuthErrorDetail> {
    AUTH_ERROR_DETAILS.iter().find(|detail| detail.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_invalid_signature_detail() {
        let detail = lookup(1).expect("invalid signature detail");
        assert_eq!(detail.name, "InvalidSignature");
        assert!(detail.summary.contains("Ed25519"));
        assert!(detail.summary.contains("smart-wallet"));
    }

    #[test]
    fn table_covers_known_auth_codes() {
        assert_eq!(AUTH_ERROR_DETAILS.len(), 6);
        assert!(AUTH_ERROR_DETAILS
            .iter()
            .all(|detail| detail.severity == Severity::Error));
        assert!(lookup(99).is_none());
    }
}
