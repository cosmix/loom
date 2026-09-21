//! The status block and the key footer.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::super::state::ConfigState;
use super::super::theme;
use crate::commands::status::ui::tui::ledger::text::spans_width;

/// The footer's pairs in priority order: the ones at the end are the ones a
/// narrow terminal loses.
const FOOTER: [(&str, &str); 7] = [
    ("↑↓", "move"),
    ("←→", "cycle"),
    ("⏎", "edit"),
    ("x", "clear"),
    ("⇥", "scope"),
    ("s", "save"),
    ("q", "quit"),
];

/// Render the last action's result, with a glyph for what kind of result it
/// is. A short frame keeps the line and drops the box around it.
pub(super) fn render_status(frame: &mut Frame, area: Rect, state: &ConfigState, bordered: bool) {
    let (glyph, glyph_style) = if state.status_is_error() {
        ("✗", theme::error())
    } else if state.wrote() {
        ("✓", theme::written())
    } else {
        ("›", theme::secondary())
    };
    let text_style = if state.status_is_error() {
        theme::error()
    } else {
        theme::primary()
    };
    let status = Paragraph::new(Line::from(vec![
        Span::styled(format!("{glyph} "), glyph_style),
        Span::styled(state.status().to_owned(), text_style),
    ]))
    .wrap(Wrap { trim: true });
    let status = if bordered {
        status.block(theme::block(theme::chrome_title("status")))
    } else {
        status
    };
    frame.render_widget(status, area);
}

/// Render the key reminder, dropping trailing pairs the width cannot hold.
pub(super) fn render_footer(frame: &mut Frame, area: Rect) {
    frame.render_widget(Paragraph::new(footer_line(area.width)), area);
}

/// Build the footer pair by pair, measuring as it goes: the keys are arrows
/// and a tab glyph, so a character count would let the line overflow.
fn footer_line(width: u16) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (key, label) in FOOTER {
        let mut pair = Vec::new();
        if spans.len() > 1 {
            pair.push(Span::styled(" · ", theme::rule()));
        }
        pair.push(Span::styled(key, theme::key_hint()));
        pair.push(Span::styled(format!(" {label}"), theme::secondary()));
        if spans_width(&spans) + spans_width(&pair) > usize::from(width) {
            break;
        }
        spans.extend(pair);
    }
    Line::from(spans)
}
