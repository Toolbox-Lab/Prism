#![allow(dead_code)]

use grat_core::types::trace::{ContractInvocation, StateDiff};

#[derive(Debug, Default)]
pub struct TuiState {
    pub tx_hash: String,
    pub selected_panel: Panel,
    pub scroll_offset: usize,
    /// The currently selected invocation frame (if any).
    pub selected_invocation: Option<ContractInvocation>,
    /// The state diff associated with the current trace.
    pub state_diff: StateDiff,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Panel {
    #[default]
    Timeline,
    Inspector,
    Controls,
}
