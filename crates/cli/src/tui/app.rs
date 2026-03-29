//! TUI application entry point.

use prism_core::types::config::NetworkConfig;

/// Launch the interactive TUI debugger.
pub async fn launch(tx_hash: &str, _network: &NetworkConfig) -> anyhow::Result<()> {
    // TODO: Initialize ratatui terminal, run event loop
    println!("TUI debugger launching for {tx_hash}...");
    println!("(Not yet implemented — requires ratatui setup)");
    Ok(())
}
