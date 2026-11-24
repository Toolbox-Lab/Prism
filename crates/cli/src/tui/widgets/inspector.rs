//! Inspector widget for the TUI debugger.
//!
//! Renders the selected invocation's arguments, return value and error state
//! followed by a Ratatui `Table` of storage state diffs, with colour-coded
//! additions (green), deletions (red), and modifications (yellow).  Long diff
//! lists can be scrolled via a configurable scroll offset stored in
//! [`crate::tui::state::TuiState`].

use grat_core::types::trace::{ContractInvocation, DiffChangeType, LedgerEntryDiff, StateDiff};
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
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
///                  header title and call details).
/// * `scroll`     – Number of diff rows already scrolled past (modified by
///                  the caller via up/down key events).
#[allow(dead_code)]
pub fn render_inspector(
    frame: &mut Frame,
    area: Rect,
    state_diff: &StateDiff,
    selected: Option<&ContractInvocation>,
    scroll: usize,
) {
    render_inspector_with_focus(frame, area, state_diff, selected, scroll, false);
}

/// Render the inspector and highlight its border while it owns keyboard focus.
pub fn render_inspector_with_focus(
    frame: &mut Frame,
    area: Rect,
    state_diff: &StateDiff,
    selected: Option<&ContractInvocation>,
    scroll: usize,
    focused: bool,
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
        .border_style(Style::default().fg(if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }));

    // Inner drawing area (excludes the border) ------------------------------
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Show the selected call's arguments, return value and error state before
    // the recorded state changes so every invocation can be inspected.
    let mut table_offset_y = 0;
    if let Some(inv) = selected {
        let detail_height = 5_u16.min(inner.height);
        frame.render_widget(
            Paragraph::new(details(inv))
                .wrap(Wrap { trim: true })
                .style(Style::default()),
            Rect {
                height: detail_height,
                ..inner
            },
        );
        table_offset_y = detail_height.saturating_add(1);
    }

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
            y: inner.y.saturating_add(table_offset_y),
            height: inner.height.saturating_sub(table_offset_y),
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
        y: inner.y.saturating_add(table_offset_y),
        height: inner.height.saturating_sub(table_offset_y),
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

/// Build the detailed call view shown at the top of the inspector.
fn details(inv: &ContractInvocation) -> Vec<Line<'static>> {
    let label = Style::default().fg(Color::Gray);
    let error_style = if inv.is_error {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let args = if inv.arguments.is_empty() {
        "(none)".to_owned()
    } else {
        inv.arguments.join(", ")
    };
    let return_value = inv
        .return_value
        .clone()
        .unwrap_or_else(|| "unavailable".to_owned());

    vec![
        detail_line("Contract: ", &inv.contract_id, label, Style::default()),
        detail_line("Function: ", &inv.function_name, label, Style::default()),
        detail_line("Arguments: ", &args, label, Style::default()),
        detail_line("Return: ", &return_value, label, Style::default()),
        detail_line("Error: ", &inv.is_error.to_string(), label, error_style),
    ]
}

fn detail_line(
    label: &'static str,
    value: &str,
    label_style: Style,
    value_style: Style,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, label_style),
        Span::styled(truncate(value, 80), value_style),
    ])
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
    const ELLIPSIS: &str = "…";
    if s.len() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(ELLIPSIS.len());
    let mut shortened: String = s.chars().take(keep).collect();
    shortened.push_str(ELLIPSIS);
    shortened
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use grat_core::types::trace::{DiffChangeType, LedgerEntryDiff, StateDiff, TraceId};
    use ratatui::{backend::TestBackend, Terminal};

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
        let diff = make_diff(vec![LedgerEntryDiff {
            key: "k1".to_string(),
            before: None,
            after: Some("v1".to_string()),
            change_type: DiffChangeType::Created,
        }]);
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
    fn inspector_renders_selected_call_details() {
        let invocation = ContractInvocation {
            trace_id: TraceId::new(),
            contract_id: "CEXAMPLE".to_owned(),
            function_name: "transfer".to_owned(),
            arguments: vec!["amount=7".to_owned()],
            return_value: Some("ok".to_owned()),
            host_calls: Vec::new(),
            sub_invocations: Vec::new(),
            total_cpu_instructions: 0,
            total_memory_bytes: 0,
            is_error: true,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        terminal
            .draw(|frame| {
                render_inspector(
                    frame,
                    frame.area(),
                    &StateDiff::default(),
                    Some(&invocation),
                    0,
                );
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();

        for expected in [
            "Contract: CEXAMPLE",
            "Function: transfer",
            "Arguments: amount=7",
            "Return: ok",
            "Error: true",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
    }

    #[test]
    fn inspector_border_follows_panel_focus() {
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        terminal
            .draw(|frame| {
                render_inspector_with_focus(
                    frame,
                    frame.area(),
                    &StateDiff::default(),
                    None,
                    0,
                    true,
                );
            })
            .unwrap();
        assert_eq!(
            terminal.backend().buffer().cell((0, 0)).unwrap().fg,
            Color::Cyan
        );

        terminal
            .draw(|frame| {
                render_inspector_with_focus(
                    frame,
                    frame.area(),
                    &StateDiff::default(),
                    None,
                    0,
                    false,
                );
            })
            .unwrap();
        assert_eq!(
            terminal.backend().buffer().cell((0, 0)).unwrap().fg,
            Color::DarkGray
        );
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
