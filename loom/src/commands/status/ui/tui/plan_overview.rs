//! The finished-plan screen of the live TUI: the plan's report in place of the
//! ledger. The body scrolls; the footer stays on the last rows, or ends the body
//! when the terminal is too short to pin it.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::ledger::compact_tokens;
use super::ledger::text::{cut_line, text_width, truncate};
use super::plan_overview_format::{clock, keep_tail, pack_segments};
use super::plan_overview_models::model_lines;
use super::plan_overview_timeline::timeline_lines;
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::plan_stats::{all_tokens, archive_path, loom_home, PlanStatsRecord};
use crate::utils::format_elapsed;

/// Below this many rows the footer scrolls with the body instead of staying put.
const MIN_PINNED_HEIGHT: u16 = 8;
/// Footer rows without the scroll hint: the saved file and the next steps.
const FOOTER_ROWS: u16 = 2;
const TITLE: &str = "PLAN COMPLETE";
/// Narrowest plan name worth keeping beside the counts.
const MIN_NAME_CELLS: usize = 8;

/// What a draw measured, for the scroll keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Metrics {
    /// Rows of the scrolling body.
    pub total_lines: u16,
    /// Rows of the screen the body scrolls in.
    pub viewport: u16,
    /// The scroll offset drawn, clamped to the body.
    pub scroll_y: u16,
}

/// `j` and `k` scroll like the arrow keys; every other key is unchanged.
pub(super) fn scroll_alias(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Char('j') => KeyCode::Down,
        KeyCode::Char('k') => KeyCode::Up,
        other => other,
    }
}

/// Draw the report into `area`, the body from row `scroll_y`.
pub(super) fn render_plan_overview(
    frame: &mut Frame<'_>,
    area: Rect,
    record: &PlanStatsRecord,
    scroll_y: u16,
) -> Metrics {
    let mut body = overview_lines(record, area.width);
    let pinned = area.height >= MIN_PINNED_HEIGHT;
    if !pinned {
        // No room to pin the footer: it ends the body, so scrolling reaches it.
        body.extend(footer_lines(record, area.width, None));
    }
    let total_lines = u16::try_from(body.len()).unwrap_or(u16::MAX);
    // A scrollable body gets one more footer row for its scroll hint.
    let scrollable = pinned && total_lines > area.height.saturating_sub(FOOTER_ROWS);
    let footer_rows = match (pinned, scrollable) {
        (false, _) => 0,
        (true, false) => FOOTER_ROWS,
        (true, true) => FOOTER_ROWS + 1,
    };
    let viewport = area.height.saturating_sub(footer_rows);
    let scroll_y = scroll_y.min(total_lines.saturating_sub(viewport));
    let body_area = Rect {
        height: viewport,
        ..area
    };
    let lines: Vec<Line<'static>> = body
        .into_iter()
        .map(|line| cut_line(line, area.width))
        .collect();
    frame.render_widget(Paragraph::new(lines).scroll((scroll_y, 0)), body_area);
    if pinned {
        let scroll = scrollable.then_some((scroll_y, viewport, total_lines));
        let footer: Vec<Line<'static>> = footer_lines(record, area.width, scroll)
            .into_iter()
            .map(|line| cut_line(line, area.width))
            .collect();
        let footer_area = Rect {
            y: area.y + viewport,
            height: footer_rows,
            ..area
        };
        frame.render_widget(Paragraph::new(footer), footer_area);
    }
    Metrics {
        total_lines,
        viewport,
        scroll_y,
    }
}

fn overview_lines(record: &PlanStatsRecord, width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![title_line(record, width), rule(width)];
    lines.extend(pack_segments(totals(record), width));
    lines.extend(pack_segments(token_split(record), width));
    lines.push(Line::default());
    lines.extend(timeline_lines(record, width));
    let models = model_lines(record, width);
    if !models.is_empty() {
        lines.push(Line::default());
        lines.extend(models);
    }
    lines
}

