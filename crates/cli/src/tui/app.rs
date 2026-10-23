use std::{
    io::{self, IsTerminal},
    time::Duration,
};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use grat_core::types::config::NetworkConfig;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};

use super::{
    state::{Panel, TuiState},
    widgets::{controls, inspector, timeline},
};

/// Restore the terminal on normal exit, I/O errors, and panic unwinding.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let guard = Self;
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
    }
}

pub async fn launch(tx_hash: &str, network: &NetworkConfig) -> anyhow::Result<()> {
    anyhow::ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "Interactive replay requires a terminal; use grat trace for redirected output."
    );
    // Fetch/reconstruct before entering raw mode: replay errors retain normal terminal output.
    let trace = grat_core::replay::replay_transaction(tx_hash, network).await?;
    let mut state = TuiState::new(trace);
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    loop {
        terminal.draw(|frame| render(frame, &state))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if handle_key(&mut state, key) => break,
                Event::Resize(_, _) => terminal.autoresize()?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn handle_key(state: &mut TuiState, key: KeyEvent) -> bool {
    if key.kind == KeyEventKind::Release {
        return false;
    }
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return false;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Tab => state.switch_panel(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::BackTab => state.switch_panel(true),
        KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('s') => state.next(),
        KeyCode::Char('k') | KeyCode::Up => state.previous(),
        KeyCode::Char('o') => state.step_over(),
        _ => {}
    }
    false
}

fn panel(title: &str, selected: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if selected {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
}

fn render(frame: &mut Frame<'_>, state: &TuiState) {
    let area = frame.area();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(controls::height(area.width).min(area.height.saturating_sub(1))),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(format!(
            "Replay {} | {} | ledger {}",
            state.tx_hash, state.network, state.ledger_sequence
        )),
        sections[0],
    );
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sections[1]);
    timeline::render_with_focus(
        frame,
        panels[0],
        &state.invocations,
        state.scroll_offset,
        state.selected_panel == Panel::Timeline,
    );
    let detail_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(0)])
        .split(panels[1]);
    let details = state.entries.get(state.scroll_offset).map_or_else(
        || "No recorded invocations. [q] quits.".to_owned(),
        |entry| {
            format!(
                "Contract: {}\nFunction: {}\nArguments: {}\nReturn: {}\nError: {}",
                entry.contract,
                entry.function,
                entry.arguments.join(", "),
                entry.return_value.as_deref().unwrap_or("unavailable"),
                entry.is_error
            )
        },
    );
    frame.render_widget(
        Paragraph::new(details)
            .wrap(Wrap { trim: true })
            .block(panel("Invocation", state.selected_panel == Panel::Inspector)),
        detail_sections[0],
    );
    inspector::render_inspector(
        frame,
        detail_sections[1],
        &state.state_diff,
        state.selected_invocation.as_ref(),
        state.scroll_offset,
    );
    controls::render(frame, sections[2], state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn press(state: &mut TuiState, code: KeyCode) -> bool {
        handle_key(state, KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn panels_cycle_in_both_directions() {
        let mut state = super::super::state::fixture();
        for expected in [Panel::Inspector, Panel::Controls, Panel::Timeline] {
            assert!(!press(&mut state, KeyCode::Tab));
            assert_eq!(state.selected_panel, expected);
        }
        press(&mut state, KeyCode::BackTab);
        assert_eq!(state.selected_panel, Panel::Controls);
    }

    #[test]
    fn navigation_and_steps_use_real_trace_boundaries_and_depth() {
        let mut state = super::super::state::fixture();
        press(&mut state, KeyCode::Char('k'));
        assert_eq!(state.scroll_offset, 0);
        press(&mut state, KeyCode::Char('s'));
        assert_eq!(state.entries[state.scroll_offset].function, "child");
        assert_eq!(
            state
                .selected_invocation
                .as_ref()
                .map(|invocation| invocation.function_name.as_str()),
            Some("child")
        );
        press(&mut state, KeyCode::Up);
        press(&mut state, KeyCode::Char('o'));
        assert_eq!(state.entries[state.scroll_offset].function, "sibling");
        assert_eq!(
            state
                .selected_invocation
                .as_ref()
                .map(|invocation| invocation.function_name.as_str()),
            Some("sibling")
        );
        press(&mut state, KeyCode::Char('j'));
        assert_eq!(state.scroll_offset, 2);
        assert_eq!(state.replay_status(), "Replay: End 3/3");
        state.entries.clear();
        state.scroll_offset = 0;
        for code in [KeyCode::Char('s'), KeyCode::Char('o'), KeyCode::Char('k')] {
            press(&mut state, code);
        }
        assert_eq!(state.scroll_offset, 0);
        assert_eq!(state.replay_status(), "Replay: no invocations");
    }

    #[test]
    fn releases_are_ignored_and_quit_keys_work() {
        let mut state = super::super::state::fixture();
        let mut release = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert!(!handle_key(&mut state, release));
        assert_eq!(state.scroll_offset, 0);
        assert!(press(&mut state, KeyCode::Char('q')));
        assert!(press(&mut state, KeyCode::Esc));
        assert!(handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        ));
    }

    #[test]
    fn timeline_and_inspector_follow_the_controls_selection() {
        let mut state = super::super::state::fixture();
        press(&mut state, KeyCode::Char('s'));
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Timeline"));
        assert!(text.contains("Function: child"));
        assert!(text.contains("ledger 1"));
        assert!(text.contains("Replay: Paused 2/3"));
        assert_eq!(timeline::count(&state.invocations), state.entries.len());
    }

    #[test]
    fn layout_resizes_and_keeps_footer_visible() {
        let state = super::super::state::fixture();
        for (width, height) in [(120, 24), (60, 20), (40, 15), (1, 1), (0, 0)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| render(frame, &state)).unwrap();
            if width >= 40 {
                let text: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect();
                assert!(text.contains("[q] Quit") && text.contains("Replay: Paused"));
                assert!(text.contains("Timeline"));
            }
            if width == 120 {
                println!("{}", terminal.backend());
            }
        }
    }
}
