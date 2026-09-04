//! The ledger dashboard behind `loom status --live`: one row per stage with its state
//! spelled out, a needs-attention panel, and a legend on demand.

use std::collections::HashMap;

use crate::commands::status::data::{StageSummary, StatusData};
use crate::commands::status::render::attention_model::AttentionEntry;
use crate::commands::status::ui::tui::state::TuiActivityLog;
use crate::orchestrator::scheduling_report::Alert;

/// Per-cell content builders for a ledger row.
pub mod cells;
/// Ledger table column definitions.
pub mod columns;
/// Ledger dashboard header rendering.
pub mod header;
/// Ledger dashboard layout rendering.
pub mod layout;
/// Ledger dashboard legend rendering.
pub mod legend;
/// Ledger dashboard attention panels.
pub mod panels;
/// Ledger dashboard row rendering.
pub mod rows;
#[cfg(test)]
mod tests;
/// Cell width and truncation helpers.
pub mod text;

/// Render the ledger dashboard.
pub use layout::render;

/// Narrowest terminal the ledger lays out; below this a notice replaces the dashboard.
pub const MIN_COLS: u16 = 64;
/// Minimum terminal height required for the ledger dashboard.
pub const MIN_ROWS: u16 = 16;
/// Width at which every column is shown at its designed width.
pub const FULL_WIDTH: u16 = 120;

/// The kind of data shown in a ledger column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    /// Stage state.
    State,
    /// Stage identifier and name.
    Stage,
    /// Stage dependencies.
    DependsOn,
    /// Models assigned to the stage.
    Models,
    /// Current stage activity.
    Activity,
    /// Stage context usage.
    Context,
    /// Stage elapsed time.
    Time,
    /// Stage merge state.
    Merge,
}

/// A ledger column and its current width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    /// The data represented by the column.
    pub kind: ColumnKind,
    /// The column width in terminal cells.
    pub width: u16,
}

/// Everything one frame needs, borrowed from `TuiApp`.
pub struct LedgerView<'a> {
    /// The current status data.
    pub data: &'a StatusData,
    /// The execution level for each stage.
    pub levels: &'a HashMap<String, usize>,
    /// `data.stages` sorted by level then id (see `LiveStatus::all_stages`).
    pub ordered: &'a [&'a StageSummary],
    /// Stages that need human attention.
    pub attention: &'a [AttentionEntry],
    /// The stage activity log.
    pub activity: &'a TuiActivityLog,
    /// Scheduling alerts.
    pub alerts: &'a [Alert],
    /// The current spinner character.
    pub spinner: char,
    /// The vertical scroll offset.
    pub scroll_y: u16,
    /// Whether the legend is open.
    pub legend_open: bool,
    /// Age of `.loom/work/orchestrator.tick`, when readable.
    pub tick_age_secs: Option<i64>,
    /// The most recent error, if any.
    pub last_error: Option<&'a str>,
}

/// What `render` reports back so the app can bound scrolling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderOutcome {
    /// Rows available for stage lines this frame (0 when the size notice rendered).
    pub table_viewport_rows: u16,
}