fn title_line(record: &PlanStatsRecord, width: u16) -> Line<'static> {
    let mut counts = format!("{} completed", record.completed_count());
    if record.skipped_count() > 0 {
        counts.push_str(&format!(" · {} skipped", record.skipped_count()));
    }
    let counts = format!("● {counts}");
    let width = usize::from(width);
    let lead = 1 + TITLE.len() + 2;
    let beside = lead + MIN_NAME_CELLS + 2 + text_width(&counts);
    let (name_cells, show_counts) = if width >= beside {
        (width - lead - 2 - text_width(&counts), true)
    } else {
        (width.saturating_sub(lead), false)
    };
    let name = truncate(&record.plan_name, name_cells);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(
            TITLE,
            Style::default()
                .fg(StatusColors::HEADER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];
    let name_width = text_width(&name);
    spans.push(Span::raw(name));
    if show_counts {
        let gap = width - lead - name_width - text_width(&counts);
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(
            counts,
            Style::default().fg(StatusColors::COMPLETED),
        ));
    }
    Line::from(spans)
}

fn rule(width: u16) -> Line<'static> {
    let cells = usize::from(width).saturating_sub(2);
    Line::from(Span::styled(
        format!(" {}", "─".repeat(cells)),
        Theme::dimmed(),
    ))
}

/// `label value` with the value emphasized.
fn labelled(label: &str, value: String, tail: Option<String>) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled(format!("{label} "), Theme::dimmed()),
        Span::styled(value, Style::default().add_modifier(Modifier::BOLD)),
    ];
    if let Some(tail) = tail {
        spans.push(Span::styled(format!(" {tail}"), Theme::dimmed()));
    }
    spans
}

fn totals(record: &PlanStatsRecord) -> Vec<Vec<Span<'static>>> {
    let range = format!("({}–{})", clock(record.started_at), clock(record.ended_at));
    let hit = record
        .totals
        .cache_hit_percent()
        .map_or_else(|| "-".to_owned(), |percent| format!("{percent}%"));
    vec![
        labelled("wall", format_elapsed(record.wall_secs.max(0)), Some(range)),
        labelled("exec", format_elapsed(record.execution_secs.max(0)), None),
        labelled("fresh", compact_tokens(record.totals.fresh()), None),
        labelled("hit", hit, None),
        labelled("all", compact_tokens(all_tokens(&record.totals)), None),
    ]
}

fn token_split(record: &PlanStatsRecord) -> Vec<Vec<Span<'static>>> {
    let usage = &record.totals;
    [
        ("in", usage.input, StatusColors::EXECUTING),
        ("write", usage.cache_write, StatusColors::BLOCKED),
        ("out", usage.output, StatusColors::COMPLETED),
        ("read", usage.cache_read, StatusColors::DIMMED),
    ]
    .into_iter()
    .map(|(label, count, color)| {
        vec![Span::styled(
            format!("{label} {}", compact_tokens(count)),
            Style::default().fg(color),
        )]
    })
    .collect()
}

/// The saved file, the next steps and, while the body scrolls, where it stands.
fn footer_lines(
    record: &PlanStatsRecord,
    width: u16,
    scroll: Option<(u16, u16, u16)>,
) -> Vec<Line<'static>> {
    let home = loom_home().unwrap_or_else(|_| PathBuf::from("<loom home>"));
    let saved = archive_path(&home, &record.plan_id);
    let saved = keep_tail(
        &format!("saved {}", saved.display()),
        usize::from(width).saturating_sub(1),
    );
    let mut lines = vec![
        Line::from(Span::styled(format!(" {saved}"), Theme::dimmed())),
        Line::from(Span::styled(
            " loom init: next plan · loom clean --state: clear it · q quits",
            Theme::dimmed(),
        )),
    ];
    if let Some((first, viewport, total)) = scroll {
        let last = first.saturating_add(viewport).min(total);
        lines.push(Line::from(Span::styled(
            format!(
                " ↑↓ j/k PgUp/PgDn Home/End scroll · rows {}-{last} of {total}",
                first + 1
            ),
            Theme::dimmed(),
        )));
    }
    lines
}
