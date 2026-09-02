//! Ratatui rendering for the interactive config editor.
//!
//! Rendering reads state without mutating it, so terminal sizing and styling
//! cannot accidentally alter staged values or their save behavior.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
    Frame,
};

use super::state::ConfigState;
use crate::commands::status::ui::theme::{StatusColors, Theme};

/// Draw the header, registry rows, status line, and keyboard reminder.
pub(super) fn draw(frame: &mut Frame, state: &ConfigState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());
    render_header(frame, areas[0]);
    render_rows(frame, areas[1], state);
    render_status(frame, areas[2], state);
    render_footer(frame, areas[3]);
}

/// Render a compact title that explains edits remain staged until save.
fn render_header(frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(Line::from(vec![
        Span::styled("loom config", Theme::header()),
        Span::styled("  staged changes are not written until s", Theme::dimmed()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(StatusColors::BORDER)),
    );
    frame.render_widget(header, area);
}

/// Render every key-registry entry in order, emphasizing selection and staged rows.
fn render_rows(frame: &mut Frame, area: Rect, state: &ConfigState) {
    let header = Row::new(vec!["", "Key", "Value", "Origin", "Help"])
        .style(Theme::header())
        .bottom_margin(1);
    let rows: Vec<Row> = state
        .rows()
        .iter()
        .enumerate()
        .map(|(index, row)| render_row(index, row, state))
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(30),
            Constraint::Length(24),
            Constraint::Length(9),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title("Configuration")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(StatusColors::BORDER)),
    );
    frame.render_widget(table, area);
}

/// Build one row with a two-character selected/modified marker and inline buffer.
fn render_row(index: usize, row: &super::state::ConfigRow, state: &ConfigState) -> Row<'static> {
    let selected = index == state.selected();
    let marker = format!(
        "{}{}",
        if selected { '>' } else { ' ' },
        if row.is_modified() { '*' } else { ' ' },
    );
    let value = if selected {
        state
            .edit_buffer()
            .map(|buffer| format!("{buffer}▏"))
            .unwrap_or_else(|| row.displayed_value().to_owned())
    } else {
        row.displayed_value().to_owned()
    };
    let row_style = if selected {
        Style::default()
            .fg(StatusColors::EXECUTING)
            .add_modifier(Modifier::BOLD)
    } else {
        Theme::status_pending()
    };
    let value_style = if row.is_modified() {
        Style::default().fg(StatusColors::WARNING)
    } else {
        row_style
    };
    Row::new(vec![
        Cell::from(marker).style(row_style),
        Cell::from(row.spec().name).style(row_style),
        Cell::from(value).style(value_style),
        Cell::from(row.origin().to_string()).style(row_style),
        Cell::from(row.spec().help).style(row_style),
    ])
}

/// Render the latest persistence or validation result in its semantic color.
fn render_status(frame: &mut Frame, area: Rect, state: &ConfigState) {
    let style = if state.status_is_error() {
        Theme::status_blocked()
    } else {
        Theme::status_completed()
    };
    let status = Paragraph::new(Line::from(Span::styled(state.status(), style)))
        .block(Block::default().title("Status").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(status, area);
}

/// Render the fixed shortcut reminder, including the staged-row `*` marker.
fn render_footer(frame: &mut Frame, area: Rect) {
    let footer = Line::from(vec![
        Span::styled("↑↓/k/j", Theme::header()),
        Span::raw(" move  "),
        Span::styled("Enter", Theme::header()),
        Span::raw(" edit  "),
        Span::styled("s", Theme::header()),
        Span::raw(" save  "),
        Span::styled("Esc/q", Theme::header()),
        Span::raw(" quit  * pending"),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
}
