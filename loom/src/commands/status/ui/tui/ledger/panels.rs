use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::legend::LEGEND;
use super::text::{self, cut_line, spans_width};
use super::{quota, LedgerView};
use crate::commands::status::render::attention_model::{failure_label, AttentionEntry};
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::commands::status::ui::tui::state::TuiActivityLog;
use crate::models::stage::StageStatus;

/// Render the needs-attention panel.
pub fn render_attention(frame: &mut Frame, area: Rect, entries: &[AttentionEntry]) {
    frame.render_widget(Paragraph::new(attention_lines(entries, area.width)), area);
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

fn entry_lines(entry: &AttentionEntry, width: u16) -> Vec<Line<'static>> {
    let status = entry_status(entry.label);
    let detail = attention_detail(entry);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(status.icon().to_owned(), status.tui_style()),
        Span::raw(format!(" {} ", text::padded(&entry.id, 20))),
        Span::styled(
            entry.label.to_owned(),
            status.tui_style().add_modifier(Modifier::BOLD),
        ),
    ];
    if !detail.is_empty() {
        spans.push(Span::raw(" · "));
        spans.push(Span::raw(detail));
    }
    let mut lines = vec![cut_line(Line::from(spans), width)];
    if let Some(evidence) = entry.evidence.first() {
        lines.push(cut_line(
            Line::from(Span::styled(
                format!("                       {evidence}"),
                Theme::dimmed(),
            )),
            width,
        ));
    }
    lines.push(hint_line(entry, width));
    lines
}

/// `cleanup_warning` is already flattened to one line (and capped at
/// `MAX_INLINE_CHARS`) by `context::untrusted::inline_safe` before it reaches
/// a `StageSummary`, so it is used directly rather than split on newlines.
fn attention_detail(entry: &AttentionEntry) -> String {
    entry
        .review_reason
        .clone()
        .or_else(|| {
            entry
                .failure_type
                .as_ref()
                .map(failure_label)
                .map(str::to_owned)
        })
        .or_else(|| entry.cleanup_warning.clone())
        .unwrap_or_default()
}

fn hint_line(entry: &AttentionEntry, width: u16) -> Line<'static> {
    let mut spans = vec![
        Span::styled("                       → ", Theme::dimmed()),
        Span::styled(
            entry.hint.clone(),
            Style::default().fg(StatusColors::QUEUED),
        ),
    ];
    if entry.has_human_review_choices {
        spans.push(Span::styled(
            format!(" {}", human_review_choices().join(" | ")),
            Theme::dimmed(),
        ));
    }
    cut_line(Line::from(spans), width)
}

fn human_review_choices() -> [&'static str; 3] {
    ["--approve", "--reject <reason>", "--force-complete"]
}

fn entry_status(label: &str) -> StageStatus {
    match label {
        "MERGE CONFLICT" => StageStatus::MergeConflict,
        "ACCEPTANCE FAILED" => StageStatus::CompletedWithFailures,
        "MERGE ERROR" => StageStatus::MergeBlocked,
        "NEEDS REVIEW" => StageStatus::NeedsHumanReview,
        "NEEDS INPUT" => StageStatus::WaitingForInput,
        "ADJUDICATING" => StageStatus::NeedsAdjudication,
        _ => StageStatus::Blocked,
    }
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
    use super::{attention_lines, footer_line};
    use crate::commands::status::render::attention_model::AttentionEntry;
    use crate::models::stage::StageStatus;

    #[test]
    fn human_review_attention_has_choices_on_its_hint_line() {
        let entry = AttentionEntry {
            id: "review".into(),
            name: "Review".into(),
            label: "NEEDS REVIEW",
            hint: "review this stage".into(),
            failure_type: None,
            evidence: Vec::new(),
            review_reason: Some("ambiguous result".into()),
            cleanup_warning: None,
            has_human_review_choices: true,
            dispute_count: None,
            judge_heartbeat_secs: None,
        };
        let lines = attention_lines(&[entry], 120);
        assert_eq!(lines.len(), 3);
        assert!(lines[2].to_string().ends_with("--force-complete"));
    }

    #[test]
    fn entry_without_detail_omits_the_dangling_separator() {
        let entry = AttentionEntry {
            id: "s-input".into(),
            name: "Input".into(),
            label: "NEEDS INPUT",
            hint: "resume the stage".into(),
            failure_type: None,
            evidence: Vec::new(),
            review_reason: None,
            cleanup_warning: None,
            has_human_review_choices: false,
            dispute_count: None,
            judge_heartbeat_secs: None,
        };
        let lines = attention_lines(&[entry], 120);
        let detail_line = lines[1].to_string();
        assert!(detail_line.ends_with("NEEDS INPUT"));
        assert!(!detail_line.contains(" · "));
    }

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
