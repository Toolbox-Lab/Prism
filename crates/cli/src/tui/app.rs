//! TUI application entry point.

use prism_core::types::config::NetworkConfig;

/// Launch the interactive TUI debugger.
pub async fn launch(tx_hash: &str, network: &NetworkConfig) -> anyhow::Result<()> {
    let state = crate::tui::state::TuiState {
        tx_hash: tx_hash.to_string(),
        selected_panel: crate::tui::state::Panel::Timeline,
        scroll_offset: 0,
    };

    for panel in [
        crate::tui::state::Panel::Timeline,
        crate::tui::state::Panel::Inspector,
        crate::tui::state::Panel::Controls,
    ] {
        match panel {
            crate::tui::state::Panel::Timeline => crate::tui::widgets::timeline::render(),
            crate::tui::state::Panel::Inspector => crate::tui::widgets::inspector::render(),
            crate::tui::state::Panel::Controls => crate::tui::widgets::controls::render(),
        }
    }
    // TODO: Initialize ratatui terminal, run event loop
    println!(
        "TUI debugger launching for {} on {:?}...",
        state.tx_hash, network.network
    );
    println!(
        "Selected panel: {:?} (scroll offset: {})",
        state.selected_panel, state.scroll_offset
    );
    println!("(Not yet implemented — requires ratatui setup)");
    Ok(())
}
