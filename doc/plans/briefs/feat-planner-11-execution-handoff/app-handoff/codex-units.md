# Codex units of stage `app-handoff`

These are IMPLEMENTER units (`loom-codex-forwarder` runs that write this stage's code). They are
not the product's Codex `$pressure` step, which A1's driver runs as `codex exec`.

Each unit: the model flags in its block below (always `--effort high`), FOREGROUND, Bash timeout 600000 ms, only its
files, no git, no tests (compile checks at most: `cargo check --lib` from `loom/`), no dot-loom or
dot-work path. The orchestrator runs the proof command after each wave and `git status --short`.
Stay 10% under the size limits (360 lines per file, 45 per function). Double literal braces in
`format!`. Web units do not run `oxfmt`; the orchestrator formats; no TypeScript parameter
properties. `loom map` answers from the BASE tree, so units name what earlier units added.

## Wave 1 (foundation: everything after imports these)

Before this wave the orchestrator's SEED created every file the two module roots declare as a
placeholder (one `//!` line; `routes.rs` and `install.rs` with empty `all()` and `run()`), so the
crate compiles after every unit. A unit that owns a placeholder replaces it whole.

The spec for CF1-CF5 is `doc/plans/briefs/feat-planner-11-execution-handoff/app-handoff/f-wire-and-routes.md`
(types, fixtures, parity pattern, schemas), copied field for field.

- **CF1** (`--model gpt-6.1-terra --effort high`) owns `loom/src/plan_server/wire/pressure.rs` and `loom/src/plan_server/wire/tests_pressure.rs`:
  the `wire/pressure.rs` types and the `disabled_reason` consts; the parity tests for the two
  pressure fixtures (`<name>_fixture_matches_serde_output`, `<name>_fixture_deserializes`,
  `assert_all_in_schema` over every enum's wire values against `web/src/plan/api/schema-pressure.ts`,
  which CF4 writes in this wave). Proof: `cargo test --lib plan_server::wire::tests_pressure`.
- **CF2** (`--model gpt-6.1-terra --effort high`) owns `loom/src/plan_server/wire/run.rs` and `loom/src/plan_server/wire/tests_run.rs`: the
  same for `wire/run.rs` (fixtures and schema by CF5). Proof: `cargo test --lib plan_server::wire::tests_run`.
- **CF3** (`--model gpt-6.1-terra --effort high`) owns `loom/src/plan_server/wire/message.rs` and `loom/src/plan_server/wire/mod.rs`: the
  four `PlanMessage` variants `PressureRun(PressureRunView)`, `RunState(RunStateView)`,
  `RunOutput(RunOutput)`, `RunStrip(RunStrip)` (wire `type` values `pressure_run`, `run_state`,
  `run_output`, `run_strip`), and in `wire/mod.rs` the lines `pub mod pressure; pub mod run;`
  plus `#[cfg(test)] mod tests_pressure; #[cfg(test)] mod tests_run;` in plan 2's style. Proof:
  `cargo check --lib` after CF1 and CF2.
- **CF4** (`--model gpt-6.1-terra --effort high`) owns `web/src/plan/api/schema-pressure.ts`, `web/src/plan/api/schema-pressure.test.ts`,
  `web/src/plan/api/fixtures/pressure-run.json`, `web/src/plan/api/fixtures/pressure-options.json`:
  zod schemas (`z` from `@/api/zod`), the two fixtures as f-wire describes, and the vitest parse of
  each fixture. Proof: from `web/`, `node_modules/.bin/vitest run src/plan/api/schema-pressure.test.ts`.
- **CF5** (`--model gpt-6.1-terra --effort high`) owns `web/src/plan/api/schema-run.ts`, `web/src/plan/api/schema-run.test.ts`, and the
  fixtures `run-state.json`, `run-strip.json`, `run-output.json` under `web/src/plan/api/fixtures/`.
  Proof: `node_modules/.bin/vitest run src/plan/api/schema-run.test.ts`.
- **CF6** (`--model gpt-6.1-terra --effort high`) owns the module wiring, and is the ONLY writer of these four files in the stage:
  `loom/src/plan_server/mod.rs` (two lines `pub mod pressure;`, `pub mod run;` beside plan 2's
  list), `loom/src/plan_server/routes.rs` (`pressure::routes()` and `run::routes()` in
  `all_routes()`, `pressure::install(ctx)` and `run::install(ctx)` in `install_all()`), and the two
  final module roots, which only declare and delegate:
  `loom/src/plan_server/pressure/mod.rs` = `pub mod routes; pub mod install; pub mod driver; pub mod codex; pub mod recover; pub mod view; pub mod teammates; pub mod options; pub mod config_guard; #[cfg(test)] mod tests;`
  plus `pub fn routes() -> Vec<RouteSpec> { routes::all() }` and `pub fn install(ctx: &Arc<PlanContext>) { install::run(ctx) }`;
  `loom/src/plan_server/run/mod.rs` = `pub mod routes; pub mod install; pub mod state; pub mod commit; pub mod start; pub mod child; pub mod dashboard; pub mod dashboard_parse; pub mod strip; pub mod changes; pub mod abandon; #[cfg(test)] mod tests;`
  plus the same two delegating functions. A1 writes `pressure/routes.rs` (`pub fn all() -> Vec<RouteSpec>`)
  and `pressure/install.rs` (`pub fn run(ctx: &Arc<PlanContext>)`); A2 the same in `run/`; A3's
  abandon routes are appended by A2's `run/routes.rs::all`. Every declared file already exists as a
  placeholder from the orchestrator's SEED (stage description), so the crate compiles now. Proof:
  `cargo check --lib` right after the wave.

## Wave 2 (foreground, alongside A2 in the foreground, while A1 and A3 run in the background)

Each wave 2 unit's main file exists as a SEED placeholder; replace it whole. Its `tests.rs`
submodule file is new. The orchestrator runs the unit's proof right after the wave.

- **CA1** (`--model gpt-6.1-sol --effort high`) owns `loom/src/plan_server/pressure/options.rs` (ending with `#[cfg(test)] mod tests;`)
  and `loom/src/plan_server/pressure/options/tests.rs`: `pub fn options(ctx: &PlanContext, browser:
  &BrowserId) -> PressureOptions` and `pub fn validate_start(project: &Project, start:
  &PressureStart) -> Result<(), String>` per the GET options and POST start rows of
  `a1-pressure-driver.md` "Routes" (reason order; codes `plan_not_written`, `invalid_model`,
  `invalid_rounds`, `config_blocked` from plan 8's `StartBlocks::current`); tests per reason and
  code. A1 calls both from `routes.rs`. Proof: `cargo test --lib plan_server::pressure::options`.
- **CA2** (`--model gpt-6.1-sol --effort high`) owns `loom/src/plan_server/pressure/config_guard.rs` (ending with `#[cfg(test)] mod tests;`)
  and `loom/src/plan_server/pressure/config_guard/tests.rs`: `pub struct ConfigSnapshot`,
  `pub fn snapshot(root: &Path) -> anyhow::Result<ConfigSnapshot>`, `pub fn changed(before:
  &ConfigSnapshot, after: &ConfigSnapshot) -> Vec<String>` per a1 step 3 (sha256 of every file
  under `.claude/`, plus `.mcp.json` and `CLAUDE.md` at the root, tracked or untracked, read
  bounded and no-follow through `crate::fs::safe_read`; sorted repo-relative paths). Proof: `cargo
  test --lib plan_server::pressure::config_guard` and `cargo test --test plan_handoff_contracts
  codex_config_edit_is_detected`.
- **CB1** (`--model gpt-6.1-sol --effort high`) owns `loom/src/plan_server/run/dashboard_parse.rs` (with `#[cfg(test)] mod tests;`) and
  `loom/src/plan_server/run/dashboard_parse/tests.rs`: `pub fn parse_dashboard_startup(lines:
  &[&str]) -> Option<String>` and `pub fn strip_token(url: &str) -> String`, per a2 "Dashboard
  child" (the dashboard's pinned `print_startup` lines: loopback, terminals, remote bootstrap).
  A2's `dashboard.rs` re-exports `parse_dashboard_startup`. Proof: `cargo test --lib
  plan_server::run::dashboard_parse` and `cargo test --test plan_handoff_contracts
  dashboard_url_parsed_from_print_startup`.
- **CB2** (`--model gpt-6.1-sol --effort high`) owns `loom/src/plan_server/run/strip.rs` (with `#[cfg(test)] mod tests;`) and
  `loom/src/plan_server/run/strip/tests.rs`: `pub fn build_strip(work_dir: &WorkDir, dashboard_url:
  Option<&str>) -> anyhow::Result<RunStrip>` (tokenless via CB1's `strip_token`, pinned above) and
  the 5-second thread `pub(super) fn spawn_strip(ctx: Arc<PlanContext>)` per a2 "Run strip". Proof:
  `cargo test --lib plan_server::run::strip` and `cargo test --test plan_handoff_contracts
  run_strip_carries_no_token`.
- **CB3** (`--model gpt-6.1-sol --effort high`) owns `loom/src/plan_server/run/changes.rs` (with `#[cfg(test)] mod tests;`) and
  `loom/src/plan_server/run/changes/tests.rs`: `pub fn split_changes(root: &Path) ->
  anyhow::Result<(Vec<ChangedPath>, Vec<ChangedPath>)>` per a2 "doc_changes / blocking_changes"
  (`-z` porcelain, rename records, the summaries temp exclusion, no line cap). Security-relevant
  (it decides what the commit lane may commit): the frozen contract
  `changes_split_blocks_only_tracked_outside_doc` pins it, and the commit lane's
  `commit_refuses_paths_outside_current_doc_changes` and `commit_commits_only_ticked_paths` consume
  it; integration-verify's security review reads this file. Proof: `cargo test --lib
  plan_server::run::changes` and `cargo test --test plan_handoff_contracts
  changes_split_blocks_only_tracked_outside_doc`.
