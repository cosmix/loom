use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::entries::entry_lines;
use super::legend::LEGEND;
use super::text::{cut_line, spans_width};
use super::{quota, LedgerView};
use crate::commands::status::render::attention_model::AttentionEntry;
use crate::commands::status::ui::theme::Theme;
use crate::commands::status::ui::tui::state::TuiActivityLog;
use crate::models::stage::StageStatus;

/// Render the needs-attention panel.
pub fn render_attention(frame: &mut Frame, area: Rect, entries: &[AttentionEntry]) {
    let lines = fit_to_rows(
        attention_lines(entries, area.width),
        area.height,
        area.width,
    );
    frame.render_widget(Paragraph::new(lines), area);
}

/// `lines` cut to `rows`. A panel the layout gave too few rows for its entries
/// ends in a row counting the lines left out, so nothing goes missing unmarked.
pub(super) fn fit_to_rows(
    mut lines: Vec<Line<'static>>,
    rows: u16,
    width: u16,
) -> Vec<Line<'static>> {
    let rows = usize::from(rows);
    if rows < 2 || lines.len() <= rows {
        return lines;
    }
    let left_out = lines.len() - (rows - 1);
    lines.truncate(rows - 1);
    lines.push(cut_line(
        Line::from(Span::styled(
            format!(" … {left_out} more lines, `loom status` shows them all"),
            Theme::dimmed(),
        )),
        width,
    ));
    lines
}

/// Render the activity panel.
pub fn render_activity(frame: &mut Frame, area: Rect, log: &TuiActivityLog) {
    let mut lines = vec![cut_line(
        Line::from(Span::styled(
            " ACTIVITY",
            Theme::status_pending().add_modifier(Modifier::BOLD),
        )),
        area.width,
    )];
    if log.is_empty() {
        lines.push(cut_line(
            Line::from(Span::styled("Waiting for events...", Theme::dimmed())),
            area.width,
        ));
    } else {
        lines.extend(
            log.render_lines(area.height.saturating_sub(1) as usize)
                .into_iter()
                .map(|line| cut_line(line, area.width)),
        );
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Render the footer: the quota line when the area has a row for it, above the legend
/// strip with its key hints, or the last error in the strip's place.
pub fn render_footer(frame: &mut Frame, area: Rect, present: &[StageStatus], view: &LedgerView) {
    let mut lines = Vec::new();
    if area.height >= 2 {
        lines.push(quota::quota_line(
            &view.data.quota,
            view.now_epoch,
            area.width,
        ));
    }
    lines.push(view.last_error.map_or_else(
        || footer_line(present, area.width, view.scrollable),
        |message| {
            cut_line(
                Line::from(Span::styled(
                    format!("Error: {message}"),
                    Theme::status_blocked(),
                )),
                area.width,
            )
        },
    ));
    frame.render_widget(Paragraph::new(lines), area);
}

/// Build the needs-attention title and detailed entry lines.
pub fn attention_lines(entries: &[AttentionEntry], width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![cut_line(
        Line::from(Span::styled(
            " NEEDS ATTENTION",
            Theme::status_pending().add_modifier(Modifier::BOLD),
        )),
        width,
    )];
    for entry in entries {
        lines.extend(entry_lines(entry, width));
    }
    lines
}

/// Lines the entries take in the needs-attention panel, below its title.
pub fn attention_line_count(entries: &[AttentionEntry], width: u16) -> usize {
    entries
        .iter()
        .map(|entry| entry_lines(entry, width).len())
        .sum()
}

/// Build the legend strip and right-aligned key hints for the footer; the scroll hint
/// is only offered when the table overflows its viewport.
pub fn footer_line(present: &[StageStatus], width: u16, scrollable: bool) -> Line<'static> {
    let keys = key_hints(scrollable);
    let key_width = spans_width(&keys);
    let mut entries: Vec<_> = LEGEND
        .iter()
        .filter(|(status, _)| present.contains(status))
        .map(|(status, _)| legend_entry(status))
        .collect();
    while entries_width(&entries) + key_width > width as usize {
        if entries.pop().is_none() {
            break;
        }
    }
    let mut spans = join_entries(entries);
    let gap = (width as usize).saturating_sub(spans_width(&spans) + key_width);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(keys);
    cut_line(Line::from(spans), width)
}

fn legend_entry(status: &StageStatus) -> Vec<Span<'static>> {
    vec![
        Span::styled(status.icon().to_owned(), status.tui_style()),
        Span::styled(format!(" {}", footer_label(status)), Theme::dimmed()),
    ]
}

fn footer_label(status: &StageStatus) -> String {
    if *status == StageStatus::Completed {
        "done".to_owned()
    } else {
        status.label().to_ascii_lowercase()
    }
}

fn key_hints(scrollable: bool) -> Vec<Span<'static>> {
    let key = Style::default().add_modifier(Modifier::BOLD);
    let mut spans = vec![
        Span::styled("?", key),
        Span::styled(" legend", Theme::dimmed()),
    ];
    if scrollable {
        spans.push(Span::styled(" · ", Theme::dimmed()));
        spans.push(Span::styled("↑↓", key));
        spans.push(Span::styled(" scroll", Theme::dimmed()));
    }
    spans.push(Span::styled(" · ", Theme::dimmed()));
    spans.push(Span::styled("q", key));
    spans.push(Span::styled(" quit", Theme::dimmed()));
    spans
}

fn join_entries(entries: Vec<Vec<Span<'static>>>) -> Vec<Span<'static>> {
    entries
        .into_iter()
        .enumerate()
        .flat_map(|(index, entry)| {
            if index == 0 {
                entry
            } else {
                let mut separated = vec![Span::raw("  ")];
                separated.extend(entry);
                separated
            }
        })
        .collect()
}

fn entries_width(entries: &[Vec<Span<'static>>]) -> usize {
    entries
        .iter()
        .map(|entry| spans_width(entry))
        .sum::<usize>()
        + entries.len().saturating_sub(1) * 2
}

#[cfg(test)]
mod tests {
    use super::footer_line;
    use crate::models::stage::StageStatus;

    #[test]
    fn footer_only_lists_present_statuses_and_keeps_quit_hint() {
        let present = [StageStatus::Executing, StageStatus::Completed];
        let line = footer_line(&present, 100, true).to_string();
        assert!(line.contains("executing"));
        assert!(line.contains("done"));
        assert!(!line.contains("queued"));
        assert!(footer_line(&present, 40, true)
            .to_string()
            .ends_with("q quit"));
    }

    #[test]
    fn scroll_hint_appears_only_when_the_table_overflows() {
        let present = [StageStatus::Executing];
        let scrollable = footer_line(&present, 100, true).to_string();
        let fixed = footer_line(&present, 100, false).to_string();
        assert!(scrollable.contains("↑↓ scroll"));
        assert!(!fixed.contains("↑↓"));
        assert!(fixed.ends_with("? legend · q quit"));
    }
}
