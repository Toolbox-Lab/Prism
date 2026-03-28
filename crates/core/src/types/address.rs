//! Stellar address types and validation.
//!
//! Wraps `stellar_strkey` to provide a strongly-typed `Address` that
//! validates the strkey format (including checksum) on construction.

use stellar_strkey::DecodeError;

/// A validated Stellar account address (G-address / Ed25519 public key strkey).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address(stellar_strkey::ed25519::PublicKey);

impl Address {
    /// Parse and validate a Stellar G-address strkey.
    ///
    /// Returns `Err(DecodeError::Invalid)` if the string is not a valid
    /// Ed25519 public-key strkey, including when the checksum is corrupted.
    pub fn from_string(s: &str) -> Result<Self, DecodeError> {
        stellar_strkey::ed25519::PublicKey::from_string(s).map(Address)
    }

    /// Return the raw 32-byte Ed25519 public key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0 .0
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a valid G-address from a known 32-byte payload so the test is
    /// self-contained and does not depend on any external address being valid.
    fn valid_address_string() -> String {
        stellar_strkey::ed25519::PublicKey([1u8; 32]).to_string()
    }

    #[test]
    fn valid_address_parses_successfully() {
        let s = valid_address_string();
        let result = Address::from_string(&s);
        assert!(
            result.is_ok(),
            "expected valid address to parse, got: {result:?}"
        );
    }

    #[test]
    fn roundtrip_preserves_address_string() {
        let s = valid_address_string();
        let addr = Address::from_string(&s).unwrap();
        assert_eq!(addr.to_string(), s);
    }

    #[test]
    fn corrupted_checksum_last_char_is_rejected() {
        // Mutate the final character — this is entirely within the 2-byte
        // CRC-16 checksum that strkey appends after the 32-byte payload.
        let valid = valid_address_string();
        let mut corrupted = valid.clone();
        let last = corrupted.pop().unwrap();
        // Replace with a different base-32 character so the checksum differs.
        let replacement = if last == 'A' { 'B' } else { 'A' };
        corrupted.push(replacement);

        // Guard: we actually changed something.
        assert_ne!(corrupted, valid);

        let result = Address::from_string(&corrupted);
        assert!(
            result.is_err(),
            "expected corrupted checksum to be rejected, but got Ok"
        );
        assert_eq!(result.unwrap_err(), DecodeError::Invalid);
    }

    #[test]
    fn corrupted_checksum_last_four_chars_is_rejected() {
        // Replace the last 4 characters (covers the full 2-byte checksum
        // region regardless of base-32 boundary alignment).
        let valid = valid_address_string();
        let base = &valid[..valid.len() - 4];

        // Pick a suffix that differs from the original.
        let original_suffix = &valid[valid.len() - 4..];
        let replacement_suffix = if original_suffix == "AAAA" {
            "BBBB"
        } else {
            "AAAA"
        };
        let corrupted = format!("{base}{replacement_suffix}");

        assert_ne!(corrupted, valid);

        let result = Address::from_string(&corrupted);
        assert!(
            result.is_err(),
            "expected 4-char checksum mutation to be rejected, but got Ok"
        );
        assert_eq!(result.unwrap_err(), DecodeError::Invalid);
    }

    #[test]
    fn empty_string_is_rejected() {
        assert_eq!(Address::from_string("").unwrap_err(), DecodeError::Invalid);
    }

    #[test]
    fn wrong_prefix_is_rejected() {
        // S-addresses are secret keys, not public keys — must be rejected.
        let secret = "SCZANGBA5RLMPI7JMTP2UX7BAOWELXTQ7KVESYYGWSQQQ7QMVWFASOO";
        assert_eq!(
            Address::from_string(secret).unwrap_err(),
            DecodeError::Invalid
        );
    }

    #[test]
    fn truncated_address_is_rejected() {
        let valid = valid_address_string();
        let truncated = &valid[..valid.len() - 8];
        assert_eq!(
            Address::from_string(truncated).unwrap_err(),
            DecodeError::Invalid
        );
    }
}
