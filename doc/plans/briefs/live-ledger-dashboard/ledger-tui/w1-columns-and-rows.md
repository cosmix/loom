# W1 — Columns that adapt to the terminal width, and one row per stage

Tier: codex `gpt-5.6-terra`, effort `xhigh`. Spawned after W0 returns, in parallel with W2 and W3.

## Files you own (write)

- `loom/src/commands/status/ui/tui/ledger/columns.rs` — new
- `loom/src/commands/status/ui/tui/ledger/rows.rs` — new
- `loom/src/commands/status/ui/tui/ledger/cells.rs` — new, if `rows.rs` would pass 400 lines

Each file carries its own `#[cfg(test)] mod tests`.

Read-only: `ledger/mod.rs` (W0's types), `loom/src/commands/status/data/mod.rs`,
`loom/src/commands/status/ui/theme.rs`, `loom/src/models/stage/types.rs` (`StageStatus::icon`,
`label`, `tui_style`), `loom/src/commands/status/render/attention_model.rs` (`failure_label`),
`loom/src/utils.rs` (`format_elapsed`).

## `columns.rs`

```text
pub fn columns_for_width(width: u16) -> Vec<Column>
pub fn header_line(cols: &[Column]) -> Line<'static>
pub fn rule_line(width: u16) -> Line<'static>
pub fn gap_before(kind: ColumnKind) -> u16     // 1 for Merge, 2 otherwise; 0 for the first column
```

Designed widths, in display order:

| kind | header | width |
| --- | --- | --- |
| State | `STATE` | 12 |
| Stage | `STAGE` | 25 |
| DependsOn | `DEPENDS ON` | 16 |
| Models | `MODELS` | 16 |
| Activity | `ACTIVITY` | 14 |
| Context | `CONTEXT` | 9 |
| Time | `TIME` | 7 |
| Merge | `MERGE` | 8 |

Gaps: 2 cells before every column except the first, and 1 cell before `Merge`. Total at full
width: 107 + 13 = 120 = `FULL_WIDTH`.

Algorithm for `columns_for_width(width)`:

1. `width < MIN_COLS` (64) → empty vector (the layout shows the size notice).
2. Start with all eight. While the total exceeds `width`, drop the next kind in this order:
   `Time`, `Models`, `DependsOn`, `Context`. After all four are gone the total is exactly 64
   (State 12 + 2 + Stage 25 + 2 + Activity 14 + 1 + Merge 8), so the loop always terminates.
3. `width > 120`: give `Stage` the surplus up to 15 extra cells, then `DependsOn` the rest.

Tests (`columns.rs`): 120 → 8 columns, total 120; 119 → no Time, total 111; 110 → no Models,
total 93; 92 → no DependsOn, total 75; 74 → no Context, total 64; 63 → empty; 140 → Stage 40,
DependsOn 21, total 140. Write one helper `fn total(cols) -> u16` and assert with it.

`header_line` prints each header padded to its width in `Theme::dimmed()`; `rule_line` is
`─` repeated `width` times in `StatusColors::BORDER`.

## `rows.rs`

```text
pub fn stage_row(stage: &StageSummary, level: usize, cols: &[Column]) -> Line<'static>
```

Every cell is padded (or truncated) to exactly its column width so columns align regardless of
content; truncation keeps `width - 1` characters and appends `…`. Count characters, not bytes.

| kind | content | style |
| --- | --- | --- |
| State | `{icon} {label}` with the label padded to 10 | `status.tui_style()` |
| Stage | `"  "` × `level` then the id | Executing: `Theme::header()`; WaitingForDeps: `Theme::status_pending()`; Skipped: `Theme::dimmed()`; otherwise default |
| DependsOn | empty when no deps; one dep → its id; several → `{first} +{n-1}` with `first` truncated so the suffix fits | `Theme::dimmed()` when every dep's status is Completed (look each up in the row's sibling list — pass `all: &[&StageSummary]` as a fourth argument), else `Theme::status_pending()` |
| Models | `stage.model` alone when `execution_models` is empty; otherwise `{model}›{m1},{m2}` — keep as many execution models as fit, then `+{n}` for the rest | model part `Theme::dimmed()`, execution part default |
| Activity | the table below | per row |
| Context | 5-cell bar `━`/`╌` filled by `tokens / ceiling`, then `NN%` — only when `context_tokens` and `context_ceiling_tokens` are both `Some` and status is Executing, WaitingForInput or NeedsHandoff; empty otherwise | `Theme::context_style(tokens, ceiling)` |
| Time | `format_elapsed(execution_secs.or(elapsed_secs))` for every status except WaitingForDeps and Queued; empty otherwise | `Theme::dimmed()` |
| Merge | Completed: `merged` / `unmerged` (not for `StageType::Knowledge`) / `cleanup!` when `cleanup_warning` is set (cleanup wins); MergeConflict: `conflict`; MergeBlocked: `error`; else empty | merged `Theme::status_merged()`, unmerged and conflict `Theme::status_warning()`, error `Theme::status_blocked()` |

