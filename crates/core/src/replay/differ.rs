//! State differ — compares ledger state before and after transaction execution.

use crate::replay::sandbox::SandboxResult;
use crate::replay::state::LedgerState;
use crate::types::error::PrismResult;
use crate::types::trace::{DiffChangeType, LedgerEntryDiff, StateDiff};

/// Compute the state diff between pre-execution and post-execution ledger states.
pub fn compute_diff(pre_state: &LedgerState, result: &SandboxResult) -> PrismResult<StateDiff> {
    let mut entries = Vec::new();

    // Find modified and deleted entries
    for (key, before_value) in &pre_state.entries {
        if let Some(after_value) = result.final_state.get(key) {
            if before_value != after_value {
                entries.push(LedgerEntryDiff {
                    key: key.clone(),
                    before: Some(hex_encode(before_value)),
                    after: Some(hex_encode(after_value)),
                    change_type: DiffChangeType::Updated,
                });
            } else {
                entries.push(LedgerEntryDiff {
                    key: key.clone(),
                    before: Some(hex_encode(before_value)),
                    after: Some(hex_encode(after_value)),
                    change_type: DiffChangeType::Unchanged,
                });
            }
        } else {
            entries.push(LedgerEntryDiff {
                key: key.clone(),
                before: Some(hex_encode(before_value)),
                after: None,
                change_type: DiffChangeType::Deleted,
            });
        }
    }

    // Find created entries (in final state but not in pre-state)
    for (key, after_value) in &result.final_state {
        if !pre_state.entries.contains_key(key) {
            entries.push(LedgerEntryDiff {
                key: key.clone(),
                before: None,
                after: Some(hex_encode(after_value)),
                change_type: DiffChangeType::Created,
            });
        }
    }

    Ok(StateDiff { entries })
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
