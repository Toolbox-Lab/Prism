use crate::types::report::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventsErrorDetail {
    pub code: u32,
    pub name: &'static str,
    /// Short explanation of the event emission failure.
    pub summary: &'static str,
    pub severity: Severity,
}

pub const EVENTS_ERROR_DETAILS: &[EventsErrorDetail] = &[
    EventsErrorDetail {
        code: 0,
        name: "ArithDomain",
        summary: "Event emission failed: an arithmetic overflow occurred while constructing the event payload.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 1,
        name: "IndexBounds",
        summary: "Event emission failed: a topic index was out of bounds — the contract referenced a topic slot that does not exist.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 2,
        name: "InvalidInput",
        summary: "Event emission failed: the event input is malformed — check that topics and data values are valid host values.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 3,
        name: "MissingValue",
        summary: "Event emission failed: a required event field was not provided.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 4,
        name: "ExistingValue",
        summary: "Event emission failed: the contract attempted to emit a duplicate event that is not allowed in this context.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 5,
        name: "ExceededLimit",
        summary: "Too many topics: the event exceeds the maximum number of topics allowed per event — reduce the number of topics.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 6,
        name: "InvalidAction",
        summary: "Event emission failed: the contract attempted to emit an event in an invalid execution context.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 7,
        name: "InternalError",
        summary: "Event emission failed: the host encountered an internal error in its event subsystem — this may be a platform bug.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 8,
        name: "UnexpectedType",
        summary: "Event emission failed: an event topic or data value has an unexpected type.",
        severity: Severity::Error,
    },
    EventsErrorDetail {
        code: 9,
        name: "UnexpectedSize",
        summary: "Data payload too large: the event data exceeds the maximum allowed size — reduce the size of the event data.",
        severity: Severity::Error,
    },
];

pub fn lookup(code: u32) -> Option<&'static EventsErrorDetail> {
    EVENTS_ERROR_DETAILS
        .iter()
        .find(|detail| detail.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_exceeded_limit_for_code_5() {
        let detail = lookup(5).expect("exceeded limit detail");
        assert_eq!(detail.name, "ExceededLimit");
        assert!(
            detail.summary.contains("too many topics")
                || detail.summary.to_lowercase().contains("topics")
        );
    }

    #[test]
    fn lookup_returns_unexpected_size_for_code_9() {
        let detail = lookup(9).expect("unexpected size detail");
        assert_eq!(detail.name, "UnexpectedSize");
        assert!(
            detail.summary.to_lowercase().contains("payload")
                || detail.summary.to_lowercase().contains("size")
        );
    }

    #[test]
    fn lookup_returns_invalid_input_for_code_2() {
        let detail = lookup(2).expect("invalid input detail");
        assert_eq!(detail.name, "InvalidInput");
    }

    #[test]
    fn table_covers_all_known_events_codes() {
        assert_eq!(EVENTS_ERROR_DETAILS.len(), 10);
        assert!(lookup(99).is_none());
    }

    #[test]
    fn codes_are_contiguous_and_unique() {
        for (i, detail) in EVENTS_ERROR_DETAILS.iter().enumerate() {
            assert_eq!(detail.code as usize, i, "codes should be contiguous from 0");
        }
    }
}
