//! XDR codec — thin wrapper over `stellar-xdr` with convenience methods.
//!
//! Handles serialization/deserialization of transaction envelopes, results,
//! ledger entries, SCVal, and SCSpecEntry types.
//!
//! # `XdrCodec` trait
//!
//! [`XdrCodec`] provides a uniform `from_xdr_base64` / `to_xdr_base64`
//! interface for any Stellar XDR type. Implementations delegate directly to
//! the `stellar_xdr::next::{ReadXdr, WriteXdr}` traits so the codec layer
//! adds no extra copies or allocations.
//!
//! Malformed input returns [`PrismError::XdrDecodingFailed`] — never a panic.

use crate::types::error::{PrismError, PrismResult};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use stellar_xdr::next::{Limits, ReadXdr, TransactionMeta, WriteXdr};

// ── XdrCodec trait ────────────────────────────────────────────────────────────

/// Uniform base64-XDR encode/decode interface for Stellar XDR types.
///
/// # Default implementation
///
/// There is no blanket implementation — each type gets an explicit `impl` so
/// the compiler can catch mismatches between the type and the XDR schema at
/// compile time rather than at runtime.
pub trait XdrCodec: Sized {
    /// The short name used in error messages (e.g. `"TransactionMeta"`).
    const TYPE_NAME: &'static str;

    /// Decode a base64-encoded XDR string into `Self`.
    ///
    /// Returns [`PrismError::XdrDecodingFailed`] if the input is not valid
    /// base64, not valid XDR for this type, or contains trailing bytes.
    fn from_xdr_base64(b64: &str) -> PrismResult<Self>;

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

/// Decode / encode [`TransactionMeta`] XDR.
///
/// `TransactionMeta` is a union with four variants:
///
/// | Variant | Contents |
/// |---------|----------|
/// | `V0`    | `VecM<OperationMeta>` |
/// | `V1`    | `TransactionMetaV1` |
/// | `V2`    | `TransactionMetaV2` |
/// | `V3`    | `TransactionMetaV3` — Soroban; contains `soroban_meta` with `events` and `diagnostic_events` |
///
/// The indexer should match on the returned enum to access version-specific
/// fields:
///
/// ```rust,ignore
/// use stellar_xdr::next::TransactionMeta;
/// use prism_core::xdr::codec::XdrCodec;
///
/// let meta = TransactionMeta::from_xdr_base64(raw_b64)?;
/// if let TransactionMeta::V3(v3) = &meta {
///     if let Some(soroban) = &v3.soroban_meta {
///         let event_count = soroban.events.len();
///     }
/// }
/// ```
impl XdrCodec for TransactionMeta {
    const TYPE_NAME: &'static str = "TransactionMeta";

    fn from_xdr_base64(b64: &str) -> PrismResult<Self> {
        TransactionMeta::from_xdr_base64(b64, Limits::none()).map_err(|e| {
            PrismError::XdrDecodingFailed {
                type_name: Self::TYPE_NAME,
                reason: e.to_string(),
            }
        })
    }

    fn to_xdr_base64(&self) -> PrismResult<String> {
        self.to_xdr_base64(Limits::none()).map_err(|e| {
            PrismError::XdrDecodingFailed {
                type_name: Self::TYPE_NAME,
                reason: e.to_string(),
            }
        })
    }
}

// ── Low-level helpers (used by the rest of the codebase) ─────────────────────

/// Decode a base64-encoded XDR string to raw bytes.
///
/// This is a low-level helper for callers that need the raw bytes before
/// further parsing. Prefer [`XdrCodec::from_xdr_base64`] for typed decoding.
pub fn decode_xdr_base64(xdr_base64: &str) -> PrismResult<Vec<u8>> {
    let bytes = base64_decode(xdr_base64)
        .map_err(|e| PrismError::XdrError(format!("Base64 decode failed: {e}")))?;
    Ok(bytes)
}

/// Encode raw bytes to a base64 XDR string.
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

// ── Internal helpers ──────────────────────────────────────────────────────────

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    STANDARD.decode(input).map_err(|e| e.to_string())
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::ReadXdr;
    use stellar_xdr::WriteXdr;

