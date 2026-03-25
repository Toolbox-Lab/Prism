//! XDR codec — thin wrapper over `stellar-xdr` with convenience methods.
//!
//! Handles serialization/deserialization of transaction envelopes, results,
//! ledger entries, SCVal, and SCSpecEntry types.

use crate::types::error::{ PrismError, PrismResult };
use base64::{ engine::general_purpose::STANDARD, Engine as _ };

/// Decode a base64-encoded XDR transaction result.
///
/// # Arguments
/// * `xdr_base64` - Base64-encoded XDR string
///
/// # Returns
/// The raw decoded bytes, ready for further parsing.
pub fn decode_xdr_base64(xdr_base64: &str) -> PrismResult<Vec<u8>> {
    // TODO: Implement full XDR decoding pipeline
    let bytes = base64_decode(xdr_base64)
        .map_err(|e| PrismError::XdrError(format!("Base64 decode failed: {e}")))?;
    let bytes = base64_decode(xdr_base64).map_err(|e|
        PrismError::XdrError(format!("Base64 decode failed: {e}"))
    )?;
    Ok(bytes)
}

/// Encode bytes to base64 XDR representation.
pub fn encode_xdr_base64(bytes: &[u8]) -> String {
    base64_encode(bytes)
}

/// Decode a transaction hash from hex string.
pub fn decode_tx_hash(hash_hex: &str) -> PrismResult<[u8; 32]> {
    let bytes = hex_decode(hash_hex).map_err(|e|
        PrismError::XdrError(format!("Invalid tx hash hex: {e}"))
    )?;
    if bytes.len() != 32 {
        return Err(
            PrismError::XdrError(format!("Transaction hash must be 32 bytes, got {}", bytes.len()))
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

// --- Internal helpers ---

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    STANDARD.decode(input).map_err(|err| err.to_string())
}

fn base64_encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn hex_decode(input: &str) -> Result<Vec<u8>, String> {
    (0..input.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&input[i..i + 2], 16).map_err(|e|
                format!("Invalid hex at position {i}: {e}")
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_tx_hash_valid() {
        let hash = "a".repeat(64);
        let result = decode_tx_hash(&hash);
        assert!(result.is_ok());
    }

    #[test]
    fn test_decode_tx_hash_invalid_length() {
        let result = decode_tx_hash("abcd");
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_xdr_base64_valid() {
        let result = decode_xdr_base64("AAAA");
        assert_eq!(result.expect("valid base64"), vec![0, 0, 0]);
    }

    #[test]
    fn test_decode_xdr_base64_invalid() {
        let result = decode_xdr_base64("!!!");
        assert!(result.is_err());
    }
}
