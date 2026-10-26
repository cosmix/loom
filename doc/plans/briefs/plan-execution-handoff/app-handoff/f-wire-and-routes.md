# F: wire types, fixtures, schemas and route registration for pressure runs and run handoff

LANE: this brief is now the SPECIFICATION for codex units CF1-CF6 in `doc/plans/briefs/plan-execution-handoff/app-handoff/codex-units.md`. No Claude worker is spawned from it.

Stage `app-handoff` of `doc/plans/PLAN-plan-execution-handoff.md`. Tier: sonnet
(`loom-software-engineer`). FOUNDATION worker: A1, A2 and A3 start after you return and import
your types. Never run git, never spawn subagents. One narrow check at the end:
`cargo test --lib plan_server::wire::` from `loom/`. Frozen contract files
`tests/plan_handoff_contracts.rs` and `tests/plan_handoff_contracts/**` are READ-ONLY.

## Files you own (relative to `loom/`)

- `src/plan_server/wire/pressure.rs`, `src/plan_server/wire/run.rs` (new)
- `src/plan_server/wire/mod.rs` (two `pub mod` lines), `src/plan_server/wire/message.rs`
  (four `PlanMessage` variants)
- `src/plan_server/wire/tests_pressure.rs`, `src/plan_server/wire/tests_run.rs` (new, declared
  from `wire/mod.rs` the way plan 2's `tests.rs` is)
- `src/plan_server/routes.rs` (two lines in `all_routes()`, two in `install_all()`)
- `src/plan_server/mod.rs` (two lines: `pub mod pressure;`, `pub mod run;`, beside plan 2's module list)
- `src/plan_server/pressure/mod.rs`, `src/plan_server/run/mod.rs`: module roots holding only
  `pub fn routes() -> Vec<RouteSpec>` returning an empty Vec and `pub fn install(ctx: &Arc<PlanContext>) {}`
  plus `mod` lines for the files A1, A2, A3 will add (A1/A2/A3 then own these two files' bodies;
  you write them first so the crate compiles)
- `../web/src/plan/api/fixtures/pressure-run.json`, `pressure-options.json`, `run-state.json`,
  `run-strip.json`, `run-output.json`
- `../web/src/plan/api/schema-pressure.ts`, `../web/src/plan/api/schema-run.ts`,
  `../web/src/plan/api/schema-pressure.test.ts`, `../web/src/plan/api/schema-run.test.ts`

## Read first

Plan 2's wire conventions (merged): `loom map --outline src/plan_server/wire/message.rs`,
`src/plan_server/wire/tests.rs` (the `<name>_fixture_matches_serde_output`,
`<name>_fixture_deserializes` and `assert_all_in_schema` pattern), `web/src/plan/api/schema.ts`
and `schema.test.ts`; `src/plan_server/routes.rs` (`all_routes`, `install_all`, `RouteSpec`).
All types `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`,
`#[serde(rename_all = "snake_case")]`, enums `#[serde(rename_all = "snake_case")]`. zod imports
`z` from `@/api/zod` only.

## `wire/pressure.rs`

```rust
pub struct PressureOptions {
    pub available: bool,
    pub disabled_reason: Option<String>,       // exact texts below
    pub plans: Vec<String>,                    // written plans, section 01 order, repo-relative
    pub defaults: PressureSlots,
    pub claude_models: Vec<String>, pub claude_efforts: Vec<String>,
    pub codex_models: Vec<String>, pub codex_efforts: Vec<String>,
    pub running: bool,
}
pub struct PressureSlots { pub rounds: u32, pub claude_model: String, pub claude_effort: String,
    pub codex_model: String, pub codex_effort: String, pub address_model: String, pub address_effort: String }
pub struct PressureStart { pub plan: String, #[serde(flatten)] pub slots: PressureSlots }   // POST body
pub enum PressureStepKind { ClaudePressure, Codex, ClaudeAddress }
pub enum StepState { Running, Done, Failed, Stopped }
pub enum CodexState { Running, ReviewWritten, NoReview, Failed }
pub struct CodexStatus { pub model: String, pub state: CodexState, pub report_path: Option<String>, pub log_tail: Option<String> }
pub struct TokenTotals { pub input: u64, pub cache_write: u64, pub cache_read: u64, pub output: u64 }
pub struct PressureStepView { pub round: u32, pub kind: PressureStepKind, pub session_id: Option<String>,
    pub model: String, pub effort: String, pub state: StepState, pub started_ms: u64, pub ended_ms: Option<u64>,
    pub tokens: Option<TokenTotals>, pub codex: Option<CodexStatus>, pub edits: Vec<String> }
pub enum RunEndKind { Recorded, Stopped, Failed, Interrupted }
pub struct PressureEnd { pub kind: RunEndKind, pub by: Option<String>, pub step: Option<String>, pub reason: Option<String>, pub line: Option<String> }
pub struct PressureRunView { pub plan: String, pub rounds: u32, pub round: u32,
    pub step: PressureStepKind, pub started_ms: u64, pub tokens_total: u64,
    pub steps: Vec<PressureStepView>, pub ended: Option<PressureEnd> }
```

`disabled_reason` texts (A1 produces them; pin them here as `pub const`s):
`CODEX_MISSING = "The codex CLI is not installed."`,
`CODEX_SKILL_MISSING = "Codex's pressure skill is not installed under $CODEX_HOME/skills/pressure/."`,
`READ_ONLY = "The server is in read-only mode."`,
`NOT_HOLDER = "Another browser holds authoring."`,
`RUNNING = "A pressure test is already running."`.

## `wire/run.rs`

```rust
pub enum WorkspaceState { Absent, Initialized, Running, Stopped, Done }
pub enum RunOffer { Start, StartInitialized, NoneRunning, Resume, StartNext }
pub struct PlanChoice { pub path: String, pub slug: String, pub listed: bool }
pub struct ChangedPath { pub path: String, pub status: String /* porcelain XY, e.g. " M", "??" */ }
pub struct UnmergedBranch { pub branch: String, pub ahead: u32 }
pub struct AbandonView { pub plan: String, pub merged_stages: Vec<String>, pub unmerged_stages: Vec<String>,
    pub unmerged_branches: Vec<UnmergedBranch>, pub base_branch: String }
pub enum Backend { Tmux, Native }
pub enum RunJob { Commit, Start, Abandon, FinishAbandon, Dashboard }
pub struct RunStateView { pub workspace: WorkspaceState, pub active_plan: Option<String>, pub offer: RunOffer,
    pub plans: Vec<PlanChoice>, pub preselected: Option<String>, pub branch: String,
    pub doc_changes: Vec<ChangedPath>, pub blocking_changes: Vec<ChangedPath>,
    pub backends: Vec<Backend>, pub default_backend: Backend, pub clean_notice: bool,
    pub abandon: Option<AbandonView>, pub finish_abandon: Option<String> /* stranded IN_PROGRESS plan */,
    pub dashboard_url: Option<String> /* never carries the token */, pub busy: Option<RunJob> }
pub struct CommitRequest { pub paths: Vec<String>, pub message: String }
pub struct RunStartRequest { pub plan: String, pub backend: Backend, pub clean: bool }
pub struct AbandonRequest { pub confirmed_unmerged: Vec<UnmergedBranch> } // the list the dialog showed
pub enum OutputStream { Stdout, Stderr }
pub struct RunOutput { pub job: RunJob, pub step: String /* e.g. "loom init" */, pub stream: Option<OutputStream>,
    pub line: Option<String>, pub exit_code: Option<i32>, pub done: bool }
pub struct RunStrip { pub plan: Option<String>, pub daemon_running: bool, pub merged: u32, pub total: u32,
    pub attention: u32, pub dashboard_url: Option<String> /* tokenless; broadcast to every browser */ }
```

`PlanMessage` gains `PressureRun(PressureRunView)`, `RunState(RunStateView)`,
`RunOutput(RunOutput)`, `RunStrip(RunStrip)` (wire `type` values `pressure_run`, `run_state`,
`run_output`, `run_strip`). Nothing is added to `ClientMessage`: every command is a POST.

## Fixtures, parity, schemas

One fixture per top-level type: `pressure-options.json`, `pressure-run.json` (a run in round 2
of 2 at the `codex` step with one finished `claude_pressure` step carrying tokens and edits, a
running codex step, `ended: null`), `run-state.json` (workspace `stopped`, offer `resume`,
abandon present with one unmerged branch), `run-strip.json`, `run-output.json` (a stdout line).
Parity tests in `tests_pressure.rs`/`tests_run.rs` per plan 2's pattern, including
`assert_all_in_schema` for every enum's wire values against the `.ts` schema text. zod schemas
`schema-pressure.ts`/`schema-run.ts` exporting `pressureRunSchema`, `pressureOptionsSchema`,
`runStateSchema`, `runStripSchema`, `runOutputSchema` and the inferred types; vitest files parse
each fixture.

## Done when

`cargo test --lib plan_server::wire::` passes and `cd ../web && node_modules/.bin/vitest run
src/plan/api/schema-pressure.test.ts src/plan/api/schema-run.test.ts` passes (run only the cargo
one if you get one check; the orchestrator's gate runs the other).
