# W0 — `ledger/mod.rs`: the shared types every other worker compiles against

Tier: codex `gpt-5.6-luna`, effort `xhigh`. FOUNDATION: runs alone; W1, W2 and W3 spawn only
after this file exists.

## File you own (write)

- `loom/src/commands/status/ui/tui/ledger/mod.rs` — new

Read-only: `loom/src/commands/status/data/mod.rs`, `loom/src/commands/status/render/attention_model.rs`,
`loom/src/commands/status/ui/tui/state.rs`, `loom/src/orchestrator/scheduling_report.rs` (the `Alert` type).

## What to write, exactly

```text
//! The ledger dashboard behind `loom status --live`: one row per stage with its state
//! spelled out, a needs-attention panel, and a legend on demand.

pub mod columns;
pub mod header;
pub mod layout;
pub mod legend;
pub mod panels;
pub mod rows;
#[cfg(test)]
mod tests;

pub use layout::render;

/// Narrowest terminal the ledger lays out; below this a notice replaces the dashboard.
pub const MIN_COLS: u16 = 64;
pub const MIN_ROWS: u16 = 16;
/// Width at which every column is shown at its designed width.
pub const FULL_WIDTH: u16 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind { State, Stage, DependsOn, Models, Activity, Context, Time, Merge }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column { pub kind: ColumnKind, pub width: u16 }

/// Everything one frame needs, borrowed from `TuiApp`.
pub struct LedgerView<'a> {
    pub data: &'a StatusData,
    pub levels: &'a HashMap<String, usize>,
    /// `data.stages` sorted by level then id (see `LiveStatus::all_stages`).
    pub ordered: &'a [&'a StageSummary],
    pub attention: &'a [AttentionEntry],
    pub activity: &'a TuiActivityLog,
    pub alerts: &'a [Alert],
    pub spinner: char,
    pub scroll_y: u16,
    pub legend_open: bool,
    /// Age of `.loom/work/orchestrator.tick`, when readable.
    pub tick_age_secs: Option<i64>,
    pub last_error: Option<&'a str>,
}

/// What `render` reports back so the app can bound scrolling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderOutcome {
    /// Rows available for stage lines this frame (0 when the size notice rendered).
    pub table_viewport_rows: u16,
}
```

Use the real import paths (`crate::commands::status::data::{StatusData, StageSummary}`,
`crate::commands::status::render::attention_model::AttentionEntry`,
`crate::commands::status::ui::tui::state::TuiActivityLog`,
`crate::orchestrator::scheduling_report::Alert`). The six submodule files and `tests.rs` are written
by other workers; declare them anyway. Also add `pub(crate) mod ledger;` to
`loom/src/commands/status/ui/tui/mod.rs` — that one line is the only edit outside your file.

## Done means

The file exists with exactly these items and doc comments on every public item. It cannot compile
until the sibling files exist; say so in your report instead of stubbing them.

## Constraints

- Do not run `git`, the test suite, the linter or the formatter.
