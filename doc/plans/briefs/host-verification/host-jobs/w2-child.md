# W2: host child `loom stage verify-host` (stage host-jobs)

Read `doc/plans/briefs/host-verification/common.md` first. Section 3.6 pins your interface. You also use W1's store (3.2) and W3's `digest::{tree_digest, knowledge_tree_digest}` (3.5) by their pinned names. Paths are relative to `loom/`.

## Territory

You own:

- `src/host_verify/child.rs` and `tests_child.rs`;
- `src/commands/stage/verify_host.rs` (new);
- edits to `src/commands/stage/mod.rs` and `src/commands/mod.rs` (declarations only, as needed), and the visibility of `format_criterion_result` in `src/commands/stage/criterion_output.rs`;
- edits to `src/cli/types_stage.rs` (the hidden subcommand on `StageCommands`) and `src/cli/dispatch_stage.rs` (its arm);
- edits to `src/verify/criteria/config.rs` and `src/verify/criteria/runner.rs` (a per-criterion observer);
- `src/verify/criteria/spawn_observer.rs` (new), its declaration and re-export in `src/verify/criteria/mod.rs`, and one call in `src/verify/criteria/confine.rs` (the spawn observer, common.md 3.6).

W1 owns `src/host_verify/mod.rs` and declares `pub mod child;` and `#[cfg(test)] mod tests_child;` there; do not add a `#[path] mod tests` to `child.rs`. Never edit other workers' files.

## Steps

1. **Per-criterion observer.**
   - `verify/criteria/runner.rs` runs the acceptance list and returns results at the end.
   - Add an optional observer to `CriteriaConfig` (`config.rs`): a builder method `with_observer(self, observer: CriterionObserver) -> Self`, where `CriterionObserver` is a newtype over `Arc<dyn Fn(usize, &CriterionResult) + Send + Sync>` with a manual `Debug` impl (`CriteriaConfig` derives `Debug`, which a bare `Arc<dyn Fn>` cannot satisfy). The runner calls it as each criterion finishes, with the criterion's index and result.
   - Use whatever the runner's per-criterion result type is called; read the file.
   - Default: none. Every existing caller is unchanged. Keep `runner.rs` within its limits.
   - **Spawn observer** (`verify/criteria/spawn_observer.rs`): `pub struct SpawnObserver(Box<dyn Fn(u32) + Send + Sync>)`, `pub fn set_spawn_observer(observer: SpawnObserver) -> SpawnObserverGuard` (a process-wide `Mutex<Option<Arc<SpawnObserver>>>`; setting replaces any current observer, and dropping the guard clears it) and `pub(crate) fn notify_spawned(pid: u32)` (no observer → nothing; clone the `Arc` and release the lock before calling it). Declare it in `verify/criteria/mod.rs` and re-export `set_spawn_observer` and `SpawnObserver`. In `confine.rs::PreparedCommand::spawn_retaining_resources`, call `spawn_observer::notify_spawned(child.id())` after a successful `spawn()` and before returning the child; keep `confine.rs` within its limits. A criterion's pid is its process-group id (`process_group(0)`).
