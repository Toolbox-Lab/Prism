//! TUI application state management.

#![allow(dead_code)]

/// TUI application state.
#[derive(Debug)]
pub struct TuiState {
    pub tx_hash: String,
    pub selected_panel: Panel,
    pub scroll_offset: usize,
}

/// Active panel in the TUI.
#[derive(Clone, Copy, Debug)]
pub enum Panel {
    Timeline,
    Inspector,
    Controls,
}
