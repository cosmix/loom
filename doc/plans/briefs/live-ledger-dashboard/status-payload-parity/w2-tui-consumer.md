# W2 — The existing live TUI keeps working on the new payload

Tier: codex `gpt-5.6-terra`, effort `xhigh`. Spawned after W1 returns.

## Files you own (write)

- `loom/src/commands/status/ui/tui/state.rs`
- `loom/src/commands/status/ui/tui/app.rs`
- `loom/src/commands/status/ui/tui/renderer.rs`

Read-only: `loom/src/commands/status/data/mod.rs` (W1's new shape), `loom/src/commands/status/ui/tree_widget.rs`,
`loom/src/models/stage/types.rs`, `loom/src/plan/graph/levels.rs`.

## Entry points

- `LiveStatus`, `TuiActivityLog::update` in `state.rs`.
- `TuiApp::handle_response`, `TuiApp::render` in `app.rs`.
- `stage_info_to_stage`, `render_tree_graph` in `renderer.rs`.

## Why

W1 replaced `Response::StatusUpdate`'s four `Vec<StageInfo>` with `data: StatusData`. This worker
adapts the three TUI files so the dashboard renders exactly the rows it rendered before, now from
`StageSummary`. The next stage replaces this renderer wholesale; keep the change minimal and do not
redesign anything.

## What to build

### `state.rs`

- `pub struct LiveStatus { pub data: StatusData }`. `StatusData` has no `Default`; implement
  `Default for LiveStatus` by hand (empty `stages`, empty `MergeSummary` vectors, zeroed
  `ProgressSummary`, `plan_name: None`).
- `total()` returns `data.progress.total`; `progress_pct()` is `completed / total` from
  `data.progress`, `0.0` when total is zero.
- `compute_levels()` calls `levels::compute_all_levels(&self.data.stages, |s| s.id.as_str(), |s| &s.dependencies)`
  — the same call `render/graph.rs::compute_stage_levels` already makes on `StageSummary`.
- `all_stages() -> Vec<&StageSummary>` sorted by level then id. The dedupe set is no longer needed;
  drop it.
- `TuiActivityLog::update(&mut self, stages: &[&StageSummary])` — identical transition logic keyed
  on `stage.status`, `stage.id`. `update` carries an exact ledger entry at 57 lines; do not grow it.
- Rewrite the test fixtures (the `StageInfo { .. }` literals near the bottom of the file) as one
  local helper `fn summary(id: &str, deps: &[&str], status: StageStatus) -> StageSummary` with every
  field defaulted, and keep every existing test's assertions unchanged.

### `app.rs`

- `handle_response`: `Response::StatusUpdate { data } => { self.status = LiveStatus { data }; ... }`
  then the same activity-log update and `last_error = None`.
- `render`: `stages_for_graph` maps `all_stages()` through `stage_summary_to_stage`.
- `completed_count` becomes `status.data.progress.completed`.

### `renderer.rs`

Rename `stage_info_to_stage` to `pub fn stage_summary_to_stage(s: &StageSummary) -> Stage` and map
every field the tree widget reads (read `tree_widget.rs::build_lines`, `compute_stage_elapsed`,
`merge_status_spans` to see which): `id`, `name`, `status`, `dependencies`, `stage_type`, `held`,
`execution_secs`, `duration_secs` (from `execution_secs` when the stage is Completed), `retry_count`,
`max_retries`, `failure_info`, `base_branch`, `base_merged_from`, `merged`, `cleanup_warning`,
`review_reason`, `context_ceiling_tokens`, `model: Some(s.model.clone())` when non-empty. Set
`started_at`/`created_at` to `Utc::now() - elapsed_secs` so the tree's elapsed column keeps
ticking as before. Everything else stays `Default`/empty as it is today. Update the inline test
(`test_stage_info_to_stage_conversion`) to the new name and a `StageSummary` fixture.

The function has a ledger entry under its OLD name (`stage_info_to_stage 76`); the orchestrator
reconciles the ledger — report the new function's line count.

## Done means

`cargo test --manifest-path loom/Cargo.toml --lib commands::status::ui::` passes and the crate
builds. The live tree renders the same rows for the same stage set as before this stage: the
`state.rs` tests and the renderer test are the pin.

## Constraints the graph will not show you

- `app.rs` (410) and `renderer.rs` (414) are at their exact ledger line counts; neither may grow.
- Do not touch `tree_widget.rs`; the next stage deletes it.
- Do not run `git`. Do not run the full test suite, the linter or the formatter.
