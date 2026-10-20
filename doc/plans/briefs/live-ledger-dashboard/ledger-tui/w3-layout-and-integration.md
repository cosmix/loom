# W3 — Vertical layout, the size notice, `?` legend, app integration, tree-widget removal

Tier: codex `gpt-5.6-terra`, effort `xhigh`. Spawned after W0 returns, in parallel with W1 and W2;
your layout compiles only once their modules exist.

## Files you own (write)

- `loom/src/commands/status/ui/tui/ledger/layout.rs` — new
- `loom/src/commands/status/ui/tui/app.rs`
- `loom/src/commands/status/ui/tui/event_handler.rs`
- `loom/src/commands/status/ui/tui/renderer.rs`
- `loom/src/commands/status/ui/mod.rs`
- DELETE `loom/src/commands/status/ui/tree_widget.rs` and `loom/src/commands/status/ui/tree_widget_tests.rs`

Read-only: `ledger/mod.rs`, `ledger/columns.rs`, `ledger/rows.rs`, `ledger/header.rs`,
`ledger/panels.rs`, `ledger/legend.rs` (their public signatures), `ui/tui/state.rs`,
`loom/src/orchestrator/tick.rs` (`read`, `Tick::age_secs`), `loom/src/commands/status/render/attention_model.rs`.

## `layout.rs` — `pub fn render(frame: &mut Frame, view: &LedgerView) -> RenderOutcome`

1. `area = frame.area()`. When `area.width < MIN_COLS || area.height < MIN_ROWS`: render two
   centred lines — `loom status --live needs a terminal of at least 64×16; this one is {w}×{h}`
   in `Theme::status_warning()` and `q quit` dimmed — and return `table_viewport_rows: 0`. The
   subscription keeps running; the next frame after a resize lays out normally (ratatui's
   `Terminal::draw` autoresizes; nothing else is needed).
2. Vertical budget, top to bottom: header 4; one blank; alert band = `min(alerts.len(), 4)` rows
   (0 when there are none — today's reserved blank row goes away) followed by one blank when
   non-zero; the table; one blank; attention = `0` when no entries else `min(1 + 3 × entries, 10)`;
   one blank when non-zero; activity = `0` when the log is empty else `min(1 + len, 6)`; one blank
   when non-zero; footer 1. The table needs at least 6 rows (header line, rule, four stages).
   When the budget does not leave 6: shrink activity to 0, then cap attention at 4, then let the
   table take what remains and scroll. Express this as a pure
   `fn budget(height: u16, alerts: usize, attention_entries: usize, activity_len: usize) -> Budget`
   with a struct of the five heights, and unit-test it (40 rows with everything → table gets the
   rest; 16 rows with two attention entries and three activity lines → activity 0, attention 4,
   table ≥ 6).
3. Render: `header::render_header`; `renderer::render_scheduler_alerts` for the alert band (it stays
   in `renderer.rs`); the table = `columns::header_line`, `columns::rule_line`, then
   `rows::stage_row` for `view.ordered.iter().skip(view.scroll_y as usize)` with each stage's
   level from `view.levels`; `panels::render_attention`; `panels::render_activity`;
   `panels::render_footer` with `present` = the distinct statuses in `view.ordered` and
   `view.last_error`. When `view.legend_open`, `legend::render_legend_overlay(frame, area)` last.
4. Return `table_viewport_rows` = the table height minus 2.

Move `ALERT_MAX_HEIGHT` here from `app.rs`; delete `ACTIVITY_MIN_HEIGHT`/`ACTIVITY_MAX_HEIGHT`.

## `app.rs`

- New fields on `TuiApp`: `legend_open: bool`, `tick_age_secs: Option<i64>`.
- `refresh_alerts` also refreshes `tick_age_secs` on the same one-second throttle:
  `crate::orchestrator::tick::read(work_path).ok().flatten().map(|t| t.age_secs(Utc::now()))`.
- `render`: build `levels` (`self.status.compute_levels()`), `ordered` (`all_stages()`),
  `attention` (`attention_entries(&self.status.data.stages)`), then a `LedgerView`, call
  `ledger::render`, and set `graph_state.total_lines = ordered.len()` and
  `graph_state.viewport_height = outcome.table_viewport_rows`. Delete the old arithmetic that
  derived the viewport from fixed heights. The completion-screen branch stays exactly as it is.
- Keep `render` under 50 lines by moving view assembly into a helper.

## `event_handler.rs`

Add `KeyEventResult::ToggleLegend` and `KeyEventResult::CloseLegend`; `?` returns `ToggleLegend`;
`Esc` returns `CloseLegend` — `app.rs` treats `CloseLegend` as "close the legend if open, else
exit", so `Esc` on the dashboard still quits as today. `q`, Ctrl-C, scrolling and mouse are unchanged.
While the legend is open, scroll keys are ignored (handle in `app.rs`).

## `renderer.rs`

Delete `render_compact_header`, `progress_bar_compact`, `stage_summary_to_stage`,
`render_tree_graph`, `render_activity_log`, `render_compact_footer` and their tests. Keep
`render_scheduler_alerts` and `render_completion` (and the `format_elapsed` test if it tests
`utils`). Remove imports that become unused.

## Deletions

`tree_widget.rs` and `tree_widget_tests.rs` have no consumer once `render_tree_graph` is gone
(verified 2026-09-03: only `renderer.rs` and `ui/mod.rs` referenced them). Delete both, remove
`pub mod tree_widget;` and the `pub use tree_widget::{...}` line from `ui/mod.rs`.

## Tests

`layout.rs`: the `budget` cases above, plus `notice_below_minimum` using
`ratatui::backend::TestBackend::new(60, 20)` and a minimal `LedgerView` over an empty `StatusData`:
the buffer contains `at least 64×16`. (W4 writes the full-screen tests; keep yours to the budget
and the notice.)

## Done means

`cargo build --all-targets --manifest-path loom/Cargo.toml` and
`cargo test --manifest-path loom/Cargo.toml --lib commands::status::ui::` pass once W1's and W2's modules exist.
Report the new line counts of `app.rs` and `renderer.rs` and every function you touched; the
orchestrator reconciles `loom/maintainability-baseline.txt` (entries for `tree_widget.rs`,
`build_lines`, `stage_info_to_stage`, `render`, `run_event_loop`, and the two file entries).

## Constraints the graph will not show you

- `app.rs` (410) and `renderer.rs` (414) may only shrink.
- Do not run `git`, the full test suite, the linter or the formatter.
