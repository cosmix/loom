# P1: move the pressure pipeline into `loom::pressure`, keep the terminal driver

Stage `pressure-pipeline` of `doc/plans/PLAN-plan-execution-handoff.md`. Tier: sonnet
(`loom-software-engineer`). Never run git, never spawn subagents. One narrow check at the end:
`cargo test --lib pressure::` from `loom/`. The frozen contract file
`tests/pressure_pipeline_contracts.rs` (and `tests/pressure_pipeline_contracts/**`) is
READ-ONLY.

## Files you own (relative to `loom/`)

- `src/lib.rs` (one line: `pub mod pressure;`)
- `src/pressure/mod.rs`, `src/pressure/steps.rs`, `src/pressure/models.rs`,
  `src/pressure/paths.rs`, `src/pressure/codex_run.rs`, `src/pressure/instruction.rs`,
  `src/pressure/tests.rs`
- `src/commands/pressure/mod.rs`, `src/commands/pressure/spawn.rs`,
  `src/commands/pressure/terminal.rs` (new), `src/commands/pressure/tests.rs`,
  `src/commands/pressure/fallback.rs`, `src/commands/pressure/models.rs`,
  `src/commands/pressure/paths.rs` (the last three are deleted or shrunk as below)
- `src/commands/init/mod.rs` (one line: `pub use plan_setup::preflight_plan;`)

P2 owns `src/pressure/record.rs` (and its tests and fixtures); you declare `pub mod record;` in
`src/pressure/mod.rs` and call its API exactly as pinned below. P3 owns the backend-default
files; do not touch them.

## Read first

`src/commands/pressure/mod.rs` (363 lines: `Step` :57-70, `plan_steps` :78-90,
`render_dry_run` :96-165, `StepContext` :170-178, `run_pressure_step` :210-244,
`report_codex_review` :250-264, `run_pipeline` :269-299, `project_pressure_config` :305-310,
`execute` :313-360), `spawn.rs` (169), `fallback.rs` (295), `models.rs` (192), `paths.rs`
(129), `tests.rs` (340), and `src/claude/session.rs:108-150` (`run_foreground`).

## What moves into `src/pressure/` (crate-level, `pub`)

Behaviour identical unless stated. Every file under 400 lines, every function under 50.

- `steps.rs`: `pub enum Step { DeleteReport(PathBuf), Pressure { claude: String, codex: String }, Address(String) }`
  and `pub fn plan_steps(rounds: u32, invocation: &str, report: &Path) -> Vec<Step>` (moved from
  `mod.rs:57-90`, unchanged), plus `pub const RECORD_STEP_LABEL: &str = "write the ## Pressure tests record";`.
- `models.rs`: `PressureModels` and `resolve` (moved from `commands/pressure/models.rs`, made
  `pub`; its tests move with it). `resolve` keeps taking `PressureModelFlags`
  (`crate::cli::types_pressure`); add `pub fn resolve_for(root: &Path, flags: PressureModelFlags) -> PressureModels`
  that reads the project tier the way `project_pressure_config` (`mod.rs:305-310`) does and
  `UserConfig::load()`; the app driver calls it.
- `paths.rs`: `ResolvedPlan`, `resolve_plan_path`, `starts_with_doc_plans`, `codex_report_path`
  (moved, `pub`). `resolve_repo_root`, `codex_log_path` and `claude_marker_path` stay in
  `commands/pressure/paths.rs` (terminal-only).
- `codex_run.rs`: the Codex runner from `fallback.rs` (`CodexOutcome`, `CodexRun`, `CodexJob`,
  `spawn_codex_with_fallback`, `run_attempts`, `fallback_note`, `unavailable_line`,
  `codex_model_label`, `spawn_codex_background`, `codex_args` from `spawn.rs:57-121`) with two
  additions the app driver needs and NO printing:
  - `CodexRun` keeps an `Arc<AtomicU32>` holding the pid of the attempt now running (set on
    each spawn) and exposes `pub fn current_pid(&self) -> u32`, `pub fn is_finished(&self) -> bool`,
    `pub fn join(self) -> anyhow::Result<CodexOutcome>`.
  - `CodexOutcome` fields become `pub`; `print_fallbacks` moves to the terminal driver as a free
    function over `&CodexOutcome`.
  - `pub fn codex_skill_installed() -> bool`: `$CODEX_HOME/skills/pressure/SKILL.md` is a file,
    `CODEX_HOME` defaulting to `~/.codex` (via `dirs::home_dir()` or the crate's existing home
    helper; check `rg -n 'fn home_dir' src` first and reuse it). Nothing in the tree checks this
    today (`find_codex_path`, `src/codex.rs:240`, checks only the CLI).
  The spinner in `wait_codex` (`fallback.rs:189-208`) is terminal UI: it moves to
  `commands/pressure/terminal.rs` and waits through `is_finished`/`join`.
- `instruction.rs`: `pub enum MarkerHome { Workspace, PlanSession }` and
  `pub fn completion_instruction(marker: &Path, home: MarkerHome) -> String`. `Workspace` keeps
  today's text (`spawn.rs:21-31`) byte for byte. `PlanSession` replaces the sentence "That path is
  inside the repo's gitignored `.loom/work/` because ... directly." with: "That path is this
  session's completion-marker directory, the one place outside the plan this session may write;
  creating this one marker is the only write it is for." The rest is identical. No other text
  mentions `.loom/work`.
