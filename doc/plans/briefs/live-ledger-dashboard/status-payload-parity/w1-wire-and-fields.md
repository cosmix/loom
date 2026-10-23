# W1 — Live payload becomes `StatusData`; four new `StageSummary` fields

Tier: codex `gpt-5.6-terra`, effort `xhigh`. This is the FOUNDATION worker: it runs alone, and
W2/W3 spawn only after it returns, because every one of them compiles against the types you change.

## Files you own (write)

- `loom/src/daemon/protocol.rs`
- `loom/src/daemon/mod.rs`
- `loom/src/daemon/server/status.rs`
- `loom/src/daemon/server/tests.rs`
- `loom/src/daemon/wire_tests.rs`
- `loom/src/commands/status/data/mod.rs`
- `loom/src/commands/status/data/collector.rs`
- `loom/src/commands/status/data/collector_tests.rs`
- `loom/src/commands/status/render/attention_tests.rs` — ONLY the `make_stage_summary` fixture
- `loom/src/commands/status/render/graph_tests.rs` — ONLY the `make_stage_summary` fixture

Read-only: `loom/src/models/session/types.rs`, `loom/src/orchestrator/monitor/heartbeat.rs`,
`loom/src/fs/work_dir.rs`, `loom/src/daemon/server/broadcast.rs`.

## Entry points

- `Response::StatusUpdate` and `StageInfo` in `loom/src/daemon/protocol.rs` (start from `pub enum Response`).
- `collect_status` in `loom/src/daemon/server/status.rs` — the daemon's per-second status builder,
  called from `run_status_broadcaster` in `daemon/server/broadcast.rs`.
- `StatusData`, `StageSummary` in `loom/src/commands/status/data/mod.rs`.
- `build_stage_summary`, `heartbeat_facts`, `read_heartbeat_for_stage`, `assigned_session` in
  `loom/src/commands/status/data/collector.rs`.

## Why

`loom status --live` subscribes to the daemon's push, which today carries `StageInfo` — a struct
with no context tokens, no heartbeat activity, no retry count and no review reason. The static
`loom status` renders the richer `StageSummary` built by `collect_status_data`. After this worker
there is ONE status payload: the daemon pushes `StatusData`.

## What to build

### 1. `protocol.rs`

- `StatusUpdate { data: crate::commands::status::data::StatusData }` replaces the four
  `Vec<StageInfo>` fields.
- Delete `pub struct StageInfo` and any import only it used (check `WorktreeStatus`, `DateTime`).
- `CompletionSummary`, `StageCompletionInfo`, `DaemonConfig` stay untouched.

### 2. `daemon/mod.rs`

Remove `StageInfo` from the `pub use protocol::{...}` list. Nothing else changes.

### 3. `daemon/server/status.rs`

`collect_status(work_dir: &Path) -> Result<Response>` becomes:

```text
let wd = crate::fs::work_dir::WorkDir::new(work_dir).context("...")?;
let data = crate::commands::status::data::collect_status_data(&wd)?;
Ok(Response::StatusUpdate { data })
```

Delete `get_session_pid`, `detect_worktree_status`, `detect_worktree_status_with_target` and the
imports only they used (`branch_name_for_stage`, `WorktreeStatus`, `Stage`, `StatusBucket`,
`resolve_target_branch_from_config`, `parse_from_markdown` if nothing else in the file uses them).
Their only consumers are this file and `daemon/server/tests.rs` (verified 2026-09-03 with
`rg -n detect_worktree_status loom/src`), so delete the tests that exercise them too. Keep
`collect_completion_summary` and everything it needs.

### 4. `data/mod.rs` — four new `StageSummary` fields, each `#[serde(default)]`

```text
/// Distinct execution-model display names observed for this stage's subagents,
/// first-seen order (spawn ledger + codex ledger). Empty until a subagent spawns.
pub execution_models: Vec<String>,
/// Number of acceptance-criteria disputes raised on this stage.
pub dispute_count: u32,
/// Seconds since the judge session's heartbeat, when an adjudication session exists.
pub judge_heartbeat_secs: Option<u64>,
/// Terminal backend hosting the assigned session.
pub session_backend: Option<SessionBackendKind>,
```

