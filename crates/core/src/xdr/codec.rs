//! XDR codec — thin wrapper over `stellar-xdr` with convenience methods.
//!
//! Handles serialization/deserialization of transaction envelopes, results,
//! ledger entries, SCVal, and SCSpecEntry types.

use crate::types::error::{PrismError, PrismResult};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use stellar_xdr::curr::{Limits, ReadXdr, TransactionEnvelope, WriteXdr};

pub trait XdrCodec: Sized {
    fn to_xdr_bytes(&self) -> PrismResult<Vec<u8>>;
    fn from_xdr_bytes(bytes: &[u8]) -> PrismResult<Self>;
}

impl XdrCodec for TransactionEnvelope {
    fn to_xdr_bytes(&self) -> PrismResult<Vec<u8>> {
        self.to_xdr(Limits::none())
            .map_err(|e| PrismError::XdrError(format!("Failed to encode TransactionEnvelope: {e}")))
    }

    fn from_xdr_bytes(bytes: &[u8]) -> PrismResult<Self> {
        TransactionEnvelope::from_xdr(bytes, Limits::none())
            .map_err(|e| PrismError::XdrError(format!("Failed to decode TransactionEnvelope: {e}")))
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
    let bytes = base64_decode(xdr_base64)
        .map_err(|e| PrismError::XdrError(format!("Base64 decode failed: {e}")))?;
    Ok(bytes)
}

/// Encode bytes to base64 XDR representation.
pub fn encode_xdr_base64(bytes: &[u8]) -> String {
    base64_encode(bytes)
}

/// Encode a TransactionEnvelope into XDR bytes.
pub fn encode_transaction_envelope(envelope: &TransactionEnvelope) -> PrismResult<Vec<u8>> {
    envelope
        .to_xdr(Limits::none())
        .map_err(|e| PrismError::XdrError(format!("Failed to encode TransactionEnvelope: {e}")))
}

/// Decode a TransactionEnvelope from XDR bytes.
pub fn decode_transaction_envelope(bytes: &[u8]) -> PrismResult<TransactionEnvelope> {
    TransactionEnvelope::from_xdr(bytes, Limits::none())
        .map_err(|e| PrismError::XdrError(format!("Failed to decode TransactionEnvelope: {e}")))
}

/// Encode a TransactionEnvelope into base64 XDR.
pub fn encode_transaction_envelope_base64(envelope: &TransactionEnvelope) -> PrismResult<String> {
    let bytes = encode_transaction_envelope(envelope)?;
    Ok(encode_xdr_base64(&bytes))
}

/// Decode a TransactionEnvelope from base64 XDR.
pub fn decode_transaction_envelope_base64(xdr_base64: &str) -> PrismResult<TransactionEnvelope> {
    let bytes = decode_xdr_base64(xdr_base64)?;
    decode_transaction_envelope(&bytes)
}

/// Decode a transaction hash from hex string.
pub fn decode_tx_hash(hash_hex: &str) -> PrismResult<[u8; 32]> {
    let bytes = hex_decode(hash_hex)
        .map_err(|e| PrismError::XdrError(format!("Invalid tx hash hex: {e}")))?;
    if bytes.len() != 32 {
        return Err(PrismError::XdrError(format!(
            "Transaction hash must be 32 bytes, got {}",
            bytes.len()
        )));
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
    if !input.len().is_multiple_of(2) {
        return Err("Hex input must have an even length".to_string());
    }

    (0..input.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&input[i..i + 2], 16)
                .map_err(|e| format!("Invalid hex at position {i}: {e}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;
    use stellar_xdr::curr::{
        DecoratedSignature, Memo, MuxedAccount, Operation, Preconditions, SequenceNumber,
        Transaction, TransactionEnvelope, TransactionExt, TransactionV1Envelope, Uint256, VecM,
    };

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
    fn test_decode_tx_hash_invalid_hex() {
        let result = decode_tx_hash(&"z".repeat(64));
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
    fn test_transaction_envelope_round_trip_bytes() {
        let envelope = make_test_transaction_envelope();

        let encoded =
            encode_transaction_envelope(&envelope).expect("Failed to encode TransactionEnvelope");

        let decoded =
            decode_transaction_envelope(&encoded).expect("Failed to decode TransactionEnvelope");

        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_transaction_envelope_round_trip_base64() {
        let envelope = make_test_transaction_envelope();

        let base64_string = encode_transaction_envelope_base64(&envelope)
            .expect("Failed to encode TransactionEnvelope to base64");

        let decoded = decode_transaction_envelope_base64(&base64_string)
            .expect("Failed to decode TransactionEnvelope from base64");

        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_transaction_envelope_decode_invalid_bytes() {
        let result = decode_transaction_envelope(&[1, 2, 3, 4]);
        assert!(result.is_err());
    }

    #[test]
    fn test_xdr_codec_trait_round_trip() {
        let envelope = make_test_transaction_envelope();

        let encoded = envelope
            .to_xdr_bytes()
            .expect("Failed to encode with XdrCodec");
        let decoded =
            TransactionEnvelope::from_xdr_bytes(&encoded).expect("Failed to decode with XdrCodec");

        assert_eq!(envelope, decoded);
    }

    fn make_test_transaction_envelope() -> TransactionEnvelope {
        let operations: VecM<Operation, 100> = Vec::<Operation>::new()
            .try_into()
            .expect("empty operations should fit");

        let signatures: VecM<DecoratedSignature, 20> = Vec::<DecoratedSignature>::new()
            .try_into()
            .expect("empty signatures should fit");

        TransactionEnvelope::Tx(TransactionV1Envelope {
            tx: Transaction {
                source_account: MuxedAccount::Ed25519(Uint256([0; 32])),
                fee: 100,
                seq_num: SequenceNumber(1),
                cond: Preconditions::None,
                memo: Memo::None,
                operations,
                ext: TransactionExt::V0,
            },
            signatures,
        })
    }
}
