# W4 — Full-screen render tests through `TestBackend`

Tier: codex `gpt-5.6-luna`, effort `xhigh`. Spawned after W1, W2 and W3 have all returned and the
crate builds.

## File you own (write)

- `loom/src/commands/status/ui/tui/ledger/tests.rs` — new (declared by W0's `mod.rs` as
  `#[cfg(test)] mod tests;`)

Read-only: every file under `ledger/`, `loom/src/commands/status/data/mod.rs`,
`loom/src/commands/status/render/attention_model.rs`, `loom/src/commands/status/ui/tui/state.rs`,
`loom/src/plan/graph/levels.rs`.

## Harness

`ratatui::backend::TestBackend` (re-exported from `ratatui-core`; `TestBackend::new(width, height)`,
`Terminal::new(backend)`, `terminal.draw(|f| { ledger::render(f, &view); })`, then
`terminal.backend().buffer()`). Write one helper:

```text
fn screen(width: u16, height: u16, view: &LedgerView) -> Vec<String>   // one String per row
fn contains(rows: &[String], needle: &str) -> bool
```

Fixture `fn fixture() -> StatusData`: thirteen `StageSummary` values, one per `StageStatus`, ids
`s-waiting`, `s-queued`, `s-executing`, `s-input`, `s-handoff`, `s-completed`, `s-skipped`,
`s-blocked`, `s-failed`, `s-conflict`, `s-mergeblk`, `s-review`, `s-adjudicate`; `s-executing`
carries `model: "opus"`, `execution_models: ["sonnet","terra"]`, `context_tokens: Some(487_000)`,
`context_ceiling_tokens: Some(800_000)`, `activity_status: Working`, `last_tool: Some("Edit")`;
`s-review` carries `review_reason: Some("which wins?")`; `s-adjudicate` `dispute_count: 2`;
`s-input` `pid: Some(4242)`. `progress` counted with `StageStatus::bucket()`, `merge` with
`s-completed` in `merged`, `plan_name: Some("embed-assets-self-update")`.

Build the view like `app.rs` does: `levels` via `levels::compute_all_levels`, `ordered` sorted by
level then id, `attention` via `attention_entries`, an empty `TuiActivityLog`, no alerts,
spinner `⠋`, `tick_age_secs: Some(2)`.

## Tests

| name | size | asserts |
| --- | --- | --- |
| `renders_all_thirteen_states_at_full_width` | 120×40 | every `StageStatus::label()` appears; headers `MODELS`, `CONTEXT`, `TIME` present; a row contains `opus›sonnet,terra` and `61%`; `NEEDS ATTENTION`, `NEEDS ADJUDICATION`, `NEEDS INPUT`, `NEEDS REVIEW` present |
| `drops_columns_in_priority_order` | 110×40 and 74×40 | at 110: no `TIME`, no `MODELS`, `DEPENDS ON` present; at 74: no `CONTEXT`, `MERGE` present |
| `shows_notice_below_minimum_size` | 60×20 and 120×12 | contains `at least 64×16` and `q quit`; no stage id rendered |
| `legend_overlay_lists_every_state` | 120×40, `legend_open: true` | `Stage states` and all thirteen labels present |
| `footer_lists_only_present_states` | 120×40 with a two-stage fixture (Executing, Completed) | footer row contains `executing` and `done`, not `queued` |
| `header_counts_match_progress` | 120×40 | compute the expected `executing`, `queued`, `waiting` and `done` numbers from the fixture (`bucket()` and status counts) rather than hard-coding them, and assert row 2 contains each `{n} {word}` pair; assert the `need attention` count equals `attention_entries(..).len()` |
| `wide_terminal_widens_stage_column` | 140×40 | the header row has `DEPENDS ON` starting at a larger column than at 120 (compare indices) |

## Done means

`cargo test --manifest-path loom/Cargo.toml --lib commands::status::ui::tui::ledger::tests::` passes.
File under 400 lines; one fixture builder, small assertions.

## Constraints the graph will not show you

- Compare rendered text by trimming trailing spaces per row; never assert on ANSI styling.
- Do not run `git`, the full test suite, the linter or the formatter.
