//! The completion screen's amendment count and left-out tests. The wording
//! matches the foreground summary (`commands/status/render/summary.rs`).

use crate::commands::status::render::summary::{amendments_line, HANDOFFS_HEADING};
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::daemon::CompletionSummary;
use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

/// Hand-off lines shown before the rest collapse into a count, so a long list
/// cannot push the stage table off the screen.
const MAX_HANDOFF_LINES: usize = 5;

/// The lines of the section; empty when the plan has no amendment or hand-off.
fn section_lines(summary: &CompletionSummary) -> Vec<String> {
    let mut lines = Vec::new();
    if summary.amendments > 0 {
        lines.push(amendments_line(summary.amendments));
    }
    if !summary.handoffs.is_empty() {
        lines.push(HANDOFFS_HEADING.to_string());
        lines.extend(
            summary
                .handoffs
                .iter()
                .take(MAX_HANDOFF_LINES)
                .map(|line| format!("  {line}")),
        );
        let hidden = summary.handoffs.len().saturating_sub(MAX_HANDOFF_LINES);
        if hidden > 0 {
            lines.push(format!("  ... and {hidden} more"));
        }
    }
    lines
}

/// Rows the section needs, borders included; zero when it has nothing to show.
pub(super) fn section_height(summary: &CompletionSummary) -> u16 {
    match section_lines(summary).len() {
        0 => 0,
        rows => rows as u16 + 2,
    }
}

/// Draw the section in `area`, sized by [`section_height`].
pub(super) fn render_handoffs(frame: &mut Frame, area: Rect, summary: &CompletionSummary) {
    let lines = section_lines(summary);
    if lines.is_empty() {
        return;
    }
    let block = Block::default()
        .title(" Amendments and left-out tests ")
        .title_style(Theme::header())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));
    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
