//! TUI application state management.

#![allow(dead_code)]

/// TUI application state.
pub struct TuiState {
    pub tx_hash: String,
    pub selected_panel: Panel,
    pub scroll_offset: usize,
}

/// Active panel in the TUI.
pub enum Panel {
    Timeline,
    Inspector,
    Controls,
}
