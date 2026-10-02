#![allow(dead_code)]

#[derive(Debug)]
pub struct TuiState {
    pub tx_hash: String,
    pub selected_panel: Panel,
    pub scroll_offset: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum Panel {
    Timeline,
    Inspector,
    Controls,
}
