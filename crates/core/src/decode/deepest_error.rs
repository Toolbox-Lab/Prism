//! # Deepest Error Finder
//!
//! Pinpoints the true root cause of a failed Soroban transaction when the
//! failure originates several layers deep inside a nested cross-contract call.
//!
//! ## Problem
//!
//! When a transaction executes nested sub-contract calls, the Soroban VM
//! emits diagnostic events sequentially. A single failure in a deep
//! sub-contract triggers a cascade of "Trapped" / "Reverted" errors as the
//! execution stack unwinds back to the top-level contract. Naive tooling
//! that surfaces only the *first* error it sees ends up reporting the
//! generic, top-level "Transaction Failed" event — hiding the specific
//! overflow or missing-authorization failure that actually caused it,
//! several call layers down.
//!
//! ## Algorithm
//!
//! [`DeepestErrorFinder`] ingests the chronological list of [`DiagnosticEvent`]
//! wrappers exactly once (`O(n)`) and maintains an internal integer counter —
//! the **Call Depth** — that is:
//!
//! - **incremented** when a cross-contract call event (`fn_call`) is seen,
//! - **decremented** when the matching return event (`fn_return`) is seen.
//!
//! Whenever a failure event is broadcast, the finder compares the Call Depth
//! at that exact moment against the depth of the deepest failure retained so
//! far, and keeps a pointer to whichever event sits at the greatest depth.
//! Because the deepest frame on the stack is always the one that first
//! raised the fault — every frame above it is only ever propagating an error
//! it did not itself originate — isolating the maximum-depth failure event
//! mathematically isolates the primary catalyst, stripping away the
//! cascading noise from the unwind.
//!
//! Call-stack bookkeeping and failure detection reuse the exact same rules
//! as [`crate::decode::chain_analyzer::ChainAnalyzer`] (via crate-visible
//! helpers in that module) so the two analyses never disagree about what
//! counts as a call, a return, or a failure.

use serde::{Deserialize, Serialize};
use stellar_xdr::curr::{ContractEventBody, DiagnosticEvent, ScVal};

use super::chain_analyzer::{hash_to_strkey, is_failure, topic_to_string, StackFrame};

// ---------------------------------------------------------------------------
// Public output type
// ---------------------------------------------------------------------------

/// The precise root cause identified by [`DeepestErrorFinder`]: the
/// `ContractId` and (when decodable) the error code of the failure event
/// that occurred at the greatest call depth across the entire event cascade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepestError {
    /// Strkey-encoded contract address (`C...`) of the frame that was
    /// executing when this failure was emitted.
    pub contract_address: String,

    /// The function name of that frame, if known from a preceding `fn_call`.
    pub function_name: Option<String>,

    /// Call depth at which this failure occurred (`0` = top-level).
    pub depth: usize,

    /// Contract error code extracted from an `ScVal::Error(ScError::Contract(_))`
    /// payload, when the failing event carries one. `None` when the failure
    /// was signalled some other way (e.g. `in_successful_contract_call =
    /// false` with a non-error payload, or a keyword topic like `"panic"`).
    pub error_code: Option<u32>,
}

// ---------------------------------------------------------------------------
// DeepestErrorFinder
// ---------------------------------------------------------------------------

/// Pinpoints the true root cause of a failed transaction by walking the
/// *entire* diagnostic event cascade and finding the failure event that
/// occurred at the maximum call depth.
///
/// A shallow, first-seen-wins strategy stops at the first failure indicator,
/// which is often the generic top-level "Transaction Failed" event.
/// `DeepestErrorFinder` instead scans every event with an explicit call-depth
/// counter and keeps only the failure with the greatest depth — the most
/// specific error in the cascade.
///
/// The finder is stateless; create a new instance with
/// [`DeepestErrorFinder::new`] and call [`DeepestErrorFinder::find_deepest`]
/// as many times as needed.
pub struct DeepestErrorFinder;

impl DeepestErrorFinder {
    /// Create a new finder instance.
    pub fn new() -> Self {
        Self
    }

