# Brief: tui-overview (TUI overview screen)

Tier: `loom-software-engineer` (sonnet). Wave 3 (needs `StatusData.plan_stats` from `wire`). Read `doc/plans/briefs/open-github-issues/common.md` first, and the TUI tab of `doc/plans/briefs/open-github-issues/plan-overview/mockup.html` (the `drawTui` function shows the exact layout).

## Owns

`loom/src/commands/status/ui/tui/plan_overview.rs` (new), `loom/src/commands/status/ui/tui/plan_overview_tests.rs` (new), `loom/src/commands/status/ui/tui/mod.rs`, `loom/src/commands/status/ui/tui/app.rs`.

Frozen, read-only: `loom/src/commands/status/ui/tui/plan_overview_contract_tests.rs`.

## Facts (verified)

- `TuiApp` (`app.rs:53-73`) keeps flags, no view enum. `render()` (`app.rs:323-368`) returns early for `completion_summary` (:325-338, `renderer::render_completion`) before the ledger (:340-363). `app.rs` is 384 lines: keep it under 400 by putting the screen in `plan_overview.rs`.
- Keys: `event_handler.rs:28-73`; `q` exits. Leave keys unchanged.
- Helpers to reuse: `ledger/tokens_pane.rs:40` block ramp, `ledger/tokens_models.rs:31` `model_lines`, `common::model_display::model_with_effort` (`model/effort`).

## Change

1. `plan_overview.rs`: `pub(super) fn render_plan_overview(frame, area, record: &PlanStatsRecord)`: title line "PLAN COMPLETE  <plan name>" with completed and skipped counts; totals line (wall, execution, fresh, hit, all) and the token split line; TIMELINE block with one row per stage: id, a bar over the plan's span with `▓` contract, `█` executing (green), `▒` adjudication, `█` retry (yellow), `░` waiting, `┄ skipped`, then the duration; a key line; MODELS block (model/effort, agents, fresh, cache read, output, share bar); footer naming the saved stats file and "loom init starts the next plan · loom clean clears this one · q quits". Scale bars to the available width; truncate ids with `…` on narrow terminals.
2. `app.rs` `render()`: after the `completion_summary` branch and before the ledger, when the latest status data carries `plan_stats`, draw `render_plan_overview` and return.
3. `mod.rs`: declare the module and test modules.

## Tests (`plan_overview_tests.rs`)

Render into a ratatui `TestBackend` at 120x40 and at 70x24: the title, every stage id, the key and the models rows appear; a skipped stage shows `skipped`; bars stay inside the area.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib commands::status::` passes with the frozen contract.

## Traps

- Ledger tests (`ledger/tests*.rs`) must still pass when `plan_stats` is None.
