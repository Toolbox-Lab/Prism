use serde::{Deserialize, Serialize};
use stellar_strkey::Contract as StrkeyContract;
use stellar_xdr::curr::{ContractEventBody, ContractEventType, DiagnosticEvent, Hash, ScVal};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticEventKind {
    Contract,

    System,

    Debug,

    Unknown,
}

impl DiagnosticEventKind {
    fn from_contract_event_type(t: ContractEventType) -> Self {
        match t {
            ContractEventType::Contract => Self::Contract,
            ContractEventType::System => Self::System,
            ContractEventType::Diagnostic => Self::Debug,
        }
    }
}

impl std::fmt::Display for DiagnosticEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract => write!(f, "Contract"),
            Self::System => write!(f, "System"),
            Self::Debug => write!(f, "Debug"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A fully-parsed, strongly-typed representation of a single Soroban
/// diagnostic event.
/// All fields are extracted from the raw XDR envelope during the walk.
/// No data is discarded — if the event body cannot be fully decoded the
/// [`Self::kind`] is set to [`DiagnosticEventKind::Unknown`] and the
/// [`Self::parse_error`] field carries the reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredDiagnosticEvent {
    /// Functional category of the event.
    pub kind: DiagnosticEventKind,

    /// Optional strkey-encoded contract identifier (`C…` Stellar address).
    /// `None` when the event was emitted by the host rather than a specific
    /// contract (common for [`DiagnosticEventKind::System`] events).
    pub contract_id: Option<String>,

    /// Ordered topic vector extracted from the event body.
    /// Always present (may be empty) for well-formed events.
    pub topics: Vec<ScVal>,

    /// Payload data from the event body.
    /// [`ScVal::Void`] when the body cannot be decoded.
    pub data: ScVal,

    /// Whether this event occurred within a *successful* contract call frame.
    /// Mapped 1-to-1 from [`DiagnosticEvent::in_successful_contract_call`].
    pub in_successful_call: bool,

    /// Human-readable parse error, set only when the XDR body is malformed.
    /// `None` for every well-formed event.
    pub parse_error: Option<String>,
}

impl StructuredDiagnosticEvent {
    /// Returns `true` when the event was emitted during a successful call and
    /// has no parse errors.
    pub fn is_healthy(&self) -> bool {
        self.in_successful_call && self.parse_error.is_none()
    }
}

// ---------------------------------------------------------------------------
// Walker
// ---------------------------------------------------------------------------

/// Walks a collection of raw [`DiagnosticEvent`] records and maps them into
/// an ordered [`Vec<StructuredDiagnosticEvent>`].
/// The walker is zero-copy in the sense that it does not clone or buffer the
/// input: it iterates exactly once and processes each item in place.
/// `ScVal` values *are* cloned into the output structs so they can be freely
/// moved around the call-site without a lifetime dependency on the input.
/// # Guarantees
/// - Output length **always equals** input length (zero-data-loss).
/// - No `panic!` — all error paths produce an [`DiagnosticEventKind::Unknown`]
///   record with a [`StructuredDiagnosticEvent::parse_error`] message.
pub struct DiagnosticEventWalker;

impl DiagnosticEventWalker {
    /// Create a new walker instance.
    /// The walker is stateless; you may re-use a single instance for multiple
    /// walks.
    pub fn new() -> Self {
        Self
    }