Activity cell by status (`incoherence.is_some()` on an Executing stage renders `incoherent` in
`Theme::status_blocked()` instead of any of these; `held` prefixes `held ·` in
`Theme::status_warning()` to whatever follows):

| status | text | style |
| --- | --- | --- |
| WaitingForDeps, Completed, Skipped | empty | — |
| Queued | `ready` | `Theme::status_queued()` |
| Executing, Working | `working · {last_tool}` (tool cut to fit; `working` alone when None) | `Theme::status_completed()` |
| Executing, Idle | `idle {staleness}` | `Theme::dimmed()` |
| Executing, Stale | `stale {staleness}` | `Theme::status_warning()` |
| Executing, Orphaned | `orphaned` | `Theme::status_blocked()` |
| Executing, Error | `crashed` | `Theme::status_blocked()` |
| WaitingForInput | `awaiting input` | `status.tui_style()` |
| NeedsHandoff | `handing off` | `Theme::status_warning()` |
| Blocked | `{failure_label} {retry_count}/{max}` with `max = max_retries.unwrap_or(3)`; `error` label when `failure_info` is None | `Theme::status_blocked()` |
| CompletedWithFailures | `failed {retry_count}/{max}` | `Theme::status_blocked()` |
| MergeConflict | `conflict` | `Theme::status_warning()` |
| MergeBlocked | `merge error` | `Theme::status_blocked()` |
| NeedsHumanReview | `awaiting you` | `status.tui_style()` |
| NeedsAdjudication | `dispute {dispute_count} · judge {state}` where state is `none` (no heartbeat), `working` (≤ 300 s), `stale` (> 300 s) | `Theme::status_warning()` |

`staleness` is `format_elapsed(staleness_secs)`; when None, omit it. `failure_label` comes from
`attention_model::failure_label` — do not re-declare the table.

Tests (`rows.rs`): one test per non-empty Activity variant; the Models cell for
`model: "opus"`, `execution_models: ["sonnet","terra"]` renders `opus›sonnet,terra`; the Context
cell for 487000/800000 renders three filled cells and `61%`; every cell of a row at width 120 is
exactly its column width (assert the rendered `Line` width equals 120 for a row with long content
and for a row with empty cells).

## Done means

`cargo test --manifest-path loom/Cargo.toml --lib commands::status::ui::tui::ledger::columns::` and
`...::rows::` pass once W0's `mod.rs` and the sibling modules exist. Each file under 400 lines,
each function under 50 — split cell builders into small functions.

## Constraints the graph will not show you

- `ratatui::text::Line` width is `Line::width()`; pad with spaces explicitly, never rely on
  layout to align cells.
- Do not import anything from `tree_widget.rs`; W3 deletes it.
- Do not run `git`, the full test suite, the linter or the formatter.
