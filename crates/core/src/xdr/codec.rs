//! XDR codec — thin wrapper over `stellar-xdr` with convenience methods.
//!
//! Handles serialization/deserialization of transaction envelopes, results,
//! ledger entries, SCVal, and SCSpecEntry types.

use crate::types::error::{PrismError, PrismResult};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use stellar_xdr::{ReadXdr, WriteXdr, Limits, TransactionMeta, TransactionMetaV1};

/// Trait for types that can be encoded/decoded to/from XDR.
pub trait XdrCodec: Sized {
    /// Decode from XDR bytes.
    fn from_xdr_bytes(bytes: &[u8]) -> PrismResult<Self>;

    /// Encode to XDR bytes.
    fn to_xdr_bytes(&self) -> PrismResult<Vec<u8>>;

    /// Decode from base64-encoded XDR string.
    fn from_xdr_base64(base64: &str) -> PrismResult<Self> {
        let bytes = decode_xdr_base64(base64)?;
        Self::from_xdr_bytes(&bytes)
    }

    /// Encode to base64-encoded XDR string.
    fn to_xdr_base64(&self) -> PrismResult<String> {
        let bytes = self.to_xdr_bytes()?;
        Ok(encode_xdr_base64(&bytes))
    }
}

impl XdrCodec for TransactionMeta {
    fn from_xdr_bytes(bytes: &[u8]) -> PrismResult<Self> {
        TransactionMeta::from_xdr(bytes, Limits::none())
            .map_err(|e| PrismError::XdrError(format!("Failed to decode TransactionMeta: {e}")))
    }

    fn to_xdr_bytes(&self) -> PrismResult<Vec<u8>> {
        self.to_xdr(Limits::none())
            .map_err(|e| PrismError::XdrError(format!("Failed to encode TransactionMeta: {e}")))
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
    use stellar_xdr::ReadXdr;
    use stellar_xdr::WriteXdr;

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
    fn test_transaction_result_round_trip() {
        // Create a simple TransactionResult with success code
        let tx_result = stellar_xdr::TransactionResult {
            fee_charged: 100,
            result: stellar_xdr::TransactionResultResult::TxSuccess,
            ext: stellar_xdr::TransactionResultExt::V0,
        };

        // Encode to XDR bytes
        let encoded = tx_result
            .to_xdr(stellar_xdr::Limits::none())
            .expect("Failed to encode TransactionResult");

        // Decode back from XDR bytes
        let decoded =
            stellar_xdr::TransactionResult::from_xdr(&encoded, stellar_xdr::Limits::none())
                .expect("Failed to decode TransactionResult");

        // Verify round-trip produces identical result
        assert_eq!(tx_result, decoded);
    }

    #[test]
    fn test_transaction_result_round_trip_with_error() {
        // Create a TransactionResult with an error code
        let tx_result = stellar_xdr::TransactionResult {
            fee_charged: 50,
            result: stellar_xdr::TransactionResultResult::TxFeeBumpInnerSuccess,
            ext: stellar_xdr::TransactionResultExt::V0,
        };

        // Encode to XDR bytes
        let encoded = tx_result
            .to_xdr(stellar_xdr::Limits::none())
            .expect("Failed to encode TransactionResult");

        // Decode back from XDR bytes
        let decoded =
            stellar_xdr::TransactionResult::from_xdr(&encoded, stellar_xdr::Limits::none())
                .expect("Failed to decode TransactionResult");

        // Verify round-trip produces identical result
        assert_eq!(tx_result, decoded);
    }

    #[test]
    fn test_transaction_result_round_trip_base64() {
        // Create a TransactionResult
        let tx_result = stellar_xdr::TransactionResult {
            fee_charged: 200,
            result: stellar_xdr::TransactionResultResult::TxSuccess,
            ext: stellar_xdr::TransactionResultExt::V0,
        };

        // Encode to XDR bytes
        let encoded_bytes = tx_result
            .to_xdr(stellar_xdr::Limits::none())
            .expect("Failed to encode TransactionResult");

        // Convert to base64 using our codec
        let base64_string = encode_xdr_base64(&encoded_bytes);

        // Decode base64 back to bytes using our codec
        let decoded_bytes = decode_xdr_base64(&base64_string).expect("Failed to decode base64");

        // Verify bytes match
        assert_eq!(encoded_bytes, decoded_bytes);

        // Decode back to TransactionResult
        let decoded_result =
            stellar_xdr::TransactionResult::from_xdr(&decoded_bytes, stellar_xdr::Limits::none())
                .expect("Failed to decode TransactionResult from bytes");

        // Verify round-trip produces identical result
        assert_eq!(tx_result, decoded_result);
    }

    #[test]
    fn test_transaction_envelope_round_trip() {
        use stellar_xdr::{
            Limits, Memo, MuxedAccount, Preconditions, ReadXdr, SequenceNumber, Transaction,
            TransactionEnvelope, TransactionExt, TransactionV1Envelope, Uint256, WriteXdr,
        };

        // Create a dummy TransactionV1Envelope
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([0; 32])),
            fee: 100,
            seq_num: SequenceNumber(1),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![].try_into().unwrap(),
            ext: TransactionExt::V0,
        };