    /// Walk `events` and return an ordered, typed collection.
    /// Accepts any iterator whose item is a reference to a [`DiagnosticEvent`].
    /// The returned vector preserves the original ordering.
    /// # Panics
    /// Never panics.
    pub fn walk<'a, I>(&self, events: I) -> Vec<StructuredDiagnosticEvent>
    where
        I: Iterator<Item = &'a DiagnosticEvent>,
    {
        events.map(Self::process_one).collect()
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Map a single raw [`DiagnosticEvent`] to its structured counterpart.
    /// On any extraction failure the record is emitted with
    /// [`DiagnosticEventKind::Unknown`] and the error message is captured in
    /// [`StructuredDiagnosticEvent::parse_error`].
    fn process_one(raw: &DiagnosticEvent) -> StructuredDiagnosticEvent {
        let in_successful_call = raw.in_successful_contract_call;
        let inner = &raw.event;

        // Derive kind from the protocol-level discriminant.
        let kind = DiagnosticEventKind::from_contract_event_type(inner.type_);

        // Resolve optional contract address to a strkey string.
        let contract_id = inner.contract_id.as_ref().map(Self::hash_to_strkey);

        // Extract topics and data from the event body.
        match &inner.body {
            ContractEventBody::V0(v0) => {
                let topics: Vec<ScVal> = v0.topics.iter().cloned().collect();
                let data = v0.data.clone();
                StructuredDiagnosticEvent {
                    kind,
                    contract_id,
                    topics,
                    data,
                    in_successful_call,
                    parse_error: None,
                }
            }
        }
    }

    /// Given a slice of diagnostic events, returns the ContractId (as a
    /// strkey-encoded `C…` string) of the contract that emitted the final
    /// failure event.
    /// A "failure event" is one where `in_successful_contract_call` is `false`
    /// **and** the event carries a `contract_id`. Events are walked in reverse
    /// order so the last-emitted failure is found first.
    /// Returns `None` when no such event exists.
    pub fn find_failing_contract(events: &[DiagnosticEvent]) -> Option<String> {
        for event in events.iter().rev() {
            if !event.in_successful_contract_call {
                if let Some(ref hash) = event.event.contract_id {
                    return Some(Self::hash_to_strkey(hash));
                }
            }
        }
        None
    }

    /// Encode a raw 32-byte [`Hash`] as a Stellar contract strkey (`C…`).
    fn hash_to_strkey(hash: &Hash) -> String {
        StrkeyContract(hash.0).to_string()
    }
}

impl Default for DiagnosticEventWalker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Convenience free function
// ---------------------------------------------------------------------------

/// Walk `events` with a default [`DiagnosticEventWalker`].
/// Convenience wrapper; prefer constructing the walker explicitly when you
/// need to call it multiple times in a hot path.
pub fn walk_diagnostic_events(events: &[DiagnosticEvent]) -> Vec<StructuredDiagnosticEvent> {
    DiagnosticEventWalker::new().walk(events.iter())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        ContractEvent, ContractEventBody, ContractEventType, ContractEventV0, DiagnosticEvent,
        ExtensionPoint, Hash, ScSymbol, ScVal, VecM,
    };

    // -----------------------------------------------------------------------
    // Test builders
    // -----------------------------------------------------------------------

    fn contract_hash(seed: u8) -> Hash {
        Hash([seed; 32])
    }

    /// Build a minimal well-formed [`DiagnosticEvent`].
    fn make_event(
        event_type: ContractEventType,
        contract_id: Option<Hash>,
        topics: Vec<ScVal>,
        data: ScVal,
        in_successful_contract_call: bool,
    ) -> DiagnosticEvent {
        let topics_vec: VecM<ScVal> = topics.try_into().expect("topics VecM");
        DiagnosticEvent {
            in_successful_contract_call,
            event: ContractEvent {
                ext: ExtensionPoint::V0,
                contract_id,
                type_: event_type,
                body: ContractEventBody::V0(ContractEventV0 {
                    topics: topics_vec,
                    data,
                }),
            },
        }
    }

