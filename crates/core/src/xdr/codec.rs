//! XDR codec - thin wrapper over `stellar-xdr` with convenience methods.
//!
//! Handles serialization/deserialization of transaction envelopes, results,
//! ledger entries, SCVal, and SCSpecEntry types.

use crate::types::error::{ PrismError, PrismResult };
use base64::{ engine::general_purpose::STANDARD, Engine as _ };
use stellar_xdr::curr::{ Limits, ReadXdr, TransactionMeta, WriteXdr };

/// Convenience trait for working with base64-encoded Stellar XDR values.
pub trait XdrCodec: Sized {
    /// Deserialize a base64-encoded XDR payload into the target type.
    fn from_xdr_base64(xdr_base64: &str) -> PrismResult<Self>;

    /// Serialize the value to a base64-encoded XDR payload.
    fn to_xdr_base64(&self) -> PrismResult<String>;
}

impl XdrCodec for TransactionMeta {
    fn from_xdr_base64(xdr_base64: &str) -> PrismResult<Self> {
        TransactionMeta::from_xdr_base64(xdr_base64, Limits::none()).map_err(|e|
            PrismError::XdrError(format!("Failed to decode TransactionMeta XDR: {e}"))
        )
    }

    fn to_xdr_base64(&self) -> PrismResult<String> {
        WriteXdr::to_xdr_base64(self, Limits::none()).map_err(|e|
            PrismError::XdrError(format!("Failed to encode TransactionMeta XDR: {e}"))
        )
    }
}

/// Decode a base64-encoded XDR transaction result.
///
/// # Arguments
/// * `xdr_base64` - Base64-encoded XDR string
///
/// # Returns
/// The raw decoded bytes, ready for further parsing.
pub fn decode_xdr_base64(xdr_base64: &str) -> PrismResult<Vec<u8>> {
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
    use std::convert::TryInto;
    use stellar_xdr::curr::{ LedgerEntryChanges, OperationMeta };

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

    #[test]
    fn test_transaction_meta_xdr_codec_roundtrip() {
        let meta = TransactionMeta::V0(
            Vec::<OperationMeta>::new()
                .try_into()
                .expect("empty operation list should fit in XDR VecM"),
        );

        let encoded = XdrCodec::to_xdr_base64(&meta).expect("encode transaction meta");
        let decoded = TransactionMeta::from_xdr_base64(&encoded).expect("decode transaction meta");

        assert_eq!(decoded, meta);
    }

    #[test]
    fn test_transaction_meta_xdr_codec_decodes_v1() {
        let meta = TransactionMeta::V1(stellar_xdr::curr::TransactionMetaV1 {
            tx_changes: LedgerEntryChanges::try_from(vec![])
                .expect("empty ledger entry changes should fit"),
            operations: Vec::<OperationMeta>::new()
                .try_into()
                .expect("empty operation list should fit"),
        });

        let encoded = XdrCodec::to_xdr_base64(&meta).expect("encode transaction meta");
        let decoded = <TransactionMeta as XdrCodec>::from_xdr_base64(&encoded)
            .expect("decode transaction meta");

        assert_eq!(decoded, meta);
    }
}
