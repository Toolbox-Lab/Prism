//! Inspector widget for the TUI debugger.
//!
//! Renders a Ratatui `Table` showing storage state diffs for the currently
//! selected invocation frame, with colour-coded additions (green), deletions
//! (red), and modifications (yellow).  Long diff lists can be scrolled via a
//! configurable scroll offset stored in [`crate::tui::state::TuiState`].

use grat_core::types::trace::{ContractInvocation, DiffChangeType, LedgerEntryDiff, StateDiff};
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Render the inspector panel into `area` of the provided [`Frame`].
///
/// # Arguments
///
/// * `frame`      – Ratatui frame to draw into.
/// * `area`       – Terminal rectangle allocated to this widget.
/// * `state_diff` – Storage state changes computed for the current trace.
/// * `selected`   – The currently focused invocation frame (used for the
///                  header title and argument summary).
/// * `scroll`     – Number of diff rows already scrolled past (modified by
///                  the caller via up/down key events).
pub fn render_inspector(
    frame: &mut Frame,
    area: Rect,
    state_diff: &StateDiff,
    selected: Option<&ContractInvocation>,
    scroll: usize,
) {
    // Build the outer block ------------------------------------------------
    let title = selected
        .map(|inv| {
            format!(
                " Inspector ─ {}::{} ",
                truncate(&inv.contract_id, 16),
                inv.function_name
            )
        })
        .unwrap_or_else(|| " Inspector ".to_string());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    // Inner drawing area (excludes the border) ------------------------------
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // If there is a selected frame, render an argument summary first --------
    let table_offset_y = if let Some(inv) = selected {
        render_arg_summary(frame, inner, inv);
        // Reserve 3 rows for the argument summary paragraph + gap
        3
    } else {
        0
    };

    // Filter to only meaningful changes (skip Unchanged) -------------------
    let visible_entries: Vec<&LedgerEntryDiff> = state_diff
        .entries
        .iter()
        .filter(|e| !matches!(e.change_type, DiffChangeType::Unchanged))
        .collect();

    if visible_entries.is_empty() {
        let msg = Paragraph::new("No storage changes for this frame.")
            .style(Style::default().fg(Color::DarkGray));
        let msg_area = Rect {
            y: inner.y + table_offset_y as u16,
            height: inner.height.saturating_sub(table_offset_y as u16),
            ..inner
        };
        frame.render_widget(msg, msg_area);
        return;
    }

    // Apply scroll offset --------------------------------------------------
    let max_scroll = visible_entries.len().saturating_sub(1);
    let scroll = scroll.min(max_scroll);
    let scrolled_entries = &visible_entries[scroll..];

    // Build table rows -----------------------------------------------------
    let rows: Vec<Row> = scrolled_entries
        .iter()
        .map(|entry| build_row(entry))
        .collect();

    let header = Row::new(vec![
        Cell::from("Change").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Key").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Before").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("After").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1)
    .bottom_margin(1);

    let table_area = Rect {
        y: inner.y + table_offset_y as u16,
        height: inner.height.saturating_sub(table_offset_y as u16),
        ..inner
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Percentage(35),
            Constraint::Percentage(27),
            Constraint::Percentage(27),
        ],
    )
    .header(header)
    .block(Block::default());

    frame.render_widget(table, table_area);
}

// ---------------------------------------------------------------------------
// Stub kept for backward compatibility (called from app.rs with no args)
// ---------------------------------------------------------------------------

/// Legacy no-op stub retained so existing callers compile unchanged.
///
/// Prefer [`render_inspector`] for all new call-sites.
#[allow(dead_code)]
pub fn render() {}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Render a one-line argument summary at the top of `area`.
fn render_arg_summary(frame: &mut Frame, area: Rect, inv: &ContractInvocation) {
    if area.height == 0 {
        return;
    }

    let args_display = if inv.arguments.is_empty() {
        "(no args)".to_string()
    } else {
        inv.arguments.join(", ")
    };

    let line = Line::from(vec![
        Span::styled("Args: ", Style::default().fg(Color::Gray)),
        Span::styled(truncate(&args_display, 80), Style::default().fg(Color::White)),
    ]);

    let para = Paragraph::new(line);
    let summary_area = Rect {
        height: 1,
        ..area
    };
    frame.render_widget(para, summary_area);
}

