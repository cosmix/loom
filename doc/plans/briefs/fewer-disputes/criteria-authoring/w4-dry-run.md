# criteria-authoring W4: `loom plan verify --dry-run` and the extended-criterion timeout

Tier: sonnet. Wave 1, with W0, W0b and W5 (you touch no type W0 writes). Read `common.md` first. Frozen
contracts: `tests/dry_run_contracts.rs` (never edit it).

## Files you own

`src/cli/types_ops.rs`, `src/cli/dispatch.rs`, `src/commands/plan/verify.rs`,
`src/commands/plan/verify/dry_run.rs` (new), `src/commands/plan/verify/dry_run_classify.rs` (new),
`src/commands/plan/verify/dry_run_tests.rs` (new), `src/commands/plan/verify/report.rs`,
`src/commands/plan/verify/inputs/tests.rs`, `src/verify/criteria/config.rs`, `src/verify/criteria/mod.rs`,
`src/verify/criteria/runner.rs`, `src/verify/criteria/zero_tests.rs`, `src/verify/budget.rs`,
`src/orchestrator/monitor/judge_watchdog.rs`, `src/orchestrator/monitor/judge_watchdog_timeout_tests.rs` (new),
`src/models/stage/checks.rs` (doc comment only).

## Part 1: the timeout (Decision 4)

1. `src/verify/criteria/config.rs`: add `pub const DEFAULT_EXTENDED_CRITERION_TIMEOUT: Duration =
   Duration::from_secs(300);` and `pub fn extended_criterion_timeout(check: &TruthCheck) -> Duration {
   check.timeout(DEFAULT_EXTENDED_CRITERION_TIMEOUT) }`. Fix the `DEFAULT_CHECK_TIMEOUT` doc comment: it is the
   default of `after_stage` and `before_stage` truth checks only (30 s). Re-export both new items from
   `src/verify/criteria/mod.rs` beside `DEFAULT_CHECK_TIMEOUT`.
2. Replace `check.timeout(DEFAULT_CHECK_TIMEOUT)` for an `AcceptanceCriterion::Extended` with
   `extended_criterion_timeout(check)` in exactly three places: `runner.rs` (`prepare_criterion`),
   `budget.rs` (`command_secs`: `check.timeout_secs.unwrap_or(DEFAULT_EXTENDED_CRITERION_TIMEOUT.as_secs())`)
   and `judge_watchdog.rs` (`check_run_limit`, plus its module doc comment that names the constant).
   `after_stage_secs` in `budget.rs`, `verify/goal_backward/truths.rs` and `wiring_tests.rs` keep 30 s; do
   not touch them. Drop the `DEFAULT_CHECK_TIMEOUT` import from `runner.rs` and `judge_watchdog.rs` when it
   becomes unused (the acceptance criterion `! rg -qF DEFAULT_CHECK_TIMEOUT src/verify/criteria/runner.rs
   src/orchestrator/monitor/judge_watchdog.rs` requires it to vanish, comments included).
3. Update the doc comment of `TruthCheck.timeout_secs` in `src/models/stage/checks.rs` if it states 30 s for
   acceptance.
4. Tests (in-crate): an extended criterion with no `timeout_secs` gets 300 s in `command_secs` and in
   `check_run_limit`; `timeout_secs: 900` still wins; a truth check keeps 30 s. Add them to
   `src/verify/budget.rs`'s own `#[cfg(test)] mod tests` (create one when absent) and to a new sibling
   `src/orchestrator/monitor/judge_watchdog_timeout_tests.rs` wired with `#[cfg(test)] #[path = ".."] mod`
   from `judge_watchdog.rs`; `check_run_limit` is private, so the sibling test module is declared inside
   `judge_watchdog.rs` and reaches it through `super::`.

## Part 2: the dry run (Decision 3)

### CLI

