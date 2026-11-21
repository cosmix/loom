//! The TIMELINE block of the finished-plan screen: one row per stage, a bar of
//! its phases over the plan's wall-clock span, and the duration.

use chrono::{DateTime, Utc};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::ledger::text::{padded, text_width};
use super::plan_overview_format::{clock, pack_segments};
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::models::stage::phases::Phase;
use crate::plan_stats::{PlanStatsRecord, StageStats};
use crate::utils::format_elapsed;

/// Cells of the duration column (`12h59m`).
const DURATION_CELLS: usize = 6;
/// Narrowest bar the layout keeps before it shortens the id column.
const MIN_BAR_CELLS: usize = 16;
/// Widest stage id column.
const MAX_ID_CELLS: usize = 24;
/// Narrowest id column: it holds the TIMELINE heading.
const MIN_ID_CELLS: usize = 8;
/// Fewest bar cells that still hold both clock labels.
const CLOCK_LABELS_MIN_BAR: usize = 12;

type Cell = (char, Color);

/// What a phase draws as: the glyph and its colour.
fn phase_cell(phase: Phase) -> Cell {
    match phase {
        Phase::Contract => ('▓', StatusColors::CONTRACT),
        Phase::Executing => ('█', StatusColors::COMPLETED),
        Phase::Adjudication => ('▒', StatusColors::EXECUTING),
        Phase::Retry => ('█', StatusColors::WARNING),
        Phase::Waiting => ('░', StatusColors::DIMMED),
    }
}

/// The id and bar columns for a terminal `width` cells wide.
fn columns(record: &PlanStatsRecord, width: u16) -> (usize, usize) {
    let widest = record
        .stages
        .iter()
        .map(|stage| text_width(&stage.id))
        .max()
        .unwrap_or(0)
        .min(MAX_ID_CELLS);
    // Margin, the gap after the id, the gap before the duration, the duration.
    let room = usize::from(width).saturating_sub(3 + DURATION_CELLS);
    let id = widest
        .min(room.saturating_sub(MIN_BAR_CELLS))
        .max(MIN_ID_CELLS);
    (id, room.saturating_sub(id))
}

/// The TIMELINE heading, one row per stage and the colour key.
pub(super) fn timeline_lines(record: &PlanStatsRecord, width: u16) -> Vec<Line<'static>> {
    let (id_cells, bar_cells) = columns(record, width);
    let mut lines = vec![heading(record, id_cells, bar_cells)];
    lines.extend(
        record
            .stages
            .iter()
            .map(|stage| stage_row(record, stage, id_cells, bar_cells)),
    );
    lines.extend(pack_segments(key(), width));
    lines
}

fn heading(record: &PlanStatsRecord, id_cells: usize, bar_cells: usize) -> Line<'static> {
    let label = Style::default()
        .fg(StatusColors::WARNING)
        .add_modifier(Modifier::BOLD);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(padded("TIMELINE", id_cells as u16), label),
        Span::raw(" "),
    ];
    if bar_cells >= CLOCK_LABELS_MIN_BAR {
        let (start, end) = (clock(record.started_at), clock(record.ended_at));
        let gap = bar_cells.saturating_sub(text_width(&start) + text_width(&end));
        spans.push(Span::styled(
            format!("{start}{}{end}", " ".repeat(gap)),
            Theme::dimmed(),
        ));
    }
    Line::from(spans)
}

fn key() -> Vec<Vec<Span<'static>>> {
    [
        (Phase::Contract, "contract"),
        (Phase::Executing, "executing"),
        (Phase::Adjudication, "adjudication"),
        (Phase::Retry, "retry"),
        (Phase::Waiting, "waiting"),
    ]
    .into_iter()
    .map(|(phase, name)| {
        let (glyph, color) = phase_cell(phase);
        vec![
            Span::styled(glyph.to_string(), Style::default().fg(color)),
            Span::styled(format!(" {name}"), Theme::dimmed()),
        ]
    })
    .collect()
}

fn stage_row(
    record: &PlanStatsRecord,
    stage: &StageStats,
    id_cells: usize,
    bar_cells: usize,
) -> Line<'static> {
    let mut spans = vec![
        Span::raw(" "),
        Span::raw(padded(&stage.id, id_cells as u16)),
        Span::raw(" "),
    ];
    if stage.status == "skipped" {
        let text = padded("┄ skipped", bar_cells as u16);
        spans.push(Span::styled(text, Theme::dimmed()));
        return Line::from(spans);
    }
    let extent = extent(record, stage);
    spans.extend(bar_spans(&bar(record, stage, extent, bar_cells)));
    let secs = extent.map_or(stage.execution_secs, |(start, end)| {
        (end - start).num_seconds()
    });
    spans.push(Span::raw(format!(
        " {:>DURATION_CELLS$}",
        format_elapsed(secs.max(0))
    )));
    Line::from(spans)
}

/// First and last instant of a stage: its own timestamps, else its phases'.
fn extent(record: &PlanStatsRecord, stage: &StageStats) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start = stage
        .started_at
        .or_else(|| stage.phases.first().map(|span| span.started_at))?;
    let end = stage
        .completed_at
        .or_else(|| stage.phases.iter().filter_map(|span| span.ended_at).max())
        .unwrap_or(record.ended_at);
    Some((start, end.max(start)))
}

/// The bar of one stage: waiting over its whole extent, each phase over it.
fn bar(
    record: &PlanStatsRecord,
    stage: &StageStats,
    extent: Option<(DateTime<Utc>, DateTime<Utc>)>,
    cells: usize,
) -> Vec<Cell> {
    let mut bar = vec![(' ', Color::Reset); cells];
    let Some((start, end)) = extent else {
        return bar;
    };
    let column = |at: DateTime<Utc>| column_of(record, at, cells);
    fill(
        &mut bar,
        column(start),
        column(end),
        phase_cell(Phase::Waiting),
    );
    for span in &stage.phases {
        let to = span.ended_at.unwrap_or(end);
        fill(
            &mut bar,
            column(span.started_at),
            column(to),
            phase_cell(span.phase),
        );
    }
    bar
}

/// The cell, `0..=cells`, of `at` along the plan's span, rounded half up.
fn column_of(record: &PlanStatsRecord, at: DateTime<Utc>, cells: usize) -> usize {
    let span = (record.ended_at - record.started_at)
        .num_milliseconds()
        .max(1);
    let offset = (at - record.started_at).num_milliseconds().clamp(0, span);
    let scaled = i128::from(offset) * cells as i128 * 2 + i128::from(span);
    (scaled / (i128::from(span) * 2)) as usize
}

/// Paint `from..to`, at least one cell wide, inside the bar.
fn fill(bar: &mut [Cell], from: usize, to: usize, cell: Cell) {
    let from = from.min(bar.len().saturating_sub(1));
    let to = to.max(from + 1).min(bar.len());
    for slot in &mut bar[from..to] {
        *slot = cell;
    }
}

/// Runs of equal cells as one span each.
fn bar_spans(bar: &[Cell]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < bar.len() {
        let cell = bar[index];
        let run = bar[index..]
            .iter()
            .take_while(|next| **next == cell)
            .count();
        let text: String = std::iter::repeat_n(cell.0, run).collect();
        spans.push(Span::styled(text, Style::default().fg(cell.1)));
        index += run;
    }
    spans
}