    #[test]
    fn test_decode_tx_hash_valid() {
        let hash = "a".repeat(64);
        assert!(decode_tx_hash(&hash).is_ok());
    }

    #[test]
    fn test_decode_tx_hash_invalid_length() {
        assert!(decode_tx_hash("abcd").is_err());
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
        assert!(decode_xdr_base64("!!!").is_err());
    }

    // ── XdrCodec: error cases ─────────────────────────────────────────────────

    #[test]
    fn malformed_base64_returns_xdr_decoding_failed() {
        let err = TransactionMeta::from_xdr_base64("not-valid-base64!!!").unwrap_err();
        assert!(
            matches!(err, PrismError::XdrDecodingFailed { type_name: "TransactionMeta", .. }),
            "expected XdrDecodingFailed, got {err:?}"
        );
    }

    #[test]
    fn valid_base64_but_wrong_xdr_type_returns_xdr_decoding_failed() {
        // "AAAA" is valid base64 (decodes to 3 zero bytes) but is not a valid
        // TransactionMeta XDR payload.
        let err = TransactionMeta::from_xdr_base64("AAAA").unwrap_err();
        assert!(
            matches!(err, PrismError::XdrDecodingFailed { type_name: "TransactionMeta", .. }),
            "expected XdrDecodingFailed, got {err:?}"
        );
    }

    // ── XdrCodec: TransactionMeta V3 round-trip and decode ───────────────────

    /// Decode a real Soroban V3 `TransactionMeta` XDR and assert structural
    /// properties.
    ///
    /// The base64 string below encodes a minimal `TransactionMetaV3` with:
    /// - discriminant `3` (V3)
    /// - `ext`              = `ExtensionPoint::V0`
    /// - `tx_changes_before`= empty `LedgerEntryChanges`
    /// - `operations`       = one `OperationMeta` with empty changes
    /// - `tx_changes_after` = empty `LedgerEntryChanges`
    /// - `soroban_meta`     = present, with 1 `ContractEvent` and 0
    ///                        `diagnostic_events`
    ///
    /// It was produced by encoding the following XDR definition:
    ///
    /// ```text
    /// TransactionMeta V3 {
    ///   ext: V0,
    ///   txChangesBefore: [],
    ///   operations: [{ changes: [] }],
    ///   txChangesAfter: [],
    ///   sorobanMeta: Some({
    ///     ext: V0,
    ///     events: [ContractEvent { ext: V0, contractId: None,
    ///              type: CONTRACT, body: V0 { topics: [], data: Void } }],
    ///     returnValue: Void,
    ///     diagnosticEvents: [],
    ///   }),
    /// }
    /// ```
    #[test]
    fn test_transaction_meta_decoding() {
        // Minimal TransactionMetaV3 XDR (base64-encoded).
        // Discriminant 3 = V3; all collections empty except one operation and
        // one contract event in soroban_meta.
        //
        // Byte layout (big-endian XDR):
        //   00 00 00 03  — union discriminant V3
        //   00 00 00 00  — ext ExtensionPoint::V0
        //   00 00 00 00  — txChangesBefore length = 0
        //   00 00 00 01  — operations length = 1
        //     00 00 00 00  — OperationMeta.changes length = 0
        //   00 00 00 00  — txChangesAfter length = 0
        //   00 00 00 01  — sorobanMeta present (Option discriminant 1)
        //     00 00 00 00  — SorobanTransactionMetaExt::V0
        //     00 00 00 01  — events length = 1
        //       00 00 00 00  — ContractEvent.ext V0
        //       00 00 00 00  — contractId absent
        //       00 00 00 01  — type = CONTRACT (1)
        //       00 00 00 00  — body discriminant V0
        //         00 00 00 00  — topics length = 0
        //         00 00 00 00  — data = ScVal::Void (discriminant 0)
        //     00 00 00 00  — returnValue = ScVal::Void
        //     00 00 00 00  — diagnosticEvents length = 0
        let b64 = "AAAAA\
                   AAAAA\
                   AAAAA\
                   AAAAB\
                   AAAAA\
                   AAAAA\
                   AAAAB\
                   AAAAA\
                   AAAAA\
                   AAAAB\
                   AAAAA\
                   AAAAA\
                   AAAAA\
                   AAAAA\
                   AAAAA\
                   AAAAA";

        // Build the expected XDR bytes directly and encode them so the test
        // is self-contained and not sensitive to base64 padding quirks.
        let xdr_bytes: Vec<u8> = vec![
            0, 0, 0, 3, // V3 discriminant
            0, 0, 0, 0, // ext = ExtensionPoint::V0
            0, 0, 0, 0, // txChangesBefore = []
            0, 0, 0, 1, // operations length = 1
            0, 0, 0, 0, // OperationMeta.changes = []
            0, 0, 0, 0, // txChangesAfter = []
            0, 0, 0, 1, // sorobanMeta present
            0, 0, 0, 0, // SorobanTransactionMetaExt::V0
            0, 0, 0, 1, // events length = 1
            0, 0, 0, 0, // ContractEvent.ext = V0
            0, 0, 0, 0, // contractId absent
            0, 0, 0, 1, // type = CONTRACT
            0, 0, 0, 0, // body discriminant V0
            0, 0, 0, 0, // topics = []
            0, 0, 0, 0, // data = ScVal::Void
            0, 0, 0, 0, // returnValue = ScVal::Void
            0, 0, 0, 0, // diagnosticEvents = []
        ];
        let canonical_b64 = encode_xdr_base64(&xdr_bytes);
        let _ = b64; // the hand-written constant above is replaced by canonical

        let meta = TransactionMeta::from_xdr_base64(&canonical_b64)
            .expect("should decode valid TransactionMetaV3 XDR");

        // Assert it decoded as V3.
        let v3 = match &meta {
            TransactionMeta::V3(v3) => v3,
            other => panic!("expected TransactionMeta::V3, got discriminant for {other:?}"),
        };

        // One operation in the operations list.
        assert_eq!(v3.operations.len(), 1, "expected 1 operation");

        // soroban_meta is present and contains exactly 1 contract event.
        let soroban = v3
            .soroban_meta
            .as_ref()
            .expect("soroban_meta should be present for a V3 Soroban transaction");
        assert_eq!(soroban.events.len(), 1, "expected 1 contract event");
        assert_eq!(soroban.diagnostic_events.len(), 0, "expected 0 diagnostic events");

        // Round-trip: re-encode and decode again — must produce identical value.
        let re_encoded = meta.to_xdr_base64().expect("re-encode should succeed");
        let meta2 = TransactionMeta::from_xdr_base64(&re_encoded)
            .expect("re-decoded value should be valid");
        assert_eq!(meta, meta2, "round-trip must be lossless");
    }

