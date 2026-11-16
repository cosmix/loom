//! Cell widths of the live tokens pane's columns.

use chrono::{DateTime, Utc};

use super::text::spans_width;
use super::tokens_jobs::{jobs_of, state_text};
use super::tokens_pane::compact_tokens;
use crate::commands::status::data::StageSummary;
use crate::tokens::StageFlow;
use ratatui::text::Span;

/// Preferred and smallest cell widths of the series strip, the agent label and the
/// model column.
pub(super) const SERIES_CELLS: u16 = 48;
pub(super) const SERIES_MIN: u16 = 12;
pub(super) const LABEL_CELLS: u16 = 30;
pub(super) const LABEL_MIN: u16 = 12;
pub(super) const MODEL_CELLS: u16 = 24;
pub(super) const MODEL_MIN: u16 = 8;
/// Cells an agent row spends outside the label, model, strip, state and total columns:
/// the two-space indent, the two gaps and the gap before the total.
pub(super) const AGENT_FIXED: u16 = 2 + 1 + 1 + 1;
/// The state column (`live `, `idle `) is at least this many cells; a native Codex job
/// widens it to hold its outcome and duration.
pub(super) const STATE_MIN: u16 = 5;
/// Cells a stage line spends outside its id, sparkline and rate: the indent and the
/// ` {rate}/m  ` text around the rate.
pub(super) const STAGE_FIXED: u16 = 1 + 1 + 2 + 2;
/// The agent total and stage rate slots are at least this many cells wide.
pub(super) const SLOT_MIN: u16 = 5;

/// Cell widths of the pane's columns at one pane width.
pub(super) struct Columns {
    pub(super) stage_id: u16,
    pub(super) rate: u16,
    pub(super) spark: u16,
    pub(super) label: u16,
    pub(super) model: u16,
    pub(super) state: u16,
    pub(super) strip: u16,
}

impl Columns {
    /// The columns for the rows of `stages`, with the slots the widest agent total, stage
    /// rate and Codex job state among them need.
    pub(super) fn for_stages(
        width: u16,
        stages: &[&StageFlow],
        summaries: &[StageSummary],
        now: DateTime<Utc>,
    ) -> Self {
        let totals = stages.iter().flat_map(|stage| &stage.agents);
        let states = stages
            .iter()
            .flat_map(|stage| jobs_of(summaries, &stage.stage_id))
            .map(|job| text_cells(&state_text(job, now).0) + 1);
        Self::fit(
            width,
            slot_cells(totals.map(|agent| agent.total.fresh())),
            slot_cells(stages.iter().map(|stage| stage.rate.fresh())),
            states.fold(STATE_MIN, u16::max),
        )
    }

    /// Every row of the pane uses these widths, so the strips line up. `total` and `rate`
    /// are the cells the widest agent total and stage rate need, so a long number never
    /// pushes the newest cell of a strip off the row, and `state` the cells the longest
    /// state text needs. The model column gives way first, then the strip, then the
    /// label; each stops at its minimum.
    pub(super) fn fit(width: u16, total: u16, rate: u16, state: u16) -> Self {
        let [model, strip, label] = shrink(
            width.saturating_sub(AGENT_FIXED + state + total),
            [
                (MODEL_CELLS, MODEL_MIN),
                (SERIES_CELLS, SERIES_MIN),
                (LABEL_CELLS, LABEL_MIN),
            ],
        );
        let [spark, stage_id] = shrink(
            width.saturating_sub(STAGE_FIXED + rate),
            [(SERIES_CELLS, SERIES_MIN), (LABEL_CELLS, LABEL_MIN)],
        );
        Self {
            stage_id,
            rate,
            spark,
            label,
            model,
            state,
            strip,
        }
    }
}

fn text_cells(text: &str) -> u16 {
    u16::try_from(spans_width(&[Span::raw(text.to_owned())])).unwrap_or(u16::MAX)
}

/// Cells the widest of `values` needs once compacted, at least [`SLOT_MIN`].
fn slot_cells(values: impl Iterator<Item = u64>) -> u16 {
    // `compact_tokens` is ASCII and at most 18 cells, so the cast is exact.
    let widest = values.map(|n| compact_tokens(n).len() as u16).max();
    widest.unwrap_or(0).max(SLOT_MIN)
}

/// Fit `(preferred, minimum)` columns into `room` cells. They shrink in array order,
/// each no further than its minimum.
pub(super) fn shrink<const N: usize>(room: u16, columns: [(u16, u16); N]) -> [u16; N] {
    let wanted: u16 = columns.iter().map(|&(preferred, _)| preferred).sum();
    let mut excess = wanted.saturating_sub(room);
    columns.map(|(preferred, minimum)| {
        let cut = excess.min(preferred - minimum);
        excess -= cut;
        preferred - cut
    })
}
