use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoErrorDetail {
    pub code: u32,
    pub name: &'static str,
    pub summary: &'static str,
    pub severity: Severity,
}

pub const CRYPTO_ERROR_DETAILS: &[CryptoErrorDetail] = &[
    CryptoErrorDetail {
        code: 0,
        name: "InvalidInput",
        summary: "Invalid cryptographic input: the supplied key, signature, or hash has an invalid format or length.",
        severity: Severity::Error,
    },
    CryptoErrorDetail {
        code: 1,
        name: "InvalidSignature",
        summary: "Signature verification failed. The supplied signature does not match the message and public key.",
        severity: Severity::Error,
    },
    CryptoErrorDetail {
        code: 2,
        name: "MalformedPublicKey",
        summary: "The provided public key is malformed or cannot be decoded.",
        severity: Severity::Error,
    },
    CryptoErrorDetail {
        code: 3,
        name: "MalformedSignature",
        summary: "The provided signature is malformed or has an invalid encoding.",
        severity: Severity::Error,
    },
    CryptoErrorDetail {
        code: 4,
        name: "InvalidHash",
        summary: "The supplied hash value is invalid or has an unexpected length.",
        severity: Severity::Error,
    },
];

pub fn lookup(code: u32) -> Option<&'static CryptoErrorDetail> {
    CRYPTO_ERROR_DETAILS.iter().find(|d| d.code == code)
}