        let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: vec![].try_into().unwrap(),
        });

        // Encode to XDR bytes
        let encoded = envelope
            .to_xdr(Limits::none())
            .expect("Failed to encode TransactionEnvelope");

        // Decode back from XDR bytes
        let decoded = TransactionEnvelope::from_xdr(&encoded, Limits::none())
            .expect("Failed to decode TransactionEnvelope");

        // Verify round-trip produces identical result
        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_transaction_envelope_round_trip_base64() {
        use stellar_xdr::{
            Limits, Memo, MuxedAccount, Preconditions, ReadXdr, SequenceNumber, Transaction,
            TransactionEnvelope, TransactionExt, TransactionV1Envelope, Uint256, WriteXdr,
        };

        // Create a TransactionEnvelope
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([1; 32])),
            fee: 500,
            seq_num: SequenceNumber(42),
            cond: Preconditions::None,
            memo: Memo::Text("test memo".as_bytes().try_into().unwrap()),
            operations: vec![].try_into().unwrap(),
            ext: TransactionExt::V0,
        };

        let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: vec![].try_into().unwrap(),
        });

        // Encode to XDR bytes
        let encoded_bytes = envelope
            .to_xdr(Limits::none())
            .expect("Failed to encode TransactionEnvelope");

        // Convert to base64 using our codec
        let base64_string = encode_xdr_base64(&encoded_bytes);

        // Decode base64 back to bytes using our codec
        let decoded_bytes = decode_xdr_base64(&base64_string).expect("Failed to decode base64");

        // Verify bytes match
        assert_eq!(encoded_bytes, decoded_bytes);

        // Decode back to TransactionEnvelope
        let decoded_envelope = TransactionEnvelope::from_xdr(&decoded_bytes, Limits::none())
            .expect("Failed to decode TransactionEnvelope from bytes");

        // Verify round-trip produces identical result
        assert_eq!(envelope, decoded_envelope);
    }

    #[test]
    fn test_fee_bump_transaction_envelope_round_trip() {
        use stellar_xdr::{
            FeeBumpTransaction, FeeBumpTransactionEnvelope, FeeBumpTransactionExt,
            FeeBumpTransactionInnerTx, Limits, Memo, MuxedAccount, Preconditions, ReadXdr,
            SequenceNumber, Transaction, TransactionEnvelope, TransactionExt,
            TransactionV1Envelope, Uint256, WriteXdr,
        };

        // Create an inner TransactionV1Envelope
        let inner_tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([0; 32])),
            fee: 100,
            seq_num: SequenceNumber(1),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![].try_into().unwrap(),
            ext: TransactionExt::V0,
        };

        let inner_envelope = TransactionV1Envelope {
            tx: inner_tx,
            signatures: vec![].try_into().unwrap(),
        };

        // Create FeeBumpTransaction
        let fee_bump = FeeBumpTransaction {
            fee_source: MuxedAccount::Ed25519(Uint256([1; 32])),
            fee: 200,
            inner_tx: FeeBumpTransactionInnerTx::Tx(inner_envelope),
            ext: FeeBumpTransactionExt::V0,
        };

        let envelope = TransactionEnvelope::FeeBump(FeeBumpTransactionEnvelope {
            tx: fee_bump,
            signatures: vec![].try_into().unwrap(),
        });

        // Encode to XDR bytes
        let encoded = envelope
            .to_xdr(Limits::none())
            .expect("Failed to encode FeeBumpTransactionEnvelope");

        // Decode back from XDR bytes
        let decoded = TransactionEnvelope::from_xdr(&encoded, Limits::none())
            .expect("Failed to decode FeeBumpTransactionEnvelope");

        // Verify round-trip produces identical result
        assert_eq!(envelope, decoded);
    }
}

#[cfg(test)]
mod test_xdr_codec {
    use super::*;
    use stellar_xdr::TransactionMeta;

    #[test]
    fn test_transaction_meta_xdr_codec() {
        // Create a simple TransactionMeta with V1 containing empty changes
        // TransactionMeta contains ledger changes made during transaction execution
        let meta = TransactionMeta::V1(TransactionMetaV1 {
            tx_changes: vec![].try_into().unwrap(),
            operations: vec![].try_into().unwrap(),
        });

        // Test encoding to bytes
        let bytes = meta.to_xdr_bytes().expect("Failed to encode TransactionMeta");
        assert!(!bytes.is_empty());

        // Test decoding from bytes
        let decoded = TransactionMeta::from_xdr_bytes(&bytes).expect("Failed to decode TransactionMeta");
        assert_eq!(meta, decoded);

        // Test encoding to base64
        let base64 = meta.to_xdr_base64().expect("Failed to encode to base64");
        assert!(!base64.is_empty());

        // Test decoding from base64
        let decoded_from_base64 = TransactionMeta::from_xdr_base64(&base64).expect("Failed to decode from base64");
        assert_eq!(meta, decoded_from_base64);
    }
}