`PlanCommands::Verify` (`src/cli/types_ops.rs`) gains `#[arg(long)] dry_run: bool` ("Run every criterion
against a checkout of HEAD under the stage's confinement and report environment failures, timeouts and empty
test selections"). `dispatch_plan` passes it to `plan::verify::execute(&path, strict, json, no_color,
dry_run)`; update every caller of `execute` (`src/commands/plan/verify/inputs/tests.rs` calls it).
`dispatch_plan` and `dispatch` are listed in the ledger only for `dispatch` (57 lines): add the field to the
existing `Some(PlanCommands::Verify { .. })` arm without changing `dispatch`.

### Flow in `execute` (`src/commands/plan/verify.rs`)

After the report is built and printed-or-serialised inputs are ready: when `dry_run` and `!report.stopped` and
`report.errors.is_empty()`, call `dry_run::run_dry_run(path, &mut report)?` BEFORE the human or JSON output and
before `should_fail` is computed. With errors already present the dry run is skipped and a note says so
(`dry run skipped: the plan has validation errors`). `--strict` alone is unchanged. The flag runs locally, never
through the daemon.

### `dry_run.rs` (target under 250 lines) and `dry_run_classify.rs`

```rust
pub enum DryOutcome { Environment(String), Missing(String), TimedOut, ZeroTests, Passed, Failed }
pub enum DrySeverity { Error, Warning, Note }
pub struct DryCommand { pub stage_id: String, pub field: &'static str, pub index: usize,
    pub command: String, pub dir: PathBuf, pub timeout: Duration, pub tree_root: PathBuf,
    pub confinement: CommandConfinement }   // field: acceptance | after_stage | wiring_tests
pub fn classify(result: &CriterionResult, failures: &[String]) -> DryOutcome;
pub fn severity(outcome: &DryOutcome, stage_has_dependencies: bool) -> Option<DrySeverity>;
pub fn stage_cannot_tell(acceptance: &[DryOutcome], stage_has_dependencies: bool) -> bool;
pub fn settle_timeout(first: DryOutcome, rerun: &mut dyn FnMut() -> DryOutcome) -> DryOutcome;
pub fn run_dry_run_with(path: &Path, report: &mut VerifyReport,
    execute: &mut dyn FnMut(&DryCommand) -> (CriterionResult, Vec<String>)) -> anyhow::Result<()>;
pub fn run_dry_run(path: &Path, report: &mut VerifyReport) -> anyhow::Result<()>;
```

`classify`, `severity`, `stage_cannot_tell` and `settle_timeout` live in `dry_run_classify.rs` (the enums derive
`Debug, Clone, PartialEq, Eq`); all are `pub` and re-exported from `dry_run`. `commands::plan::verify` must
declare `pub mod dry_run;`.

`classify(result, failures)`, first match wins:

1. `result.timed_out` -> `TimedOut`.
2. a failure string containing `selected zero tests` -> `ZeroTests` (the runner's zero-test guard text; see
   `zero_tests.rs`).
3. a non-zero exit whose stderr or stdout, lower-cased, contains one of these markers: `bwrap:`, `operation not
   permitted`, `temporary failure in name resolution`, `could not resolve host`, `couldn't resolve host`,
   `network is unreachable`, `connection refused`, `read-only file system` -> `Environment(<the marker>)`.
   A marker only counts when the run produced no recognised test summary: a Rust test that exits 101 and
   prints `test result:` together with the words `operation not permitted` in its own assertion text is
   `Failed`. Detect a summary with `crate::testrun::registry::recognize(command, cwd)` and the adapter's
   `parse`, as `zero_tests.rs` does: add `pub(crate) fn has_test_summary(command: &str, cwd: &Path, result:
   &CriterionResult) -> bool` to `zero_tests.rs` (true when an adapter recognises the command and its parse
   reports an executed count) and re-export it from `verify/criteria/mod.rs` as `pub(crate)`. `classify` takes
   no command, so `run_dry_run_with` computes the flag and passes a second argument: the public signature is
   `classify(result: &CriterionResult, failures: &[String]) -> DryOutcome` and delegates to a private
   `classify_with(result, failures, has_summary: bool)`; `run_dry_run_with` calls `classify_with`. The public
   `classify` assumes no summary.
4. exit code 126 or 127 (a command that cannot start: a missing program or script) -> `Missing("exit 126" or
   "exit 127")`. It is a separate variant because it is an error only where nothing can provide the program:
   a stage with dependencies may get the program or script from an earlier stage, so `severity` makes it a note
   there. (A bubblewrap or network marker above is an environment fault whatever the dependencies.)
5. `result.success` and `failures.is_empty()` -> `Passed`.
6. anything else -> `Failed`.

`settle_timeout(first, rerun)`: when `first` is `TimedOut`, call `rerun` once and return what it returns (a second
timeout stays `TimedOut`); any other `first` is returned without calling `rerun`. The first run of a cold build
can time out on compilation alone; the rerun runs in the same checkout, whose build cache the first run warmed,
so only a command that is slow with a warm cache reads as a timeout.

