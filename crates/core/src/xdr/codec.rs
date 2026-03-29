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
use stellar_xdr::next::{Limits, ReadXdr, TransactionMeta, TransactionResult, WriteXdr};

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

    /// Encode `self` back to a base64 XDR string.
    ///
    /// Useful for round-trip testing and caching decoded values.
    fn to_xdr_base64(&self) -> PrismResult<String>;
}

// ── TransactionMeta ───────────────────────────────────────────────────────────

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

// ── TransactionResult ─────────────────────────────────────────────────────────

/// Decode / encode [`TransactionResult`] XDR.
///
/// `TransactionResult` is a struct wrapping:
/// - `fee_charged: i64` — actual fee deducted from the source account
/// - `result: TransactionResultResult` — the outcome union; key variants:
///
/// | Variant | Meaning |
/// |---------|---------|
/// | `TxSuccess(ops)` | All operations succeeded; `ops` holds per-op results |
/// | `TxFailed(ops)`  | One or more operations failed; `ops` holds per-op results |
/// | `TxInsufficientBalance` | Fee would drop account below minimum reserve |
/// | `TxBadSeq` | Sequence number mismatch |
/// | `TxInsufficientFee` | Submitted fee too low |
/// | `TxSorobanInvalid` | Soroban-specific precondition not met |
/// | *(other void variants)* | See [`stellar_xdr::next::TransactionResultResult`] |
///
/// # Example
///
/// ```rust,ignore
/// use stellar_xdr::next::{TransactionResult, TransactionResultResult};
/// use prism_core::xdr::codec::XdrCodec;
///
/// let result = TransactionResult::from_xdr_base64(raw_b64)?;
/// match &result.result {
///     TransactionResultResult::TxSuccess(ops) => { /* index ops */ }
///     TransactionResultResult::TxInsufficientBalance => { /* surface error */ }
///     _ => {}
/// }
/// ```
impl XdrCodec for TransactionResult {
    const TYPE_NAME: &'static str = "TransactionResult";

    fn from_xdr_base64(b64: &str) -> PrismResult<Self> {
        TransactionResult::from_xdr_base64(b64, Limits::none()).map_err(|e| {
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
    base64_decode(xdr_base64)
        .map_err(|e| PrismError::XdrError(format!("Base64 decode failed: {e}")))
}

/// Encode raw bytes to a base64 XDR string.
pub fn encode_xdr_base64(bytes: &[u8]) -> String {
    base64_encode(bytes)
}

/// Decode a transaction hash from a hex string into a 32-byte array.
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
    use stellar_xdr::next::TransactionMeta;

    // ── Existing helper tests ─────────────────────────────────────────────────

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

    // ── XdrCodec: TransactionResult ───────────────────────────────────────────

    /// Decode a successful `TransactionResult` XDR and assert structural
    /// properties.
    ///
    /// XDR byte layout (big-endian):
    /// ```text
    ///   00 00 00 00  00 00 00 64  — fee_charged = 100 (i64)
    ///   00 00 00 00              — result discriminant TxSuccess = 0
    ///   00 00 00 00              — TxSuccess: OperationResult vec length = 0
    ///   00 00 00 00              — ext discriminant V0
    /// ```
    #[test]
    fn test_transaction_result_success_decoding() {
        use stellar_xdr::next::{TransactionResult, TransactionResultResult};

        let xdr_bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 100, // fee_charged = 100 (i64 big-endian)
            0, 0, 0, 0,               // result discriminant: TxSuccess = 0
            0, 0, 0, 0,               // TxSuccess: empty OperationResult vec
            0, 0, 0, 0,               // ext: V0
        ];
        let b64 = encode_xdr_base64(&xdr_bytes);

        let result = TransactionResult::from_xdr_base64(&b64)
            .expect("should decode valid TxSuccess TransactionResult XDR");

        assert_eq!(result.fee_charged, 100, "fee_charged should be 100");
        assert!(
            matches!(result.result, TransactionResultResult::TxSuccess(_)),
            "expected TxSuccess, got {:?}",
            result.result
        );
        if let TransactionResultResult::TxSuccess(ops) = &result.result {
            assert_eq!(ops.len(), 0, "expected 0 operation results");
        }

        // Round-trip.
        let re_encoded = result.to_xdr_base64().expect("re-encode should succeed");
        let result2 = TransactionResult::from_xdr_base64(&re_encoded)
            .expect("re-decoded value should be valid");
        assert_eq!(result, result2, "round-trip must be lossless");
    }

    /// Decode a failed `TransactionResult` with code `TxInsufficientBalance`
    /// and assert the correct void variant is parsed.
    ///
    /// XDR byte layout (big-endian):
    /// ```text
    ///   00 00 00 00  00 00 00 64  — fee_charged = 100 (i64)
    ///   FF FF FF F9              — result discriminant TxInsufficientBalance = -7
    ///                            — (void body — no additional bytes)
    ///   00 00 00 00              — ext discriminant V0
    /// ```
    #[test]
    fn test_transaction_result_failure_decoding() {
        use stellar_xdr::next::{TransactionResult, TransactionResultResult};

        let xdr_bytes: Vec<u8> = vec![
            0, 0, 0, 0, 0, 0, 0, 100, // fee_charged = 100 (i64 big-endian)
            0xFF, 0xFF, 0xFF, 0xF9,   // result discriminant: TxInsufficientBalance = -7
                                      // void body — no bytes
            0, 0, 0, 0,               // ext: V0
        ];
        let b64 = encode_xdr_base64(&xdr_bytes);

        let result = TransactionResult::from_xdr_base64(&b64)
            .expect("should decode valid TxInsufficientBalance TransactionResult XDR");

        assert_eq!(result.fee_charged, 100, "fee_charged should be 100");
        assert!(
            matches!(result.result, TransactionResultResult::TxInsufficientBalance),
            "expected TxInsufficientBalance, got {:?}",
            result.result
        );

        // Round-trip.
        let re_encoded = result.to_xdr_base64().expect("re-encode should succeed");
        let result2 = TransactionResult::from_xdr_base64(&re_encoded)
            .expect("re-decoded value should be valid");
        assert_eq!(result, result2, "round-trip must be lossless");
    }
}
