# W2: host child `loom stage verify-host` (stage host-jobs)

Read `doc/plans/briefs/host-verification/common.md` first. Section 3.6 pins your interface. You also use W1's store (3.2) and W3's `digest::tree_digest` (3.5) by their pinned names. Paths are relative to `loom/`.

## Territory

You own:

- `src/host_verify/child.rs` and `tests_child.rs`;
- `src/commands/stage/verify_host.rs` (new);
- edits to `src/commands/stage/mod.rs` and `src/commands/mod.rs` (declarations only, as needed), and the visibility of `format_criterion_result` in `src/commands/stage/criterion_output.rs`;
- edits to `src/cli/types_stage.rs` (the hidden subcommand on `StageCommands`) and `src/cli/dispatch.rs` (its arm);
- edits to `src/verify/criteria/config.rs` and `src/verify/criteria/runner.rs` (a per-criterion observer).

W1 owns `src/host_verify/mod.rs` and declares `pub mod child;` there. Never edit other workers' files.

## Steps

1. **Per-criterion observer.**
   - `verify/criteria/runner.rs` runs the acceptance list and returns results at the end.
   - Add an optional observer to `CriteriaConfig` (`config.rs`): a builder method `with_observer(self, observer: Arc<dyn Fn(usize, &CriterionResult) + Send + Sync>) -> Self`. The runner calls it as each criterion finishes, with the criterion's index and result.
   - Use whatever the runner's per-criterion result type is called; read the file.
   - Default: none. Every existing caller is unchanged. Keep `runner.rs` within its ledger limits; check `maintainability-baseline.txt` for rows naming it.
2. **`child.rs`, `pub fn run_job(work_dir, stage_id, job_id) -> Result<JobResult>`.**
   - Load the current record with `store::load_current`. It must exist, have this `id`, and be `Running`; anything else is an error.
   - Load the stage (`verify::transitions::load_stage`). Resolve the worktree and acceptance dir as `commands/stage/complete_resolved.rs::verify_resolved_completion` does: `WorkDir::new(work_dir)?.main_project_root()`, `git::worktree::stage_worktree_path`, `commands::stage::acceptance_runner::resolve_acceptance_dir` (already `pub(crate)`).
   - Check `git rev-parse HEAD` in the worktree against `record.head`, and `digest::tree_digest(&worktree)` against `record.tree_digest`. A mismatch is an error naming both values.
   - Acceptance: build the `CriteriaConfig` the way `acceptance_runner.rs::run_acceptance_with_display` does, with the cache dir unless `record.no_cache`. The observer turns each result into `CriterionOutcome { label: format!("acceptance-{i}"), passed, summary }`, where `summary` is the text `commands/stage/criterion_output.rs::format_criterion_result` produces for it (raise it to `pub(crate)` and the `mod criterion_output;` line in `commands/stage/mod.rs` to `pub(crate) mod`), cut to 8 KiB on a char boundary. Before appending, it writes the criterion's full captured stdout, then stderr, through W1's `store::save_criterion_log(work_dir, stage_id, job_id, "acceptance-<i>", bytes)` and sets `log` to the returned file name. A result answered from the acceptance cache has no captured output and gets no log. It then calls `store::append_progress`. An acceptance list that is empty yields no outcome.
   - Goal-backward: only when the stage has goal checks (`commands/verify.rs::run_and_verify_stage_goal`, already `pub`). One outcome labelled `goal-backward`, with the printed gap list as summary.
   - Recompute the tree digest. If it differs, return the result with `error: Some("worktree changed during the run")` and `passed: false`.
   - `passed` = every outcome passed and no error.
   - `store::save_result` before returning.
   - The function prints nothing itself. The CLI wrapper prints one summary line per outcome to stdout, which the runner captures in `child.log`.
3. **CLI.** `loom stage verify-host <stage-id> --job <job-id>`, hidden from help (`#[command(hide = true)]`). `StageCommands` is defined in `cli/types_stage.rs`; find its dispatch in `cli/dispatch.rs`. `dispatch` is ledgered at 69 lines, so add the arm without growing it: put the arm in the stage-commands sub-dispatch if one exists, otherwise move one existing arm's body into a helper first.

   `commands/stage/verify_host.rs::execute(stage_id: String, job: String) -> Result<()>`:
   - Refuses with exit 2 when `LOOM_SESSION_ID` or `LOOM_STAGE_ID` is set, printing `verify-host runs only from the loom daemon`. Use `std::process::exit(2)` after printing, as other commands that set specific codes do (`rg -n 'process::exit' src/commands | head`).
   - Resolves the state root with `crate::commands::common::work_dir_path()`.
   - Runs `run_job`.
   - Exit 0 when passed, 1 when failed. On an error before a result could be written, exit 2.
4. **Environment.**
   - The runner already strips session variables. The child must still never pass `LOOM_SESSION_*` through to criteria: `verify/criteria/confine.rs::apply_confined_environment` builds the allowlist from the child's environment, so nothing extra is needed. Confirm by reading it, and say so in your report.
   - Confirm also that criteria see the same `PATH` and `HOME` a session's own commands see: the daemon spawns sessions from the same environment (`daemon/server/environment.rs::HOST_ENV_ALLOWLIST`, the session wrapper in `orchestrator/terminal/native/wrapper.rs`). Report any variable a session has that the daemon-run child lacks.

## Tests you write (`tests_child.rs`)

- A fixture repository with a worktree and a stage whose acceptance is `["true", "false"]` (reuse the fixture helpers other `verify` or `commands::stage` tests use; `rg -n 'fn init_repo_with_worktree|fn worktree_fixture' src tests`): two outcomes, the second failed, progress has two lines, `passed` false.
- A criterion that prints 5,000 lines and exits 1: its summary is at most 8 KiB, and `acceptance-0.log` holds all 5,000 lines.
- A record whose head differs from the worktree HEAD → error result.
- A tree changed during the run: an acceptance command that writes a tracked file → error `worktree changed during the run`.
- A `no_cache` record runs a criterion a cached pass would have skipped.
- A record not `Running` → `Err`, no result written.

These run real confined criteria (bubblewrap). They pass on the host and in this plan's own sessions, which run the installed binary's capsule.

## Proof (run once, from `loom/`)

```bash
cargo test --lib host_verify::tests_child 2>&1 | tail -30
```

If the build fails only on W1's or W3's files not existing yet, report that and stop.

## Report

Files changed; how the observer is threaded (signature); the dispatch placement; the environment comparison; the proof output.