- `mod.rs`: re-exports, `pub mod record;`, and the pipeline:

```rust
pub enum StepVerdict { Continue, Stop { step: String /* "round 2 /address" */, reason: String } }
pub struct PressureResult { pub verdict: StepVerdict, pub codex_model_used: Option<String> }
pub trait PressureDriver {
    /// Delete `report` if present (each driver may log it).
    fn delete_report(&mut self, report: &Path) -> anyhow::Result<()>;
    /// Run Claude `/pressure` and Codex `$pressure` concurrently; return when both ended.
    fn run_pressure(&mut self, round: u32, claude: &str, codex: &str) -> anyhow::Result<PressureResult>;
    /// Run Claude `/address`.
    fn run_address(&mut self, round: u32, slash: &str) -> anyhow::Result<StepVerdict>;
}
pub struct PipelineInput<'a> {
    pub rounds: u32, pub invocation: &'a str, pub report: &'a Path,
    pub plan_path: &'a Path /* absolute fs path the run started with */,
    pub claude_model: &'a str /* the /pressure slot model */,
    pub today: chrono::NaiveDate,
}
pub enum PipelineOutcome {
    Recorded { line: String },
    RecordSkipped { line: String, why: String /* plan moved */ },
    Stopped { step: String, reason: String },
}
pub fn run_pipeline(driver: &mut dyn PressureDriver, input: &PipelineInput<'_>) -> anyhow::Result<PipelineOutcome>
```

`run_pipeline` walks `plan_steps(rounds, invocation, report)`, numbering rounds from 1 at each
`Step::Pressure`. After each `run_pressure` it notes the Codex model used (the result's
`codex_model_used`, else the empty string) for that round; immediately before each
`run_address` it records `report.is_file()` for that round. On the first `Stop` it deletes the
report through `delete_report` and returns `Stopped` (no record). At the final `DeleteReport` it builds
`record::RecordLine { date: input.today, claude_model, codex_models, rounds, missing_rounds }`
(`missing_rounds` = rounds whose report was absent when `/address` ran), calls
`record::append_record(input.plan_path, &line)` and maps `RecordWrite::Written` to `Recorded`,
`RecordWrite::PlanMoved` to `RecordSkipped`. Order at the end follows section 11 ("after the
last round, write the record, then delete the report"): the record is written when the walk
reaches the final `DeleteReport` step, and that step runs after it. P2 pins `RecordLine` and `append_record`; read P2's brief
section "API" at `doc/plans/briefs/plan-execution-handoff/pressure-pipeline/p2-record-writer.md`.

## What stays in `src/commands/pressure/`

- `terminal.rs` (new): `pub(super) struct TerminalDriver<'a>` holding today's `StepContext`
  fields; `impl PressureDriver for TerminalDriver` built from `run_pressure_step`,
  `report_codex_review`, the `Address` arm of `run_pipeline` and the spinner; it keeps every
  line the terminal prints today, with the same text, plus `delete_report` calling
  `crate::claude::remove_if_exists`. `claude_should_stop`/`should_stop` map to
  `StepVerdict::Stop { step, reason }` where `reason` is the text `should_stop` prints.
- `spawn.rs`: `claude_args` (still `--permission-mode auto` and
  `completion_instruction(marker, MarkerHome::Workspace)`), `run_claude_foreground`,
  `print_log_tail`, `should_stop`, `claude_should_stop`.
- `mod.rs`: `execute` resolves as today, then `run_pipeline(&mut TerminalDriver { .. },
  &PipelineInput { .., today: chrono::Local::now().date_naive() })`, and prints by outcome:
  `Recorded` → `✓ Pressure test complete.` then `✓ recorded in <plan>: <line>`;
  `RecordSkipped` → `! <plan> is no longer at the path the run started with; not recorded: <line>`
  (yellow `!`); `Stopped` → nothing further (the driver already printed why).
  `render_dry_run` appends one numbered line after the steps: `  <n>. <RECORD_STEP_LABEL> in <plan invocation>`.
- `models.rs` and `fallback.rs` are deleted once empty; `paths.rs` keeps only the three
  terminal helpers.
- `tests.rs`: keep every existing test, re-pointed at the new paths; the moved items' tests
  move into `src/pressure/tests.rs`. Do not delete or weaken any assertion.

## Same step list for both drivers

Both drivers get their steps only from `loom::pressure::plan_steps`; add a unit test in
`src/pressure/tests.rs` that a recording fake `PressureDriver` sees, for 2 rounds, exactly:
delete, pressure(1), address(1), delete, pressure(2), address(2), delete. The app driver (stage
`app-handoff`) reuses that fake.

Also test (in `src/commands/pressure/tests.rs`): the terminal's outcome lines for `Recorded`
(`✓ recorded in <plan>: <line>`) and `RecordSkipped` (`! <plan> is no longer at the path the run
started with; not recorded: <line>`), through a small `fn outcome_lines(outcome, plan) ->
Vec<String>` the printer uses; and that `completion_instruction(.., MarkerHome::Workspace)` is
byte-identical to today's text (paste today's text into the test).

## Done when

`cargo test --lib pressure::` passes; `loom pressure --dry-run` output is unchanged except the
added record line. Report the final line counts of every file you touched.