    /// Walk `events` end-to-end and return the [`DeepestError`] — the
    /// failure indicator found at the greatest call depth — or `None` when
    /// the event sequence contains no failure indicators at all.
    ///
    /// Every event is inspected so that a deeper, more specific failure
    /// occurring later in the cascade is preferred over an earlier,
    /// shallower one. Among failures tied at the same depth, the
    /// first-encountered one is kept.
    pub fn find_deepest(&self, events: &[DiagnosticEvent]) -> Option<DeepestError> {
        // The live call stack, mirroring the Soroban host's execution stack.
        // `stack.len()` doubles as the "Call Depth" integer counter: it goes
        // up by one on every `fn_call` and down by one on every `fn_return`.
        let mut stack: Vec<StackFrame> = Vec::new();

        // Pointer to the deepest failure event seen so far.
        let mut deepest: Option<DeepestError> = None;

        for event in events {
            let ContractEventBody::V0(v0) = &event.event.body;

            let topics: Vec<String> = v0.topics.iter().filter_map(topic_to_string).collect();
            let first_topic = topics.first().map(String::as_str);

            // ----------------------------------------------------------------
            // Call Depth counter: increment on call, decrement on return.
            // ----------------------------------------------------------------
            match first_topic {
                Some("fn_call") => {
                    let address = match &event.event.contract_id {
                        Some(hash) => hash_to_strkey(hash),
                        // Host-level fn_call events sometimes lack a
                        // contract_id; fall back to the callee address hint
                        // carried as the second topic.
                        None => topics
                            .get(1)
                            .cloned()
                            .unwrap_or_else(|| "<unknown>".to_string()),
                    };
                    let function_name = topics.get(1).cloned();
                    stack.push(StackFrame {
                        contract_address: address,
                        function_name,
                        depth: stack.len(),
                    });
                    continue;
                }
                Some("fn_return") => {
                    stack.pop();
                    continue;
                }
                _ => {}
            }

            // ----------------------------------------------------------------
            // Evaluate every failure event, not just the first.
            // ----------------------------------------------------------------
            if is_failure(event, &topics, &v0.data) {
                // The depth of the frame that was active (innermost on the
                // stack) when the failure fired — 0-indexed, matching the
                // `StackFrame::depth` convention (outermost call = 0).
                let depth = stack.last().map_or(0, |f| f.depth);

                let is_new_deepest = match &deepest {
                    None => true,
                    // Strictly greater so that, among equally deep failures,
                    // the first-encountered one is kept.
                    Some(current) => depth > current.depth,
                };

                if is_new_deepest {
                    let (contract_address, function_name) = if let Some(frame) = stack.last() {
                        (frame.contract_address.clone(), frame.function_name.clone())
                    } else {
                        let address = event
                            .event
                            .contract_id
                            .as_ref()
                            .map_or_else(|| "<unknown>".to_string(), hash_to_strkey);
                        (address, None)
                    };

                    deepest = Some(DeepestError {
                        contract_address,
                        function_name,
                        depth,
                        error_code: Self::extract_error_code(&v0.data),
                    });
                }
            }
        }

        deepest
    }

    /// Extract a contract error code from an `ScVal::Error` payload.
    /// Returns `None` for host-level errors or any non-error payload.
    fn extract_error_code(data: &ScVal) -> Option<u32> {
        match data {
            ScVal::Error(stellar_xdr::curr::ScError::Contract(code)) => Some(*code),
            _ => None,
        }
    }
}

impl Default for DeepestErrorFinder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Convenience free function
// ---------------------------------------------------------------------------

