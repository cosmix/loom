//! The by-model block of the t pane: plan-wide token totals per model and effort.

use ratatui::text::{Line, Span};

use super::text::{cut_line, padded, text_width};
use super::tokens_pane::compact_tokens;
use crate::commands::status::ui::theme::Theme;
use crate::tokens::ModelUsage;

/// Lines the block may use, its title included.
pub(super) const MODEL_BLOCK_MAX_ROWS: usize = 10;
/// Cells of a share bar.
const BAR_CELLS: usize = 20;
/// Widest model-name column.
const NAME_MAX: usize = 24;
/// Below this pane width the bar is dropped.
const BAR_MIN_WIDTH: u16 = 60;
/// Cells of a row with a bar besides the name: margins, bar, percent and fresh columns.
const BAR_ROW_FIXED_CELLS: u16 = 2 + 2 + BAR_CELLS as u16 + 2 + 4 + 2 + 7;
const EFFORT_CELLS: u16 = 8;

/// One model with its efforts summed, in the order of the input entries.
struct Group<'a> {
    model: &'a str,
    fresh: u64,
    efforts: Vec<&'a ModelUsage>,
}

/// The block for the t pane: a dim title, one row per model with a share bar, and one
/// dim row per effort under it. Empty when no model has fresh tokens.
pub(super) fn model_lines(models: &[ModelUsage], width: u16) -> Vec<Line<'static>> {
    let groups = group_models(models);
    let sum: u64 = groups.iter().fold(0, |acc, g| acc.saturating_add(g.fresh));
    if sum == 0 {
        return Vec::new();
    }
    let name_cells = groups
        .iter()
        .map(|g| text_width(g.model))
        .max()
        .unwrap_or(0)
        .min(NAME_MAX) as u16;
    let mut lines = vec![cut_line(
        Line::from(Span::styled(
            " BY MODEL · fresh so far · share",
            Theme::dimmed(),
        )),
        width,
    )];
    for group in &groups {
        lines.push(cut_line(model_row(group, sum, name_cells, width), width));
        for usage in &group.efforts {
            lines.push(cut_line(effort_row(usage), width));
        }
    }
    lines.truncate(MODEL_BLOCK_MAX_ROWS);
    lines
}

/// Group by model name; models sort by fresh descending, then name ascending.
fn group_models(models: &[ModelUsage]) -> Vec<Group<'_>> {
    let mut groups: Vec<Group<'_>> = Vec::new();
    for usage in models {
        let fresh = usage.total.fresh();
        match groups.iter_mut().find(|g| g.model == usage.model) {
            Some(group) => {
                group.fresh = group.fresh.saturating_add(fresh);
                group.efforts.push(usage);
            }
            None => groups.push(Group {
                model: &usage.model,
                fresh,
                efforts: vec![usage],
            }),
        }
    }
    groups.sort_by(|a, b| b.fresh.cmp(&a.fresh).then_with(|| a.model.cmp(b.model)));
    groups
}

/// `part / whole` scaled to `scale`, rounded half up, in u128 so it cannot overflow.
fn scaled_share(part: u64, whole: u64, scale: u128) -> usize {
    let whole = u128::from(whole);
    ((u128::from(part) * scale * 2 + whole) / (whole * 2)) as usize
}

fn model_row(group: &Group<'_>, sum: u64, name_cells: u16, width: u16) -> Line<'static> {
    let mut text = format!("  {}  ", padded(group.model, name_cells));
    if width >= BAR_MIN_WIDTH.max(name_cells + BAR_ROW_FIXED_CELLS) {
        let filled = scaled_share(group.fresh, sum, BAR_CELLS as u128).min(BAR_CELLS);
        text.push_str(&"█".repeat(filled));
        text.push_str(&"░".repeat(BAR_CELLS - filled));
        text.push_str("  ");
    }
    text.push_str(&format!(
        "{:>4}  {:>7}",
        format!("{}%", scaled_share(group.fresh, sum, 100)),
        compact_tokens(group.fresh)
    ));
    Line::from(text)
}

fn effort_row(usage: &ModelUsage) -> Line<'static> {
    let effort = usage.effort.as_deref().unwrap_or("—");
    Line::from(Span::styled(
        format!(
            "      {}  {} fresh · {} cache read",
            padded(effort, EFFORT_CELLS),
            compact_tokens(usage.total.fresh()),
            compact_tokens(usage.total.cache_read)
        ),
        Theme::dimmed(),
    ))
}

#[cfg(test)]
#[path = "tokens_models_tests.rs"]
mod tests;