`severity`: `Environment`, `TimedOut` -> `Some(Error)`; `ZeroTests` -> `Some(Note)` for every stage (a filter
naming tests the stage itself or an earlier stage adds selects nothing at base by design; `loom plan verify`'s
static Rust filter lint and completion's zero-test guard cover a filter that stays empty); `Missing` -> `Some(Error)` when
`!stage_has_dependencies`, else `Some(Note)`; `Passed` -> `Some(Note)` when `!stage_has_dependencies`, else
`None` (passing at base is expected for a regression gate such as a format check; it is not a finding by
itself); `Failed` -> `Some(Note)`.

`stage_cannot_tell(acceptance, stage_has_dependencies)`: true only when `!stage_has_dependencies`, the list is
non-empty and EVERY outcome is `Passed`. Such a stage cannot tell done from not done: every acceptance
criterion already passes at base. This is the only "already passes" warning; one passing criterion among
failing ones is never a finding beyond its note, so `--strict --dry-run` passes on a plan with sound criteria.
Only `acceptance` outcomes count (not `after_stage` or `wiring_tests`).

`run_dry_run_with(path, report, execute)` holds everything except executing a command, so the frozen contracts
exercise it without bubblewrap (they run inside the host job's confinement, where bubblewrap cannot nest):

1. Re-read and parse the plan text the way `report::verify_text` does (extract the metadata block, parse
   `LoomMetadata`); add a small `pub(super) fn parse_metadata(text: &str) -> Option<LoomMetadata>` to
   `report.rs` and use it from `verify_text` as well, so the parse is not duplicated.
2. Find the repository (`plan_repo_root`) and the committed HEAD (`run_git_checked(&["rev-parse", "HEAD"],
   root)`), the commit `loom run` branches the plan's first stages from; the working tree, staged changes and
   untracked files are never part of the checkout. No repository or no commit: push the note `dry run skipped:
   ...` and return `Ok`.
3. Build `Stage::from_definition(&definition, &PlanIdentity { id: <plan file stem>, version:
   metadata.loom.version, ratchet_files: &metadata.loom.ratchet_files })` per stage.
4. One `BaseCheckout::create(root, &head)` (`crate::verify::impact_tests::base`), shared by every stage; keep it
   alive until the last stage ran.
5. Provision, once, before any criterion: `crate::orchestrator::provision::run_provision(&metadata.loom.provision,
   checkout.root())` (the executor `loom run` uses; the entries are plan-verified hardened commands, so this runs
   the same commands, on the host, in `<checkout>/<working_dir>`, 600 s each). Reason: a stage's criteria run
   against dependencies the daemon would have installed (`node_modules/`, `.venv/`), and without them every
   JS or Python criterion reads as a missing tool. An `Err(reason)` is an error finding without a stage id:
   `dry run: provision failed in the base checkout: <reason cut to 300 characters>`; push it and return `Ok`
   without running any criterion (every later result would be noise). With no entries nothing runs.
6. Per stage, per command in the order acceptance, after_stage, wiring_tests, build a `DryCommand` (`dir` is
   `checkout.root().join(&stage.working_dir)`; `timeout` is `extended_criterion_timeout(..)` for acceptance,
   `check.timeout(DEFAULT_CHECK_TIMEOUT)` for after_stage, `WIRING_TEST_TIMEOUT` for wiring_tests) and call
   `settle_timeout(first, &mut rerun)` where both closures call `execute(&command)` and `classify_with` its
   result: a command whose first run times out runs once more in the same checkout and the second outcome is
   the one reported. The `has_summary` flag comes from `has_test_summary(&command.command, &command.dir,
   &result)`.
7. Push the findings by `severity(outcome, !stage.dependencies.is_empty())`: `Error` -> `report.errors`
   (`VerifyError { stage_id: Some(id), message }`, message `dry run: <field> <index> `<command>` <what>` with
   `<what>` one of `hit an environment failure (<marker>) at the base commit`, `cannot start at the base commit
   (<marker>) and the stage has no dependency that could provide it`, `timed out at the base commit after <n>s,
   also on a second run in the same checkout`); `Note` ->
   `report.notes` (`dry run: <field> <index> `<command>` <what>` with `<what>` one of `already passes at the base
   commit`, `fails at the base commit (expected before the stage's work)`, `cannot start at the base commit
   (<marker>); an earlier stage may provide it`, `selects zero tests at the base commit (expected when the plan
   adds them)`). `Warning` is produced only by `stage_cannot_tell` over the
   stage's acceptance outcomes -> `report.warnings.baseline` (`dry run: stage `<id>` passes every acceptance
   criterion at the base commit, so no criterion can tell a stage that did its work from one that did nothing`).
   Cut each command to 120 characters. `report.valid` is recomputed from `errors.is_empty()`.
8. Print one progress line per command to stderr (`dry run: <stage> <field> <index> ...`), and a second line
   (`dry run: <stage> <field> <index> timed out; running it once more`) before a rerun, because a cold build can
   run for minutes; they are the only output outside the report.

`run_dry_run(path, report)` is the confined entry: it parses the metadata, resolves each stage's confinement (as step 6 does), and when any is `Confined` and `crate::process::sandbox_probe::confined_commands_runnable()` is false
pushes an error `loom plan verify --dry-run needs a shell where loom can confine commands (bubblewrap cannot
create its namespaces here); run it from the operator's shell` (without a stage id) and returns before anything
runs. Otherwise it calls `run_dry_run_with` with the real executor: for `acceptance`, `run_acceptance_with_config(
&stage, Some(&command.dir), &CriteriaConfig::default().with_tree_root(Some(checkout_root)).with_only(Some(
BTreeSet::from([command.index]))))` (cache off), returning the single `CriterionResult` and the `failures` of an
`AcceptanceResult::Failed`; for `after_stage` and `wiring_tests`, `run_spec_in_optional_tree(&CommandSpec::shell(
cmd), Some(root), Some(&command.dir), command.timeout, confinement)` with no failures. `DryCommand.tree_root` is the
checkout root and `DryCommand.confinement` the stage's resolved confinement (`resolve_confinement(
stage.sandbox.command_confinement, Some(metadata.loom.sandbox.command_confinement))`), both set in step 6, so the
executor needs no second channel. The executor looks the `Stage` up by `stage_id` in a map `run_dry_run` builds from the same definitions.

## Traps

- The checkout holds tracked files only: no `node_modules`, no `target/`. A criterion that builds from
  nothing may time out on its first run; the rerun in the same checkout is the only allowance, and a second
  timeout is an error by the plan's decision, which the plan-writer doctrine (W5) says. Do not add a cache
  shortcut or a third run.
- A rerun doubles the worst-case wall time of a slow command; the progress lines are how the author sees why.
- `report.rs` is shared with the planner UI (`plan_server/files/validate.rs` calls `verify_text`): keep
  `verify_text`'s signature and behaviour.
- `BaseCheckout` removes its directory on drop; keep it alive until every stage has run.
- `CriterionResult::stdout`/`stderr` are the captured output; lower-case once before matching markers.
- Never run a command outside the checkout and never with `CommandConfinement` weaker than the stage's.

## Tests (in `dry_run_tests.rs`)

`classify` on hand-built `CriterionResult::new(..)` values: exit 127 with `command not found` -> `Missing`;
exit 1 with `assertion failed` -> `Failed`; `classify_with(.., true)` on an exit-101 result whose output holds the
text `Operation not permitted` -> `Failed`, and the same result through `classify` -> `Environment`; `timed_out`
-> `TimedOut`; `settle_timeout`: a `TimedOut` first outcome calls the rerun exactly once and returns its outcome
(`Passed`, or `TimedOut` again), and a `Passed`, `Failed` or `Environment` first outcome never calls it; the
zero-test failure string -> `ZeroTests`; exit 0 -> `Passed`. `severity` for all six outcomes with and without
dependencies; `stage_cannot_tell` (all passed and no dependencies -> true; one failed, a dependency, or an empty
list -> false).

`run_dry_run_with` with a recording executor (a closure; no bubblewrap, so these run everywhere): the executor
sees the same checkout root for every call and never the working tree (a committed plan whose stage checks
`dir.join("marker.txt")` while `marker.txt` exists only as an untracked file in the repository); a first timeout
is followed by exactly one more call for that command and a second timeout is the reported error, while a pass on
the rerun is not; a stage with dependencies whose command exits 127 yields a note and no error, and a stage
without dependencies yields an error; a stage whose every acceptance passes yields the baseline warning and one
with a failing criterion yields none (notes only); a provision entry (`touch provisioned.txt` in the checkout)
has run before the first executor call, and a failing provision (`exit 3`) yields the `provision failed` error
with zero executor calls.

One real end-to-end test of `run_dry_run` (the only test that needs confinement; it guards itself with
`skip_unless(confined_commands_runnable(), ..)`): a committed plan whose standard stage runs `test -f marker.txt`
where `marker.txt` exists only as an untracked file reports a note, and one running `test ! -e marker.txt`
reports the stage-level warning; `sleep 4` with `timeout_secs: 2` is an error naming the second run.

## Done

Your one check once the crate compiles: `cargo test --lib commands::plan::verify::`.
