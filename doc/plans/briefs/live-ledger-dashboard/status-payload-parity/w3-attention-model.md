# W3 — One attention model for static and live status, covering all seven human-needing states

Tier: codex `gpt-5.6-terra`, effort `xhigh`. Spawned after W1 returns.

## Files you own (write)

- `loom/src/commands/status/render/attention_model.rs` — new
- `loom/src/commands/status/render/attention_model_tests.rs` — new
- `loom/src/commands/status/render/attention.rs`
- `loom/src/commands/status/render/graph.rs` — ONLY the failure-label match inside `format_stage_annotations`
- `loom/src/commands/status/render/mod.rs` — module declarations only

Read-only: `loom/src/commands/status/data/mod.rs`, `loom/src/models/failure.rs`,
`loom/src/models/stage/types.rs`, `loom/src/commands/status/render/attention_tests.rs` (do NOT edit;
it pins today's output and must stay green).

## Entry points

- `render_attention`, `render_problem_stage`, `render_human_review_choices`,
  `render_cleanup_warning` in `attention.rs`.
- The `FailureType` match inside `format_stage_annotations` in `graph.rs` (the `crash`/`test`/
  `build`/... labels).
- Look at how `attention.rs` declares its test module (`mod tests;` with a `#[path]`) and mirror it
  for `attention_model_tests.rs`.

## Why

Today only five statuses get an attention block: Blocked, MergeConflict, CompletedWithFailures,
MergeBlocked, NeedsHumanReview (plus any stage with a `cleanup_warning`). `NeedsAdjudication` and
`WaitingForInput` get nothing, and the live TUI has no attention panel at all. The next stage's
dashboard needs the same entries as the static view, so the model is extracted into pure data.

## What to build

### `attention_model.rs`

```text
pub struct AttentionEntry {
    pub stage_id: String,
    pub stage_name: String,
    pub status: StageStatus,
    pub title: &'static str,
    pub failure_type: Option<FailureType>,
    pub evidence: Vec<String>,        // stage.failure_info.evidence, untruncated
    pub reason: Option<String>,       // review_reason, or the adjudication/input sentence below
    pub hint: String,                 // the command or instruction that resolves it
    pub choices: &'static [(&'static str, &'static str)],   // human-review flags, else empty
    pub cleanup_warning: Option<String>,
}

pub fn attention_entries(stages: &[StageSummary]) -> Vec<AttentionEntry>
pub fn attention_title(status: &StageStatus) -> Option<&'static str>
pub fn failure_label(failure_type: &FailureType) -> &'static str
```

Titles (`attention_title`): Blocked `BLOCKED`, MergeConflict `MERGE CONFLICT`,
CompletedWithFailures `ACCEPTANCE FAILED`, MergeBlocked `MERGE ERROR`, NeedsHumanReview
`NEEDS REVIEW`, NeedsAdjudication `NEEDS ADJUDICATION`, WaitingForInput `NEEDS INPUT`; every
other status `None`. A stage with `cleanup_warning` set produces an entry titled `CLEANUP FAILED`
regardless of status (this mirrors `render_problem_stage`, which decides cleanup before status).

Hints, exactly the strings `attention.rs` prints today for the five existing statuses:
`loom stage retry <id>` (Blocked, CompletedWithFailures), `loom stage merge <id>` (MergeConflict,
MergeBlocked), `loom stage human-review <id>` (NeedsHumanReview), `loom worktree remove <id>`
(cleanup). New: NeedsAdjudication → `reason` is
`dispute {n} of 3 awaiting the judge session's verdict` with `n = dispute_count`, hint
`loom status --verbose`; WaitingForInput → `reason` is
`the agent is waiting for an answer in its session` and hint
`answer in the session's terminal (pid {pid}, {backend})` using `pid`/`session_backend`, with
`pid unknown` / `backend unknown` when absent. `choices` is the three-tuple table now in
`render_human_review_choices` for NeedsHumanReview, empty otherwise.

`failure_label` is the eleven-arm match currently inline in `graph.rs` (`SessionCrash` → `crash`,
`TestFailure` → `test`, ... `Unknown` → `error`); move it here verbatim and make `graph.rs` call it.

Entry order = input order. Statuses outside the seven, without a cleanup warning, produce nothing.

### `attention.rs`

`render_attention` builds `attention_entries(stages)` and renders from entries. Output for the five
existing statuses and for cleanup warnings must stay byte-identical — `attention_tests.rs` is the
pin and you may not edit it. The two new titles render through the same `render_problem_stage`
shape (`► name (TITLE)`, `ID:`, `Reason:`, `Hint:`). `render_problem_stage` currently carries an
exact ledger entry at 53 lines; the rewrite must bring it UNDER 50 so the entry can be deleted.

### `attention_model_tests.rs`

- `entries_cover_adjudication_and_input`: one stage per status, assert the seven titles, the
  two new reasons and hints (use `dispute_count: 2`, `pid: Some(4242)`, `session_backend:
  Some(SessionBackendKind::Native)`).
- `quiet_statuses_produce_no_entry`: Completed, Executing, Queued, WaitingForDeps, Skipped,
  NeedsHandoff produce nothing.
- `cleanup_warning_wins_over_status`.
- `failure_label_table` covers all eleven variants.

## Done means

`cargo test --manifest-path loom/Cargo.toml --lib commands::status::render::` passes, including the
untouched `attention_tests.rs`.

## Constraints the graph will not show you

- Never write the finalize verb next to `loom stage` in any shell command — the control-hook
  prefilter matches that triple. Hints above are strings in Rust source, not commands you run.
- Both new files stay under 400 lines, every function under 50.
- Do not run `git`. Do not run the full test suite, the linter or the formatter.