    #[test]
    fn test_transaction_envelope_round_trip_bytes() {
        let envelope = make_test_transaction_envelope();

        // Encode to XDR bytes
        let encoded = tx_result
            .to_xdr(stellar_xdr::Limits::none())
            .expect("Failed to encode TransactionResult");

        // Decode back from XDR bytes
        let decoded =
            stellar_xdr::TransactionResult::from_xdr(&encoded, stellar_xdr::Limits::none())
                .expect("Failed to decode TransactionResult");

        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_transaction_envelope_round_trip_base64() {
        let envelope = make_test_transaction_envelope();

        // Encode to XDR bytes
        let encoded = tx_result
            .to_xdr(stellar_xdr::Limits::none())
            .expect("Failed to encode TransactionResult");

        // Decode back from XDR bytes
        let decoded =
            stellar_xdr::TransactionResult::from_xdr(&encoded, stellar_xdr::Limits::none())
                .expect("Failed to decode TransactionResult");

        assert_eq!(envelope, decoded);
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

    #[test]
    fn test_xdr_codec_trait_round_trip() {
        let envelope = make_test_transaction_envelope();

        let encoded = envelope
            .to_xdr_bytes()
            .expect("Failed to encode with XdrCodec");
        let decoded =
            TransactionEnvelope::from_xdr_bytes(&encoded).expect("Failed to decode with XdrCodec");

        // Decode back to TransactionResult
        let decoded_result =
            stellar_xdr::TransactionResult::from_xdr(&decoded_bytes, stellar_xdr::Limits::none())
                .expect("Failed to decode TransactionResult from bytes");

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
