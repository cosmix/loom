//! The MODELS block of the finished-plan screen: tokens per model and effort.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::ledger::compact_tokens;
use super::ledger::text::{padded, text_width};
use crate::commands::common::model_display::model_with_effort;
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::plan_stats::{ModelStats, PlanStatsRecord};
use crate::tokens::share_percent;

/// Cells of the agents, fresh, cache read and output columns.
const AGENTS_CELLS: usize = 6;
const FRESH_CELLS: usize = 8;
const READ_CELLS: usize = 10;
const OUTPUT_CELLS: usize = 8;
/// The four number columns and the gap before each.
const NUMBER_CELLS: usize = 4 + AGENTS_CELLS + FRESH_CELLS + READ_CELLS + OUTPUT_CELLS;
/// Widest model column.
const MAX_NAME_CELLS: usize = 30;
/// Narrowest model column once the numbers leave less room than the names want.
const MIN_NAME_CELLS: usize = 6;
/// The gap before the bar, the gap before the percent and the percent (`100%`).
const SHARE_FIXED_CELLS: usize = 6;
const MIN_BAR_CELLS: usize = 6;
const MAX_BAR_CELLS: usize = 16;

/// The block, or nothing when the plan used no model.
pub(super) fn model_lines(record: &PlanStatsRecord, width: u16) -> Vec<Line<'static>> {
    let mut rows: Vec<(String, &ModelStats)> = record
        .models
        .iter()
        .map(|model| {
            (
                model_with_effort(&model.model, model.effort.as_deref()),
                model,
            )
        })
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    rows.sort_by(|a, b| {
        b.1.tokens
            .fresh()
            .cmp(&a.1.tokens.fresh())
            .then_with(|| a.0.cmp(&b.0))
    });
    let total: u64 = rows.iter().fold(0, |sum, (_, model)| {
        sum.saturating_add(model.tokens.fresh())
    });
    let (name_cells, bar_cells) = columns(&rows, width);
    let mut lines = vec![heading(name_cells, bar_cells > 0)];
    lines.extend(
        rows.iter()
            .map(|(name, model)| row(name, model, total, name_cells, bar_cells)),
    );
    lines
}

/// The model and share-bar columns for a terminal `width` cells wide; the bar is
/// zero cells when the share does not fit.
fn columns(rows: &[(String, &ModelStats)], width: u16) -> (usize, usize) {
    let widest = rows
        .iter()
        .map(|(name, _)| text_width(name))
        .max()
        .unwrap_or(0)
        .min(MAX_NAME_CELLS);
    let room = usize::from(width).saturating_sub(1 + NUMBER_CELLS);
    let name = widest.min(room.max(MIN_NAME_CELLS));
    let rest = room.saturating_sub(name);
    let bar = if rest >= SHARE_FIXED_CELLS + MIN_BAR_CELLS {
        (rest - SHARE_FIXED_CELLS).min(MAX_BAR_CELLS)
    } else {
        0
    };
    (name, bar)
}

fn heading(name_cells: usize, with_share: bool) -> Line<'static> {
    let label = Style::default()
        .fg(StatusColors::WARNING)
        .add_modifier(Modifier::BOLD);
    let mut numbers = format!(
        " {:>AGENTS_CELLS$} {:>FRESH_CELLS$} {:>READ_CELLS$} {:>OUTPUT_CELLS$}",
        "agents", "fresh", "cache read", "output"
    );
    if with_share {
        numbers.push_str(" share");
    }
    Line::from(vec![
        Span::raw(" "),
        Span::styled(padded("MODELS", name_cells as u16), label),
        Span::styled(numbers, Theme::dimmed()),
    ])
}

fn row(
    name: &str,
    model: &ModelStats,
    total: u64,
    name_cells: usize,
    bar_cells: usize,
) -> Line<'static> {
    let usage = &model.tokens;
    let numbers = format!(
        " {:>AGENTS_CELLS$} {:>FRESH_CELLS$} {:>READ_CELLS$} {:>OUTPUT_CELLS$}",
        model.agents,
        compact_tokens(usage.fresh()),
        compact_tokens(usage.cache_read),
        compact_tokens(usage.output)
    );
    let mut spans = vec![
        Span::raw(" "),
        Span::raw(padded(name, name_cells as u16)),
        Span::raw(numbers),
    ];
    if bar_cells > 0 {
        let share = share_percent(usage.fresh(), total).map_or(0, u64::from);
        let filled = if usage.fresh() == 0 {
            0
        } else {
            (share as usize * bar_cells)
                .div_ceil(100)
                .clamp(1, bar_cells)
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "▇".repeat(filled),
            Style::default().fg(StatusColors::COMPLETED),
        ));
        spans.push(Span::raw(" ".repeat(bar_cells - filled)));
        spans.push(Span::styled(format!(" {share:>3}%"), Theme::dimmed()));
    }
    Line::from(spans)
}
