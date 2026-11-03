# check-routes W1: every gate failure carries a disputable address

Tier: sonnet. Wave 2, after W0. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `tests/check_routes_contracts.rs` (never edit it).

## Why

`loom stage complete` labels a wiring failure `[wiring n]` (`verify/goal_backward/mod.rs`,
`wiring_gaps` / `indexed_gaps`), so an agent knows what to dispute. Reachable, artifact,
dead-code, regression-test and after-stage gaps carry no index, and integration-verify's
re-verification of completed stages names no owner, so no dispute can address them. Plan 08's
integration-verify failed on exactly such a gap.

## Edits (you own these files)

1. `src/verify/goal_backward/mod.rs`
   - Artifacts: verify one entry at a time (as `wiring_gaps` does) and prefix each gap with
     `crate::models::dispute::gap_prefix(CriterionField::Artifacts, index)`.
   - `dead_code_check` and `regression_test`: prefix every gap with
     `gap_prefix(CriterionField::DeadCodeCheck, 0)` / `gap_prefix(CriterionField::RegressionTest, 0)`.
   - `reachable_gaps`: verify one entry at a time against the one graph and prefix with
     `gap_prefix(CriterionField::Reachable, index)`. When `build_for_worktree` fails, return one
     gap per entry, `[reachable <n>] could not be evaluated: <error chain>` (GapType: the one
     `verify_reachable` uses for an unreachable unit; suggestion: "dispute it with `loom stage
     dispute-criteria <stage> --field reachable --criterion-index <n>`"), never an `Err`: a
     check that cannot evaluate must still be addressable.
   - Replace `indexed_gaps`' own `format!` with `gap_prefix` so one function owns the label.
   - Keep the existing test `gaps_name_their_wiring_and_wiring_tests_index` passing; add tests
     for each new label in the same module's tests.
2. `src/commands/stage/complete_verification.rs`
   - `run_after_checks`: run `after_stage` one entry at a time and prefix with
     `gap_prefix(CriterionField::AfterStage, index)`.
   - `aggregated_wiring`: extract `pub fn reverify_completed_wiring(work_dir: &Path,
     worktree_root: &Path, plan_version: u32) -> Result<Vec<VerificationGap>>` that returns
     every gap of every completed stage, each prefixed with
     `crate::models::dispute::owner_gap_label(&definition.id, CriterionField::Wiring, index)`;
     `aggregated_wiring` calls it and bails with the gap descriptions joined (today it bails
     with a count only; the message must name every gap so a host job's outcome carries it).
   - Export `reverify_completed_wiring` through `commands::stage` (`pub use` in
     `src/commands/stage/mod.rs` or the module that already re-exports from `complete`; find it
     with `rg -n 'pub(\(crate\))? use .*complete' src/commands/stage`). If that file is not
     yours, report the one-line export to the orchestrator instead.
3. `src/commands/stage/complete_verification_v2.rs`
   - `reverify_reachable`: extract `pub fn reverify_completed_reachable(work_dir: &Path,
     worktree_root: &Path) -> Result<Vec<VerificationGap>>` (one graph for every stage, as now)
     whose gaps carry `owner_gap_label(&definition.id, CriterionField::Reachable, index)`; a
     graph that cannot be built yields one `could not be evaluated` gap per entry of every stage,
     as in step 1. `reverify_reachable` bails with every gap's description and suggestion.
   - The suggestion names the route: "dispute it from integration-verify with `loom stage
     dispute-criteria integration-verify --owner <stage> --field reachable --criterion-index
     <n>`".
4. `src/commands/stage/acceptance_runner.rs` (`print_acceptance_failure_guidance`): the
   guidance text lists every `--field` value (`live` included) and `--owner`, in one paragraph.

## Traps

- `completed_definitions` reads definitions from the plan file, not the stage file: an
  amendment rewrites both, and the re-verification must keep reading the plan (contract
  `amended-reachable-passes-reverify`).
- A stage's `working_dir` is joined onto `worktree_root` for wiring (`stage_wiring_gaps`), while
  reachable builds one graph from `worktree_root`: keep both as they are.
- Message size: a host job outcome summary is cut at `SUMMARY_MAX_BYTES`; put the labels first
  in each line so a cut keeps them.
- The live-test-lane plan's `live` check sits in `run_checks` after `after-stage`, and its
  `[live <i>]` labels are printed by `crate::live::completion::evaluate` (the same text as
  `gap_prefix(CriterionField::Live, i)`). Leave both: `src/live/` is not this stage's, and
  `run_checks` stays under 50 lines.

## Done

Your one check after the stage compiles: `cargo test --lib verify::goal_backward::`.
