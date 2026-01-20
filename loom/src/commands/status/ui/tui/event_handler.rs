//! Event handling for TUI keyboard and mouse input.

use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};

use super::state::GraphState;

/// Scroll step for arrow key navigation.
pub const SCROLL_STEP: i32 = 2;

/// Page scroll multiplier (viewport size * this factor).
pub const PAGE_SCROLL_FACTOR: f64 = 0.8;

/// Result of handling a key event.
pub enum KeyEventResult {
    /// User requested exit.
    Exit,
    /// Continue running.
    Continue,
}

/// Handle keyboard events for navigation and control.
pub fn handle_key_event(
    code: KeyCode,
    modifiers: KeyModifiers,
    graph_state: &mut GraphState,
) -> KeyEventResult {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => KeyEventResult::Exit,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => KeyEventResult::Exit,

        KeyCode::Up => {
            graph_state.scroll_by(-SCROLL_STEP as i16);
            KeyEventResult::Continue
        }
        KeyCode::Down => {
            graph_state.scroll_by(SCROLL_STEP as i16);
            KeyEventResult::Continue
        }

        KeyCode::Home => {
            graph_state.scroll_to_start();
            KeyEventResult::Continue
        }
        KeyCode::End => {
            graph_state.scroll_to_end();
            KeyEventResult::Continue
        }

        KeyCode::PageUp => {
            let page_step = (graph_state.viewport_height as f64 * PAGE_SCROLL_FACTOR) as i16;
            graph_state.scroll_by(-page_step);
            KeyEventResult::Continue
        }
        KeyCode::PageDown => {
            let page_step = (graph_state.viewport_height as f64 * PAGE_SCROLL_FACTOR) as i16;
            graph_state.scroll_by(page_step);
            KeyEventResult::Continue
        }

        KeyCode::Left | KeyCode::Right => KeyEventResult::Continue,

        _ => KeyEventResult::Continue,
    }
}

/// Handle mouse events for scrolling.
pub fn handle_mouse_event(kind: MouseEventKind, graph_state: &mut GraphState) {
    match kind {
        MouseEventKind::ScrollUp => {
            graph_state.scroll_by(-(SCROLL_STEP as i16) * 2);
        }
        MouseEventKind::ScrollDown => {
            graph_state.scroll_by((SCROLL_STEP as i16) * 2);
        }
        _ => {}
    }
}
