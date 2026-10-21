# W1: host-job runner and lifecycle (stage host-jobs)

Read `doc/plans/briefs/host-verification/common.md` first. Section 3 pins every name you write: 3.0 module layout, 3.1 records, 3.2 store, 3.3 submit and cancel, 3.4 runner. Paths below are relative to `loom/`.

## Territory

You own:

- `src/lib.rs` (one line: `pub mod host_verify;` after `pub mod hooks;`);
- `src/host_verify/mod.rs`, `store.rs`, `submit.rs`, `runner.rs`, and their tests `tests_store.rs`, `tests_submit.rs`, `tests_runner.rs`;
- `src/orchestrator/core/host_jobs.rs` (new);
- edits to `src/orchestrator/core/{mod.rs, run.rs, orchestrator.rs, event_handler.rs}`, `src/orchestrator/core/event_handler/{stage_takedown.rs, recover_hung.rs}`, `src/commands/stage/{state.rs, skip_retry.rs}`, `src/git/cleanup/batch.rs`, and the visibility of `COMPLETION_MARGIN_SECS` in `src/orchestrator/monitor/stall_line.rs`.

W2 writes `src/host_verify/child.rs` and `tests_child.rs`; W3 writes `digest.rs`, `wait.rs`, `tests_digest.rs` and `tests_wait.rs`. You write `mod.rs` with every declaration of common.md 3.0 from the start, theirs included: `pub mod store; pub mod submit; pub mod runner; pub mod digest; pub mod wait; pub mod child;` and `#[cfg(test)] mod tests_store; mod tests_submit; mod tests_runner; mod tests_child; mod tests_digest; mod tests_wait;` (each with its own `#[cfg(test)]`). Never edit their files. The crate compiles once all three of you return.

## Steps

1. **Records** (`mod.rs`), exactly as common.md 3.1, including `JobResult.policy_hash`.
   - `JobState::is_terminal`.
   - `pub fn new_job_id() -> String` (uuid v4, simple form, lowercase). `submit` calls it.
   - Keep `mod.rs` small: types, `is_terminal`, `new_job_id`, module declarations.
2. **Store** (`store.rs`), common.md 3.2.
   - Validate `stage_id` with `crate::validation::validate_id`, and `job_id` as exactly 32 chars of `[0-9a-f]`.
   - Canonicalize `work_dir` once, then go through `crate::fs::safe_fs` for everything beneath it: `safe_open_dirfd` on the canonical root, `open_safely` and the `safe_*_in_workdir` helpers (O_NOFOLLOW, dirfd-anchored), `safe_rename_in_workdir` for the atomic rename, `flock_exclusive` on `verification/<stage>/.lock` for `lock_stage`. Refuse a symlinked `verification`, stage directory, job directory or target file before creating or locking anything. Do not use `fs::locking::locked_dir_update`: its `lock_parent_dir` runs `create_dir_all`, which follows a symlinked `verification/` out of the state root.
   - `save_current` also writes `<job dir>/record.json`, creating the job directory.
   - `append_progress` refuses past 2 MiB of `progress.jsonl` by writing one final outcome `{label: "progress-truncated", passed: false, summary: "progress truncated at 2 MiB"}` and ignoring later appends.
   - `save_criterion_log(work_dir, stage_id, job_id, label, bytes) -> Result<String>` writes `<job dir>/<label>.log` (label validated as `[a-z0-9:_-]+`), keeping content plus the final `[truncated at 4 MiB]` line within `safe_fs::MAX_LOG_BYTES`, and returns the file name. W2's child calls it.
   - `save_job_file` / `load_job_file` (name `[a-z0-9._-]+`, atomic). Stage 2 writes `contract-reports.json` and the `applied` marker through them.
   - `append_group` / `load_groups` (common.md 3.2): `groups` holds one decimal pgid per line, capped at 4,096 lines; W2's child appends through it.
   - Pruning: when a new job is queued, remove job directories other than the three newest by directory mtime, never the current job's.
3. **Submit and cancel** (`submit.rs`), common.md 3.3, under `store::lock_stage`.
   - The covers rule, the `Refused` case, `requests` keeping the last 16 ids, `superseded by <new id>`.
   - Queueing creates the job directory (through `save_current`).