/// Find the deepest error in `events` using a default [`DeepestErrorFinder`].
pub fn find_deepest_error(events: &[DiagnosticEvent]) -> Option<DeepestError> {
    DeepestErrorFinder::new().find_deepest(events)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_strkey::Contract as StrkeyContract;
    use stellar_xdr::curr::{
        ContractEvent, ContractEventBody, ContractEventType, ContractEventV0, ExtensionPoint, Hash,
        ScSymbol, ScVal,
    };

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn contract_hash(seed: u8) -> Hash {
        Hash([seed; 32])
    }

    fn strkey(seed: u8) -> String {
        StrkeyContract(contract_hash(seed).0).to_string()
    }

    fn make_event(
        contract_id: Option<Hash>,
        topics: Vec<ScVal>,
        data: ScVal,
        in_successful_contract_call: bool,
    ) -> DiagnosticEvent {
        DiagnosticEvent {
            in_successful_contract_call,
            event: ContractEvent {
                ext: ExtensionPoint::V0,
                contract_id,
                type_: ContractEventType::Diagnostic,
                body: ContractEventBody::V0(ContractEventV0 {
                    topics: topics.try_into().expect("topics VecM"),
                    data,
                }),
            },
        }
    }

    fn sym(s: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(s.try_into().expect("symbol string")))
    }

    fn fn_call(contract: Hash, function: &str) -> DiagnosticEvent {
        make_event(
            Some(contract),
            vec![sym("fn_call"), sym(function)],
            ScVal::Void,
            true,
        )
    }

    fn fn_return(contract: Hash) -> DiagnosticEvent {
        make_event(Some(contract), vec![sym("fn_return")], ScVal::Void, true)
    }

    fn error_event(contract: Hash, msg: &str) -> DiagnosticEvent {
        make_event(
            Some(contract),
            vec![sym("error"), sym(msg)],
            ScVal::Void,
            false,
        )
    }

    fn contract_error(contract: Hash, code: u32) -> DiagnosticEvent {
        make_event(
            Some(contract),
            vec![sym("error")],
            ScVal::Error(stellar_xdr::curr::ScError::Contract(code)),
            true,
        )
    }

    // -----------------------------------------------------------------------
    // Empty / no-failure inputs
    // -----------------------------------------------------------------------

    #[test]
    fn find_deepest_returns_none_for_no_events() {
        assert!(DeepestErrorFinder::new().find_deepest(&[]).is_none());
    }

    #[test]
    fn find_deepest_returns_none_when_no_failures() {
        let events = vec![
            fn_call(contract_hash(1), "swap"),
            fn_return(contract_hash(1)),
        ];
        assert!(DeepestErrorFinder::new().find_deepest(&events).is_none());
    }

    #[test]
    fn find_deepest_returns_none_when_all_calls_return_successfully() {
        let (h1, h2, h3) = (contract_hash(1), contract_hash(2), contract_hash(3));
        let events = vec![
            fn_call(h1.clone(), "route"),
            fn_call(h2.clone(), "swap"),
            fn_call(h3.clone(), "transfer"),
            fn_return(h3.clone()),
            fn_return(h2.clone()),
            fn_return(h1.clone()),
        ];
        assert!(DeepestErrorFinder::new().find_deepest(&events).is_none());
    }

    // -----------------------------------------------------------------------
    // Single-level failure
    // -----------------------------------------------------------------------

    #[test]
    fn single_contract_failure_is_reported_at_depth_zero() {
        let h1 = contract_hash(1);
        let events = vec![
            fn_call(h1.clone(), "transfer"),
            error_event(h1.clone(), "insufficient balance"),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.contract_address, strkey(1));
        assert_eq!(deepest.function_name.as_deref(), Some("transfer"));
        assert_eq!(deepest.depth, 0);
    }

    // -----------------------------------------------------------------------
    // Multi-level nested failure — the core scenario from the issue:
    // Router -> LiquidityPool -> Token, with the true root cause 3 layers
    // deep behind a cascade of unwinding "Trapped"/"Reverted" noise.
    // -----------------------------------------------------------------------

    #[test]
    fn deepest_failure_wins_over_shallow_generic_failure() {
        let (router, pool, token) = (contract_hash(1), contract_hash(2), contract_hash(3));

        let events = vec![
            fn_call(router.clone(), "route"),
            fn_call(pool.clone(), "swap"),
            fn_call(token.clone(), "transfer"),
            // Deep, specific failure — the actual root cause.
            contract_error(token.clone(), 7),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.contract_address, strkey(3));
        assert_eq!(deepest.function_name.as_deref(), Some("transfer"));
        assert_eq!(deepest.depth, 2);
        assert_eq!(deepest.error_code, Some(7));
    }

    #[test]
    fn four_level_nested_failure_pinpoints_the_deepest_layer() {
        let (router, pool, vault, token) = (
            contract_hash(1),
            contract_hash(2),
            contract_hash(3),
            contract_hash(4),
        );

        let events = vec![
            fn_call(router.clone(), "route"),
            fn_call(pool.clone(), "swap"),
            fn_call(vault.clone(), "withdraw"),
            fn_call(token.clone(), "transfer"),
            // Mathematical overflow, 4 layers deep.
            contract_error(token.clone(), 11),
            // Cascading unwind noise as the stack unwinds back up.
            error_event(vault.clone(), "Trapped"),
            error_event(pool.clone(), "Reverted"),
            error_event(router.clone(), "Transaction Failed"),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.contract_address, strkey(4));
        assert_eq!(deepest.depth, 3);
        assert_eq!(deepest.error_code, Some(11));
    }

    #[test]
    fn find_deepest_prefers_deeper_of_two_failures_regardless_of_order() {
        let (h1, h2, h3) = (contract_hash(10), contract_hash(20), contract_hash(30));

        let events = vec![
            fn_call(h1.clone(), "entry"),
            // Shallow failure at depth 0 (does not unwind the stack).
            contract_error(h1.clone(), 1),
            fn_call(h2.clone(), "middle"),
            fn_call(h3.clone(), "inner"),
            // Deeper failure at depth 2.
            contract_error(h3.clone(), 99),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.contract_address, strkey(30));
        assert_eq!(deepest.depth, 2);
        assert_eq!(deepest.error_code, Some(99));
    }

    // -----------------------------------------------------------------------
    // Depth counter correctly decrements on fn_return
    // -----------------------------------------------------------------------

    #[test]
    fn completed_frame_is_not_counted_when_later_failure_occurs() {
        let (h1, h2, h3) = (contract_hash(1), contract_hash(2), contract_hash(3));

        let events = vec![
            fn_call(h1.clone(), "outer"),
            fn_call(h2.clone(), "inner"),
            fn_return(h2.clone()), // depth counter back down to 1
            fn_call(h3.clone(), "another"),
            error_event(h3.clone(), "failed"),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.contract_address, strkey(3));
        assert_eq!(deepest.depth, 1);
    }

    #[test]
    fn find_deepest_keeps_first_when_tied_depth() {
        let (h1, h2) = (contract_hash(1), contract_hash(2));

        let events = vec![
            fn_call(h1.clone(), "a"),
            contract_error(h1.clone(), 1),
            fn_return(h1.clone()),
            fn_call(h2.clone(), "b"),
            contract_error(h2.clone(), 2),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        // Both failures are at depth 1; the first-encountered one wins.
        assert_eq!(deepest.contract_address, strkey(1));
        assert_eq!(deepest.error_code, Some(1));
    }

    // -----------------------------------------------------------------------
    // Recursive calls: same contract appears at multiple depths
    // -----------------------------------------------------------------------

    #[test]
    fn recursive_call_does_not_loop_and_finds_deepest_recurrence() {
        let (h1, h2) = (contract_hash(10), contract_hash(20));

        let events = vec![
            fn_call(h1.clone(), "entry"),
            fn_call(h2.clone(), "helper"),
            fn_call(h1.clone(), "callback"), // h1 recurses at depth 2
            error_event(h1.clone(), "panic"),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.contract_address, strkey(10));
        assert_eq!(deepest.depth, 2);
        assert_eq!(deepest.function_name.as_deref(), Some("callback"));
    }

    // -----------------------------------------------------------------------
    // Error code extraction
    // -----------------------------------------------------------------------

    #[test]
    fn find_deepest_returns_none_error_code_for_non_error_payload() {
        let h1 = contract_hash(5);
        let events = vec![fn_call(h1.clone(), "go"), error_event(h1.clone(), "panic")];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.error_code, None);
        assert_eq!(deepest.contract_address, strkey(5));
    }

    #[test]
    fn scval_error_payload_triggers_failure_detection() {
        let h1 = contract_hash(7);
        let events = vec![
            fn_call(h1.clone(), "call"),
            make_event(
                Some(h1.clone()),
                vec![sym("status")],
                ScVal::Error(stellar_xdr::curr::ScError::Contract(42)),
                true,
            ),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.error_code, Some(42));
    }

    #[test]
    fn unsuccessful_call_flag_triggers_failure_detection() {
        let h1 = contract_hash(8);
        let events = vec![
            fn_call(h1.clone(), "execute"),
            make_event(Some(h1.clone()), vec![sym("transfer")], ScVal::Void, false),
        ];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.contract_address, strkey(8));
        assert_eq!(deepest.error_code, None);
    }

    // -----------------------------------------------------------------------
    // Failure before any fn_call
    // -----------------------------------------------------------------------

    #[test]
    fn find_deepest_handles_failure_before_any_fn_call() {
        let h1 = contract_hash(9);
        let events = vec![contract_error(h1.clone(), 42)];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.depth, 0);
        assert_eq!(deepest.contract_address, strkey(9));
        assert_eq!(deepest.error_code, Some(42));
    }

    #[test]
    fn find_deepest_handles_failure_with_no_contract_id_and_empty_stack() {
        let events = vec![make_event(None, vec![sym("panic")], ScVal::Void, false)];

        let deepest = DeepestErrorFinder::new()
            .find_deepest(&events)
            .expect("deepest error should be found");

        assert_eq!(deepest.contract_address, "<unknown>");
        assert_eq!(deepest.depth, 0);
    }

    // -----------------------------------------------------------------------
    // Convenience free function / Default impl
    // -----------------------------------------------------------------------

    #[test]
    fn find_deepest_error_free_fn_matches_struct() {
        let h1 = contract_hash(6);
        let events = vec![fn_call(h1.clone(), "go"), contract_error(h1.clone(), 3)];

        let via_fn = find_deepest_error(&events);
        let via_struct = DeepestErrorFinder::new().find_deepest(&events);
        assert_eq!(via_fn, via_struct);
    }

    #[test]
    fn deepest_error_finder_default_behaves_like_new() {
        let h1 = contract_hash(4);
        let events = vec![contract_error(h1.clone(), 1)];
        let a = DeepestErrorFinder::new().find_deepest(&events);
        let b = DeepestErrorFinder::default().find_deepest(&events);
        assert_eq!(a, b);
    }
}