2. **`child.rs`, `pub fn run_job(work_dir, stage_id, job_id) -> Result<JobResult>`.**
   - Load the current record with `store::load_current`. It must exist, have this `id`, and be `Running`; anything else is an error and writes no result.
   - Install the spawn observer before running anything and hold its guard until `run_job` returns: a closure that calls `store::append_group(work_dir, stage_id, job_id, pid)` and logs and ignores an error. In `verify-host` one job runs per process. In the library test binary every `tests_child.rs` test that calls `run_job` is `#[serial]` (`serial_test`), so two jobs never share the observer; a confined command another test spawns meanwhile only adds a line to that job's `groups`, which no lib test kills from.
   - Load the stage (`verify::transitions::load_stage`). Resolve the tree and acceptance dir as `commands/stage/complete_resolved.rs::verify_resolved_completion` does: `WorkDir::new(work_dir)?.main_project_root()`, `git::worktree::stage_worktree_path(&stage, repo_root)`, `commands::stage::acceptance_runner::resolve_acceptance_dir` (already `pub(crate)`). A knowledge stage verifies the repository root through `acceptance_runner::resolve_knowledge_execution_paths` and uses `digest::knowledge_tree_digest`.
   - Check `git rev-parse HEAD` in the tree against `record.head`, and the digest against `record.tree_digest`. A mismatch is an error naming both values.
   - Acceptance: build the `CriteriaConfig` the way `acceptance_runner.rs::run_acceptance_with_display` does, with the cache dir unless `record.no_cache`. The observer turns each result into `CriterionOutcome { label: format!("acceptance-{i}"), passed, summary }`, where `summary` is the text `commands/stage/criterion_output.rs::format_criterion_result` produces for it (raise it to `pub(crate)` and the `mod criterion_output;` line in `commands/stage/mod.rs` to `pub(crate) mod`), cut to 8 KiB on a char boundary. Before appending, it writes the criterion's full captured stdout, then stderr, through W1's `store::save_criterion_log(work_dir, stage_id, job_id, "acceptance-<i>", bytes)` and sets `log` to the returned file name. A result answered from the acceptance cache has no captured output and gets no log. It then calls `store::append_progress`. An acceptance list that is empty yields no outcome.
   - Goal-backward: the goal checks live in the plan, found through `crate::fs::resolve_source_path(work_dir)` (the state root's `config.toml`). Without a plan source the result is an error, never a skipped step. When the stage's plan entry has goal checks, run `commands/verify.rs::run_and_verify_stage_goal` (already `pub`) and append one outcome labelled `goal-backward`, with the printed gap list as summary.
   - Re-read HEAD and recompute the digest. If either differs from the record, return the result with `error: Some("worktree changed during the run: <what changed>")` and `passed: false`. A clean commit leaves the digest unchanged, so HEAD is compared on its own.
   - `passed` = every outcome passed and no error.
   - `store::save_result` before returning.
   - The function prints nothing itself. The CLI wrapper prints one summary line per outcome to stdout, which the runner captures in `child.log`.
3. **CLI.** `loom stage verify-host <stage-id> --job <job-id>`, hidden from help (`#[command(hide = true)]`): the variant is `StageCommands::VerifyHost { stage_id: String, #[arg(long)] job: String }` (the stage's wiring checks `StageCommands::VerifyHost` in `dispatch_stage.rs` and `run_job(` in `verify_host.rs`). `StageCommands` is defined in `cli/types_stage.rs` and matched in `cli/dispatch_stage.rs::dispatch_stage` (49 lines; the limit is 50): add the arm as one line that calls `commands::stage::verify_host::execute(stage_id, job)`. `cli/dispatch.rs` only forwards to `dispatch_stage` and stays untouched.

   `commands/stage/verify_host.rs::execute(stage_id: String, job: String) -> Result<()>`:
   - Refuses with exit 2 when `LOOM_SESSION_ID` or `LOOM_STAGE_ID` is set (either one alone), printing `verify-host runs only from the loom daemon`. Use `std::process::exit(2)` after printing, as other commands that set specific codes do (`rg -n 'process::exit' src/commands | head`).
   - Resolves the state root from `LOOM_WORK_DIR` (required; canonicalized; exit 2 with a message when unset or not a directory). Never `crate::commands::common::work_dir_path()`: it resolves from the current directory, which in a worktree is the live plan's `.loom/work`.
   - Runs `run_job`.
   - Exit 0 when passed, 1 when failed. On an error before a result could be written, exit 2.
4. **Environment.**
   - The runner already strips session variables. The child must still never pass `LOOM_SESSION_*` through to criteria: `verify/criteria/confine.rs::apply_confined_environment` builds the allowlist from the child's environment, so nothing extra is needed. Confirm by reading it, and say so in your report.
   - Confirm also that criteria see the same `PATH` and `HOME` a session's own commands see: the daemon spawns sessions from the same environment (`daemon/server/environment.rs::HOST_ENV_ALLOWLIST`, the session wrapper in `orchestrator/terminal/native/wrapper.rs`). Report any variable a session has that the daemon-run child lacks.

## Tests you write (`tests_child.rs`)

Every test here runs real confined criteria, so each opens with the `sandbox_probe::skip_unless(confined_commands_runnable(), ..)` guard of common.md section 5. They run on the host and in this plan's sessions.

- A fixture repository with a worktree, a state root whose `config.toml` names a plan file defining the stage (no goal checks), and a stage whose acceptance is `["true", "false"]` (reuse the fixture helpers other `verify` or `commands::stage` tests use; `rg -n 'fn init_repo_with_worktree|fn worktree_fixture' src tests`): two outcomes, the second failed, progress has two lines, `passed` false.
- A criterion that prints 5,000 lines and exits 1: its summary is at most 8 KiB, and `acceptance-0.log` holds all 5,000 lines.
- A record whose head differs from the worktree HEAD → error result.
- A tree changed during the run: an acceptance command that writes a tracked file → error `worktree changed during the run`.
- HEAD moved cleanly during the run: run `run_job` on a thread with an acceptance command that waits for a `go` file in a separate TempDir (`while [ ! -f <dir>/go ]; do sleep 0.1; done`; grant nothing else); while it waits, commit a change in the worktree from the test, then create `go`. The digest is unchanged (clean before, clean after), and the result is the error `worktree changed during the run` naming HEAD.
- A knowledge stage: the record's head and digest are the repository root's (`knowledge_tree_digest`), the criterion runs in the repository root (an acceptance `test -d .worktrees || pwd > <TempDir>/cwd` style probe), and the result passes.
- The spawn observer: a job whose acceptance is `["true"]` leaves a non-empty `groups` file, and the observer is cleared once `run_job` returns (a confined spawn afterwards adds no line). Assert "non-empty", never an exact count: non-serial tests in the same binary may spawn confined commands while the observer is set.
- A `no_cache` record runs a criterion a cached pass would have skipped.
- A record not `Running` → `Err`, no result written.
- No plan source in the state root → error result.

## Proof (run once, from `loom/`)

```bash
cargo test --lib host_verify::tests_child 2>&1 | tail -30
```

If the build fails only on W1's or W3's files not existing yet, report that and stop.

## Report

Files changed; how the observer is threaded (signature); the dispatch placement; the environment comparison; the proof output.
