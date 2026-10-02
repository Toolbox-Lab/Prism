use crate::error::GratResult;
use crate::replay::sandbox::SandboxResult;
use crate::replay::state::LedgerState;
use crate::types::trace::{DiffChangeType, LedgerEntryDiff, StateDiff};

pub fn compute_diff(pre_state: &LedgerState, result: &SandboxResult) -> GratResult<StateDiff> {
    let mut entries = Vec::new();

    for (key, before_value) in &pre_state.entries {
        if let Some(after_value) = result.final_state.get(key) {
            if before_value == after_value {
                entries.push(LedgerEntryDiff {
                    key: key.clone(),
                    before: Some(hex::encode(before_value)),
                    after: Some(hex::encode(after_value)),
                    change_type: DiffChangeType::Unchanged,
                });
            } else {
                entries.push(LedgerEntryDiff {
                    key: key.clone(),
                    before: Some(hex::encode(before_value)),
                    after: Some(hex::encode(after_value)),
                    change_type: DiffChangeType::Updated,
                });
            }
        } else {
            entries.push(LedgerEntryDiff {
                key: key.clone(),
                before: Some(hex::encode(before_value)),
                after: None,
                change_type: DiffChangeType::Deleted,
            });
        }
    }

    for (key, after_value) in &result.final_state {
        if !pre_state.entries.contains_key(key) {
            entries.push(LedgerEntryDiff {
                key: key.clone(),
                before: None,
                after: Some(hex::encode(after_value)),
                change_type: DiffChangeType::Created,
            });
        }
    }

    Ok(StateDiff { entries })
}