    fn sym(s: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(s.try_into().expect("symbol string")))
    }

    fn u32_val(n: u32) -> ScVal {
        ScVal::U32(n)
    }

    // -----------------------------------------------------------------------
    // Kind mapping
    // -----------------------------------------------------------------------

    #[test]
    fn contract_event_maps_to_contract_kind() {
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(1)),
            vec![sym("transfer")],
            u32_val(42),
            true,
        );
        let walker = DiagnosticEventWalker::new();
        let result = walker.walk(std::iter::once(&event));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, DiagnosticEventKind::Contract);
    }

    #[test]
    fn system_event_maps_to_system_kind() {
        let event = make_event(
            ContractEventType::System,
            None,
            vec![sym("fn_call"), sym("do_thing")],
            ScVal::Void,
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert_eq!(result[0].kind, DiagnosticEventKind::System);
    }

    #[test]
    fn diagnostic_event_maps_to_debug_kind() {
        let event = make_event(
            ContractEventType::Diagnostic,
            None,
            vec![sym("log"), sym("assertion failed")],
            ScVal::Void,
            false,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert_eq!(result[0].kind, DiagnosticEventKind::Debug);
    }

    // -----------------------------------------------------------------------
    // Contract event — full data integrity
    // -----------------------------------------------------------------------

    #[test]
    fn contract_event_topics_and_data_are_fully_preserved() {
        let topics = vec![sym("transfer"), sym("from"), sym("to")];
        let data = u32_val(1_000_000);
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(2)),
            topics.clone(),
            data.clone(),
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        let out = &result[0];

        assert_eq!(out.topics.len(), 3, "all three topics must be present");
        assert_eq!(out.topics[0], topics[0]);
        assert_eq!(out.topics[1], topics[1]);
        assert_eq!(out.topics[2], topics[2]);
        assert_eq!(out.data, data);
        assert!(out.contract_id.is_some(), "contract_id must be populated");
        assert!(out.parse_error.is_none());
        assert!(out.in_successful_call);
    }

    #[test]
    fn contract_event_strkey_encoding_is_correct() {
        let hash = contract_hash(5);
        let expected_strkey = StrkeyContract(hash.0).to_string();
        let event = make_event(
            ContractEventType::Contract,
            Some(hash),
            vec![],
            ScVal::Void,
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert_eq!(
            result[0].contract_id.as_deref(),
            Some(expected_strkey.as_str())
        );
        // Stellar contract strkeys start with 'C'
        assert!(result[0].contract_id.as_ref().unwrap().starts_with('C'));
    }

    #[test]
    fn event_without_contract_id_yields_none() {
        let event = make_event(
            ContractEventType::System,
            None,
            vec![sym("host_fn")],
            ScVal::Void,
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert!(result[0].contract_id.is_none());
    }

    #[test]
    fn system_event_has_no_contract_id_and_correct_topics() {
        let event = make_event(
            ContractEventType::System,
            None,
            vec![sym("call_stack_push"), sym("CABC1234")],
            ScVal::Bool(true),
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        let out = &result[0];

        assert_eq!(out.kind, DiagnosticEventKind::System);
        assert!(out.contract_id.is_none());
        assert_eq!(out.topics.len(), 2);
        assert_eq!(out.data, ScVal::Bool(true));
    }

    #[test]
    fn debug_event_parses_log_message_cleanly() {
        let log_msg = sym("debug: entering transfer fn");
        let event = make_event(
            ContractEventType::Diagnostic,
            None,
            vec![log_msg.clone()],
            ScVal::Void,
            false,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        let out = &result[0];

        assert_eq!(out.kind, DiagnosticEventKind::Debug);
        assert_eq!(out.topics.len(), 1);
        assert_eq!(out.topics[0], log_msg);
        assert!(!out.in_successful_call);
        assert!(out.parse_error.is_none());
    }

    #[test]
    fn in_successful_call_false_is_preserved() {
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(3)),
            vec![sym("error_emit")],
            ScVal::Error(stellar_xdr::curr::ScError::Contract(4)),
            false,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert!(!result[0].in_successful_call);
    }

    #[test]
    fn in_successful_call_true_is_preserved() {
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(4)),
            vec![sym("ok")],
            ScVal::Void,
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert!(result[0].in_successful_call);
    }

    #[test]
    fn is_healthy_returns_true_for_successful_well_formed_event() {
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(6)),
            vec![sym("mint")],
            u32_val(500),
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert!(result[0].is_healthy());
    }

    #[test]
    fn is_healthy_returns_false_when_call_failed() {
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(7)),
            vec![sym("revert")],
            ScVal::Void,
            false,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert!(!result[0].is_healthy());
    }

    #[test]
    fn event_with_empty_topics_is_accepted() {
        let event = make_event(ContractEventType::System, None, vec![], ScVal::Void, true);
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert_eq!(result[0].topics.len(), 0);
        assert_eq!(result[0].data, ScVal::Void);
    }

    #[test]
    fn output_count_equals_input_count_for_uniform_batch() {
        let n = 100usize;
        let events: Vec<DiagnosticEvent> = (0..n)
            .map(|i| {
                let kind = match i % 3 {
                    0 => ContractEventType::Contract,
                    1 => ContractEventType::System,
                    _ => ContractEventType::Diagnostic,
                };
                make_event(
                    kind,
                    Some(contract_hash(i as u8)),
                    vec![sym("topic")],
                    ScVal::Void,
                    i % 2 == 0,
                )
            })
            .collect();

        let result = DiagnosticEventWalker::new().walk(events.iter());
        assert_eq!(
            result.len(),
            n,
            "output count must match input count exactly"
        );
    }

    #[test]
    fn output_count_equals_input_count_for_mixed_batch() {
        let events = [
            make_event(
                ContractEventType::Contract,
                Some(contract_hash(1)),
                vec![sym("a")],
                u32_val(1),
                true,
            ),
            make_event(
                ContractEventType::System,
                None,
                vec![sym("b")],
                ScVal::Void,
                true,
            ),
            make_event(
                ContractEventType::Diagnostic,
                None,
                vec![sym("c"), sym("d")],
                ScVal::Bool(false),
                false,
            ),
            make_event(
                ContractEventType::Contract,
                Some(contract_hash(2)),
                vec![],
                ScVal::Void,
                true,
            ),
        ];

        let result = DiagnosticEventWalker::new().walk(events.iter());
        assert_eq!(result.len(), events.len());
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let events: Vec<DiagnosticEvent> = vec![];
        let result = DiagnosticEventWalker::new().walk(events.iter());
        assert!(result.is_empty());
    }

    #[test]
    fn walk_diagnostic_events_convenience_fn_matches_walker_output() {
        let events = [
            make_event(
                ContractEventType::Contract,
                Some(contract_hash(10)),
                vec![sym("transfer")],
                u32_val(99),
                true,
            ),
            make_event(
                ContractEventType::System,
                None,
                vec![sym("host_fn")],
                ScVal::Void,
                true,
            ),
        ];
        let via_fn = walk_diagnostic_events(&events);
        let via_walker = DiagnosticEventWalker::new().walk(events.iter());

        assert_eq!(via_fn.len(), via_walker.len());
        for (a, b) in via_fn.iter().zip(via_walker.iter()) {
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.contract_id, b.contract_id);
            assert_eq!(a.topics, b.topics);
            assert_eq!(a.data, b.data);
            assert_eq!(a.in_successful_call, b.in_successful_call);
        }
    }

    #[test]
    fn output_ordering_mirrors_input_ordering() {
        let events = [
            make_event(
                ContractEventType::Contract,
                Some(contract_hash(10)),
                vec![sym("first")],
                u32_val(1),
                true,
            ),
            make_event(
                ContractEventType::System,
                None,
                vec![sym("second")],
                u32_val(2),
                true,
            ),
            make_event(
                ContractEventType::Diagnostic,
                None,
                vec![sym("third")],
                u32_val(3),
                false,
            ),
        ];

        let result = DiagnosticEventWalker::new().walk(events.iter());

        assert_eq!(result[0].kind, DiagnosticEventKind::Contract);
        assert_eq!(result[1].kind, DiagnosticEventKind::System);
        assert_eq!(result[2].kind, DiagnosticEventKind::Debug);
    }

    #[test]
    fn all_kind_variants_are_reachable_from_xdr_type() {
        let contract_event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(20)),
            vec![],
            ScVal::Void,
            true,
        );
        let system_event = make_event(ContractEventType::System, None, vec![], ScVal::Void, true);
        let debug_event = make_event(
            ContractEventType::Diagnostic,
            None,
            vec![],
            ScVal::Void,
            true,
        );

        let events = [contract_event, system_event, debug_event];
        let result = DiagnosticEventWalker::new().walk(events.iter());

        let kinds: Vec<&DiagnosticEventKind> = result.iter().map(|e| &e.kind).collect();
        assert!(kinds.contains(&&DiagnosticEventKind::Contract));
        assert!(kinds.contains(&&DiagnosticEventKind::System));
        assert!(kinds.contains(&&DiagnosticEventKind::Debug));
    }

    #[test]
    fn complex_scval_data_survives_round_trip() {
        let data = ScVal::I64(-9_999_999_999_i64);
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(30)),
            vec![sym("large_payment")],
            data.clone(),
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert_eq!(result[0].data, data);
    }

    #[test]
    fn multiple_topics_of_mixed_types_survive_round_trip() {
        let topics = vec![sym("transfer"), ScVal::U32(1_234_567), ScVal::Bool(true)];
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(31)),
            topics.clone(),
            ScVal::Void,
            true,
        );
        let result = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        assert_eq!(result[0].topics.len(), 3);
        for (i, t) in topics.iter().enumerate() {
            assert_eq!(&result[0].topics[i], t, "topic {i} must match");
        }
    }

    #[test]
    fn default_walker_behaves_identically_to_new() {
        let event = make_event(
            ContractEventType::Contract,
            Some(contract_hash(99)),
            vec![sym("x")],
            ScVal::Void,
            true,
        );
        let a = DiagnosticEventWalker::new().walk(std::iter::once(&event));
        let b = DiagnosticEventWalker.walk(std::iter::once(&event));
        assert_eq!(a[0].kind, b[0].kind);
        assert_eq!(a[0].contract_id, b[0].contract_id);
    }

    #[test]
    fn kind_display_strings_are_correct() {
        assert_eq!(DiagnosticEventKind::Contract.to_string(), "Contract");
        assert_eq!(DiagnosticEventKind::System.to_string(), "System");
        assert_eq!(DiagnosticEventKind::Debug.to_string(), "Debug");
        assert_eq!(DiagnosticEventKind::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn find_failing_contract_returns_last_failed_event_contract() {
        let hash_a = contract_hash(1);
        let hash_b = contract_hash(2);
        let expected = StrkeyContract(hash_b.0).to_string();

        let events = vec![
            make_event(
                ContractEventType::Contract,
                Some(hash_a),
                vec![sym("transfer")],
                ScVal::Void,
                true,
            ),
            make_event(
                ContractEventType::Contract,
                Some(hash_b),
                vec![sym("error")],
                ScVal::Void,
                false,
            ),
        ];

        let result = DiagnosticEventWalker::find_failing_contract(&events);
        assert_eq!(result.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn find_failing_contract_returns_none_when_no_events() {
        assert_eq!(DiagnosticEventWalker::find_failing_contract(&[]), None);
    }

    #[test]
    fn find_failing_contract_returns_none_when_all_succeeded() {
        let events = vec![
            make_event(
                ContractEventType::Contract,
                Some(contract_hash(1)),
                vec![],
                ScVal::Void,
                true,
            ),
            make_event(ContractEventType::System, None, vec![], ScVal::Void, true),
        ];
        assert_eq!(DiagnosticEventWalker::find_failing_contract(&events), None);
    }

    #[test]
    fn find_failing_contract_returns_none_when_failed_event_has_no_contract_id() {
        let events = vec![make_event(
            ContractEventType::System,
            None,
            vec![sym("error")],
            ScVal::Void,
            false,
        )];
        assert_eq!(DiagnosticEventWalker::find_failing_contract(&events), None);
    }

    #[test]
    fn find_failing_contract_prefers_last_emitted_failure() {
        let hash_a = contract_hash(10);
        let hash_b = contract_hash(20);
        let expected_b = StrkeyContract(hash_b.0).to_string();

        let events = vec![
            make_event(
                ContractEventType::Contract,
                Some(hash_a),
                vec![sym("error")],
                ScVal::Void,
                false,
            ),
            make_event(
                ContractEventType::Contract,
                Some(hash_b),
                vec![sym("error")],
                ScVal::Void,
                false,
            ),
        ];

        let result = DiagnosticEventWalker::find_failing_contract(&events);
        assert_eq!(result.as_deref(), Some(expected_b.as_str()));
    }

    #[test]
    fn find_failing_contract_skips_successful_events_in_reverse() {
        let hash_fail = contract_hash(42);
        let expected = StrkeyContract(hash_fail.0).to_string();

        let events = vec![
            make_event(
                ContractEventType::Contract,
                Some(contract_hash(1)),
                vec![],
                ScVal::Void,
                true,
            ),
            make_event(
                ContractEventType::Contract,
                Some(hash_fail),
                vec![sym("error")],
                ScVal::Void,
                false,
            ),
            make_event(ContractEventType::System, None, vec![], ScVal::Void, true),
        ];

        let result = DiagnosticEventWalker::find_failing_contract(&events);
        assert_eq!(result.as_deref(), Some(expected.as_str()));
    }
}
