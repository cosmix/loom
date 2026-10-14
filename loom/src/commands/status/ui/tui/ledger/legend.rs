use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use super::text::{self, cut_line};
use crate::commands::status::ui::theme::Theme;
use crate::models::stage::StageStatus;

/// Stage states in display order with their verbatim meanings.
pub const LEGEND: [(StageStatus, &str); 13] = [
    (
        StageStatus::WaitingForDeps,
        "waiting for its dependencies to complete and merge",
    ),
    (
        StageStatus::Queued,
        "ready; the daemon spawns a session when a slot frees up",
    ),
    (
        StageStatus::Executing,
        "a session is working in the stage's worktree",
    ),
    (
        StageStatus::WaitingForInput,
        "the agent is waiting for an answer in its session",
    ),
    (
        StageStatus::NeedsHandoff,
        "context ceiling reached; a fresh session resumes it",
    ),
    (
        StageStatus::Completed,
        "work done and verified — may still be unmerged",
    ),
    (
        StageStatus::Skipped,
        "skipped by you; does not satisfy its dependents",
    ),
    (
        StageStatus::Blocked,
        "stopped; crashes auto-retry, else do what its note asks, then → loom stage retry <id>",
    ),
    (
        StageStatus::CompletedWithFailures,
        "acceptance failed → loom stage retry <id>",
    ),
    (
        StageStatus::MergeConflict,
        "merge conflict; a merge resolver session fixes it",
    ),
    (
        StageStatus::MergeBlocked,
        "merge errored (not a conflict); a resolver session fixes it",
    ),
    (
        StageStatus::NeedsHumanReview,
        "asked for a decision → loom stage human-review <id>",
    ),
    (
        StageStatus::NeedsAdjudication,
        "a disputed criterion (acceptance, wiring or wiring test) awaits the judge's verdict",
    ),
];

/// Rows of the overlay: its 22 content lines and the two border rows, so it fits an
/// 80x24 terminal.
const OVERLAY_MAX_HEIGHT: u16 = 24;

/// Render the centred legend overlay.
pub fn render_legend_overlay(frame: &mut Frame, area: Rect) {
    let popup = overlay_area(area);
    let block = Block::default()
        .title(Span::styled(" Stage states ", Theme::header()))
        .title_bottom(Span::styled(" ? or esc to close ", Theme::dimmed()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::status_pending());
    let inner = block.inner(popup);
    let lines = legend_lines()
        .into_iter()
        .map(|line| cut_line(line, inner.width))
        .collect::<Vec<_>>();
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

/// Build the contents of the stage-state legend.
pub fn legend_lines() -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("")];
    lines.extend(LEGEND.iter().take(7).map(state_line));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  needs you",
        Theme::status_blocked().add_modifier(Modifier::BOLD),
    )));
    lines.extend(LEGEND.iter().skip(7).map(state_line));
    lines.push(Line::from(""));
    lines.extend(activity_lines());
    lines.push(Line::from(vec![
        Span::styled("  context", Theme::dimmed()),
        Span::raw("    resident tokens of the stage's session against its ceiling"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  tokens", Theme::dimmed()),
        Span::raw("     fresh tokens per minute (uncached input + cache writes +"),
    ]));
    lines.push(Line::from(
        "             output), 5 min average; t opens the per-agent pane",
    ));
    lines
}

fn overlay_area(area: Rect) -> Rect {
    let width = area.width.min(78);
    let height = area.height.min(OVERLAY_MAX_HEIGHT);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn state_line((status, meaning): &(StageStatus, &str)) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(status.icon().to_owned(), status.tui_style()),
        Span::raw(" "),
        Span::styled(text::padded(status.label(), 12), status.tui_style()),
        Span::styled((*meaning).to_owned(), Theme::dimmed()),
    ])
}

fn activity_lines() -> [Line<'static>; 2] {
    [
        Line::from(vec![
            Span::styled("  activity", Theme::dimmed()),
            Span::raw("   "),
            Span::styled("working", Theme::status_completed()),
            Span::styled(" · idle · ", Theme::dimmed()),
            Span::styled("stale", Theme::status_warning()),
            Span::styled(" = no heartbeat for 5 min", Theme::dimmed()),
        ]),
        Line::from(vec![
            Span::raw("             "),
            Span::styled("orphaned", Theme::status_blocked()),
            Span::styled(" = executing with no session record", Theme::dimmed()),
        ]),
    ]
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use super::{legend_lines, render_legend_overlay, LEGEND};

    #[test]
    fn legend_has_thirteen_distinct_statuses() {
        assert_eq!(LEGEND.len(), 13);
        for (index, (status, _)) in LEGEND.iter().enumerate() {
            assert!(LEGEND
                .iter()
                .skip(index + 1)
                .all(|(other, _)| other != status));
        }
    }

    #[test]
    fn legend_lines_include_every_status_label() {
        let text = legend_lines()
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        for (status, _) in LEGEND {
            assert!(text.contains(status.label()));
        }
    }

    #[test]
    fn legend_explains_the_tokens_column_within_the_overlay_width() {
        let lines = legend_lines();
        let text: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
        let at = text
            .iter()
            .position(|line| line.starts_with("  tokens"))
            .unwrap();
        assert_eq!(
            text[at],
            "  tokens     fresh tokens per minute (uncached input + cache writes +"
        );
        assert_eq!(
            text[at + 1],
            "             output), 5 min average; t opens the per-agent pane"
        );
        assert!(lines[at..=at + 1].iter().all(|line| line.width() <= 76));
        assert_eq!(lines.len(), usize::from(super::OVERLAY_MAX_HEIGHT) - 2);
    }

    #[test]
    fn every_legend_line_is_visible_at_80x24() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_legend_overlay(frame, area);
            })
            .unwrap();
        let rows: Vec<String> = terminal
            .backend()
            .buffer()
            .content
            .chunks(80)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect();

        // The popup is as tall as the legend plus its two borders, so no line is clipped.
        assert_eq!(legend_lines().len() + 2, 24);
        let last = rows
            .iter()
            .position(|row| row.contains("5 min average; t opens the per-agent pane"))
            .unwrap_or_else(|| panic!("last legend line clipped: {rows:#?}"));
        assert!(rows[last + 1].contains("? or esc to close"), "{rows:#?}");
        assert!(rows[0].contains("Stage states"), "{rows:#?}");
    }
}