Add `mod execution_models;` and `pub use execution_models::execution_models_for_stage;`. W4 writes
that module in parallel with you, with exactly this signature:
`pub fn execution_models_for_stage(work_dir: &WorkDir, stage_id: &str) -> Vec<String>`.
The crate compiles only once both files exist; do not stub the module.

`StatusData` is now a wire type. `#[serde(default)]` on every new field is what lets a client
parse a payload from a daemon built before this change; keep it.

### 5. `collector.rs`

- Fill the four fields in `build_stage_summary`. That function is 56 lines and carries an exact
  ledger entry (`function src/commands/status/data/collector.rs build_stage_summary 56`); it must
  NOT grow. Put the new assignments behind one helper, e.g.
  `fn stage_extras(stage: &Stage, session: Option<&Session>, work_dir: &WorkDir) -> StageExtras`,
  and destructure it in the struct literal.
- `dispute_count: stage.dispute_count`.
- `session_backend: assigned_session(stage, sessions).map(|s| s.backend)` — check whether
  `SessionBackendKind` is `Copy`; clone if not.
- `judge_heartbeat_secs`: read `heartbeat/<stage_id>.adjudication.json` the same way
  `read_heartbeat_for_stage` reads `heartbeat/<stage_id>.json` — factor the path-taking part into
  `fn read_heartbeat_file(path: &Path) -> Option<Heartbeat>` so both callers share it — and report
  `now - heartbeat.timestamp` in seconds, `None` when the file is absent or unparsable.
- `execution_models: execution_models_for_stage(work_dir, &stage.id)`.

### 6. Fixtures

Every `StageSummary` struct literal in the crate must gain the four fields:
`collector_tests.rs`, `attention_tests.rs::make_stage_summary`, `graph_tests.rs::make_stage_summary`.
Use `Vec::new()`, `0`, `None`, `None`.

### 7. Tests

- `daemon/wire_tests.rs::status_update_round_trip`: build a `StatusData` with one `StageSummary`
  carrying `context_tokens: Some(487_000)`, `execution_models: vec!["sonnet".into()]`, round-trip
  through `write_message`/`read_message`, assert both survive.
- `daemon/server/tests.rs`: the two tests that destructure `StatusUpdate` now assert on
  `data.progress` (counts via `StageStatus::bucket()`, same grouping as before) and `data.stages`.
- `collector_tests.rs`, two new tests: `judge_heartbeat_secs_reads_adjudication_file` (write a
  heartbeat JSON at `heartbeat/<id>.adjudication.json` inside a temp `WorkDir`, timestamp a few
  seconds in the past, assert `Some(_)`; assert `None` when the file is absent) and
  `session_backend_comes_from_assigned_session`.

## Done means

`cargo build --all-targets --manifest-path loom/Cargo.toml` compiles (once W4's module exists) and
`cargo test --manifest-path loom/Cargo.toml --lib daemon::` plus
`cargo test --manifest-path loom/Cargo.toml --lib commands::status::data::` pass. Report the exact
new line counts of `daemon/server/status.rs` and any function you changed — the orchestrator
reconciles `loom/maintainability-baseline.txt`; you do not touch it.

## Constraints the graph will not show you

- `collect_status_data` runs git ancestry checks per completed stage; the old builder ran git per
  worktree. The broadcaster already skips all work when nobody is subscribed
  (`broadcast.rs`, the `has_subscribers` short-circuit) — keep that call shape, do not add caching.
- `daemon/server/status.rs` is at its 416-line ledger cap. Shrinking is fine; growing is not.
- Never read `.work/admin.token` or `.work/user.token`; never sweep `.work/` with a grep.
- Do not run `git` at all. Do not run the full test suite, the linter or the formatter.