4. **Runner** (`runner.rs`), common.md 3.4.
   - Hold `HashMap<String /*job id*/, (String /*stage*/, OwnedProcess)>`. `process/owned.rs`: `OwnedProcess::new(child)` wraps a child you spawned with `std::process::Command`; `retire` kills the group and reaps it (it may block up to `CLEANUP_GRACE`, 1 s, which is the only blocking the tick allows besides `try_wait`).
   - Every transition is a compare-and-set under `lock_stage`: reload, check id and expected state, write. Write `Running` and `started_at` before spawning, `pid` after.
   - The child's stdout and stderr go to `child.log` in the job dir, opened for append. Check its size each tick; over 4 MiB, retire the child with detail `child output exceeded 4 MiB`.
   - Environment: start from the daemon's own environment, remove the variables common.md 3.4 lists, set `LOOM_WORK_DIR` to the canonical state root, and `current_dir(WorkDir::new(&work_dir)?.main_project_root())`. The child resolves nothing from its own cwd.
   - `job_deadline_secs(&stage)` as pinned (acceptance `timeout_secs`: `models/stage/checks.rs`; counts from `stage.wiring_tests`, `dead_code_check`, `regression_test`, `contracts`, `after_stage`; margin `stall_line::COMPLETION_MARGIN_SECS`, raise it to `pub(crate)` if needed). Load the stage with `crate::verify::transitions::load_stage`; if it cannot be loaded, the job turns `Failed` with that error and never starts.
   - `with_deadline_override(self, Duration)` replaces the computed deadline for every job of this runner. `RunnerConfig` keeps exactly its three fields: a frozen contract builds it as a struct literal.
   - `tick` returns exactly the records of jobs it ended or reaped in this call, including one cancelled or superseded while running.
   - Retiring a job's child (cancel, supersede, deadline, output cap, shutdown) also SIGKILLs every group `store::load_groups` lists for it, after `OwnedProcess::retire` (common.md 3.4 "Retiring a job kills its criteria too"). Put this in one helper every retire path calls; `OwnedProcess`'s own `Drop` is not enough, so the shutdown path calls the helper for each held job before dropping the runner.
   - `recover_after_restart`: walk `verification/*/current.json`; turn `Running` into `Interrupted`; SIGTERM every process group whose `ps -o command= -p <pid>` (run through `std::process::Command`, `/bin/ps` then `ps` on PATH) contains `verify-host <stage> --job <id>`; then one 2 s sleep; then SIGKILL the ones still alive. Never signal a recycled pid. Then SIGKILL each listed criterion group whose leader still leads it (`ps -o pgid= -p <pgid>` prints `<pgid>`). Return the changed records.