/// Build a coloured [`Row`] for a single [`LedgerEntryDiff`].
fn build_row(entry: &LedgerEntryDiff) -> Row {
    let (label, row_style) = match entry.change_type {
        DiffChangeType::Created => (
            Span::styled("+ Added", Style::default().fg(Color::Green)),
            Style::default().fg(Color::Green),
        ),
        DiffChangeType::Deleted => (
            Span::styled("- Deleted", Style::default().fg(Color::Red)),
            Style::default().fg(Color::Red),
        ),
        DiffChangeType::Updated => (
            Span::styled("~ Modified", Style::default().fg(Color::Yellow)),
            Style::default().fg(Color::Yellow),
        ),
        DiffChangeType::Unchanged => (
            Span::styled("  Unchanged", Style::default().fg(Color::DarkGray)),
            Style::default().fg(Color::DarkGray),
        ),
    };

    let before = entry
        .before
        .as_deref()
        .map(|v| truncate(v, 20))
        .unwrap_or_else(|| "—".to_string());

    let after = entry
        .after
        .as_deref()
        .map(|v| truncate(v, 20))
        .unwrap_or_else(|| "—".to_string());

    Row::new(vec![
        Cell::from(label),
        Cell::from(truncate(&entry.key, 30)).style(row_style),
        Cell::from(before).style(row_style),
        Cell::from(after).style(row_style),
    ])
}

/// Truncate `s` to at most `max` bytes, appending `…` when shortened.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use grat_core::types::trace::{DiffChangeType, LedgerEntryDiff, StateDiff};

    fn make_diff(entries: Vec<LedgerEntryDiff>) -> StateDiff {
        StateDiff { entries }
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("abcdefghij", 5);
        assert!(result.len() <= 6, "truncated string should be short");
        assert!(result.ends_with('…'), "should end with ellipsis");
    }

    #[test]
    fn build_row_created_entry() {
        let entry = LedgerEntryDiff {
            key: "contract_data::owner".to_string(),
            before: None,
            after: Some("GABC1234".to_string()),
            change_type: DiffChangeType::Created,
        };
        // Just ensure it doesn't panic
        let _row = build_row(&entry);
    }

    #[test]
    fn build_row_deleted_entry() {
        let entry = LedgerEntryDiff {
            key: "contract_data::balance".to_string(),
            before: Some("0000dead".to_string()),
            after: None,
            change_type: DiffChangeType::Deleted,
        };
        let _row = build_row(&entry);
    }

    #[test]
    fn build_row_updated_entry() {
        let entry = LedgerEntryDiff {
            key: "contract_data::counter".to_string(),
            before: Some("00000001".to_string()),
            after: Some("00000002".to_string()),
            change_type: DiffChangeType::Updated,
        };
        let _row = build_row(&entry);
    }

    #[test]
    fn scroll_clamp() {
        // Scroll beyond the list length should not panic in real render;
        // here we just test the clamping arithmetic.
        let diff = make_diff(vec![
            LedgerEntryDiff {
                key: "k1".to_string(),
                before: None,
                after: Some("v1".to_string()),
                change_type: DiffChangeType::Created,
            },
        ]);
        let visible: Vec<&LedgerEntryDiff> = diff
            .entries
            .iter()
            .filter(|e| !matches!(e.change_type, DiffChangeType::Unchanged))
            .collect();
        let max_scroll = visible.len().saturating_sub(1);
        let clamped = 999_usize.min(max_scroll);
        assert_eq!(clamped, 0);
    }

    #[test]
    fn unchanged_entries_are_filtered() {
        let diff = make_diff(vec![
            LedgerEntryDiff {
                key: "k1".to_string(),
                before: Some("v1".to_string()),
                after: Some("v1".to_string()),
                change_type: DiffChangeType::Unchanged,
            },
            LedgerEntryDiff {
                key: "k2".to_string(),
                before: None,
                after: Some("v2".to_string()),
                change_type: DiffChangeType::Created,
            },
        ]);
        let visible: Vec<&LedgerEntryDiff> = diff
            .entries
            .iter()
            .filter(|e| !matches!(e.change_type, DiffChangeType::Unchanged))
            .collect();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].key, "k2");
    }
}