5. **Daemon glue** (`orchestrator/core/host_jobs.rs`, declared `mod host_jobs;` in `orchestrator/core/mod.rs`):
   - Add `impl Orchestrator { pub fn tick_host_jobs(&mut self); pub fn set_host_job_runner(&mut self, runner: HostJobRunner) }` (common.md 3.4; `pub` like `drain_session_inboxes`, so stage 2's e2e drives the production tick). Call `self.tick_host_jobs();` in `run.rs::run_tick` right after `self.drain_session_inboxes();`.
   - Construct the runner with `HostJobRunner::new(work_dir, RunnerConfig::daemon_default()?)`; a construction error is logged and leaves host jobs disabled for this run, never a panic.
   - `tick_host_jobs` logs each record that turned terminal at `info` (stage, job, state). Stage `daemon-completion` adds the apply call here. A runner error is logged with `tracing::warn!` and never propagated: a broken job must not stop the orchestrator.
   - Call `crate::host_verify::runner::recover_after_restart(..)` once in `run.rs::initialize_run` and log what it returns (stage 2 applies those records).
   - Where the runner lives:
     - `orchestrator.rs` is 399 lines, so a new field cannot be added there as is.
     - Either move an existing self-contained block out of `orchestrator.rs` into a new file to make room for `host_jobs: HostJobRunner` (field plus constructor line), or keep the runner in `host_jobs.rs` inside a `struct HostJobs` that `Orchestrator` reaches through an existing extension point.
     - Choose the option that keeps every file at or under 400 lines and every function at or under 50. Say which you chose in your report.
   - Graceful shutdown: when the orchestrator stops (`run.rs::run` returns), set each running record `Cancelled` with detail `daemon stopped` under the stage lock, so waiters stop waiting; then dropping the runner kills its children through `OwnedProcess::drop`.
6. **Cancellation call sites.** Each calls `crate::host_verify::submit::cancel(work_dir, stage_id, "<reason>")` path-qualified, logs and ignores an error, and keeps ledgered functions and limit-sized files line-neutral (common.md section 2: `handle_one_event` 116, `retry` 209, `stage_takedown.rs` at 400 lines; move a helper out first if needed):
   - `event_handler/stage_takedown.rs::take_down_agents` (reason `stage agents taken down`). Takedown, retire, handoff, hung requeue (`loop_recovery/mod.rs`), parking (`loop_recovery/park.rs`) and the contract hand-off (`contract_phase.rs`) reach it; confirm each path reaches it and add a direct call where one does not;
   - `event_handler.rs::begin_handoff` (`handoff began`) unless it already reaches `take_down_agents`;
   - `event_handler/recover_hung.rs`, the path that kills a hung session (`hung session recovered`), likewise;
   - `loom stage reset` in `commands/stage/state.rs` (`stage reset`), and `loom stage retry` in `commands/stage/skip_retry.rs` (`stage retried`): put the call in a helper `retry` already calls (for example `persist_retry_delta`), never in `retry` itself. These run in the operator's CLI process; the stage lock makes their write safe against the daemon;
   - worktree removal in `git/cleanup/batch.rs::drain_spool_before_removal`, or the function that removes the worktree right after it (`worktree removed`).

   A judge session's close does not cancel: a check it requested finishes harmlessly. `loom worktree remove` (`git::remove_worktree`) and `loom init`'s cleanup do not cancel either: a running child then fails on the missing worktree, bounded by its deadline. Name both in your report.

## Tests you write

- `tests_store.rs`: id validation; symlinked `verification`, stage dir and target file each refused with nothing created behind the link; a symlinked state root (the `.loom/work` case) accepted; atomic save and load round trip; `record.json` written beside `current.json`; progress cap; pruning keeps three.
- `tests_submit.rs`: queued (job dir created), joined (requests appended, capped at 16), superseded (old record cancelled with detail), a `no_cache` request superseding a cached job and a cached request joining a `no_cache` job, cancel on a terminal record returns `None`.
- `tests_runner.rs` (use `RunnerConfig { program: "/bin/sh".into(), args_prefix: vec!["-c".into(), "<script>".into(), "sh".into()], slots }`):
  - a child that writes a passing `result.json` turns `Passed` (the script reads `LOOM_WORK_DIR` to find the job dir);
  - one that exits 1 with no result turns `Failed` with the log tail, detail at most 512 chars;
  - two queued jobs with `slots: 1` run one after the other;
  - `with_deadline_override(Duration::from_secs(1))` on a `sleep 600` child turns `Failed` with detail `deadline of`, kills and reaps the child;
  - a record cancelled while its child runs stays `Cancelled` with its detail after the reap (the CAS refuses to overwrite it);
  - `job_deadline_secs` counts a `timeout_secs: 3600` criterion as 3600;
  - recovery leaves `Queued` alone;
  - a cancelled job's listed criterion group dies: the script runs `set -m; sleep 600 & echo $! >> "<job dir>/groups"; wait` (with `set -m` the background job leads its own group, as a confined criterion does), the test reads the pid from `groups`, cancels, ticks, and asserts within 5 s that `kill(pid, 0)` reports no such process. The same for the deadline path.
- Cancellation sites: one test per site that drives the site's own function (reset, retry, the takedown helper, `begin_handoff` or the hung-recovery path, the worktree-removal helper), never `submit::cancel` directly, on the fixtures the neighbouring tests of that module use, and asserts the record `Cancelled` with the site's reason. Where a site's function cannot be driven without a live session, say so in your report and name the test that covers the nearest callable function.

  Each test writes the stage file the runner reads; reuse the stage fixtures other orchestrator tests use (`rg -n 'fn write_stage|fn stage_fixture' src/orchestrator`).

## Proof (run once, from `loom/`)

```bash
cargo test --lib host_verify:: 2>&1 | tail -30
```

The crate builds only once W2 and W3 have returned. If it fails only on their missing files, report that and stop; do not create their files.

## Report

Files changed; where the runner lives and why; each cancellation site with `file:line`, and each path you confirmed reaches `take_down_agents`; the proof output; anything surprising.
