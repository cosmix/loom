# W1: host-job runner and lifecycle (stage host-jobs)

Read `doc/plans/briefs/host-verification/common.md` first. Section 3 pins every name you write: 3.1 records, 3.2 store, 3.3 submit and cancel, 3.4 runner. Paths below are relative to `loom/`.

## Territory

You own:

- `src/lib.rs` (one line: `pub mod host_verify;` after `pub mod hooks;`);
- `src/host_verify/mod.rs`, `store.rs`, `submit.rs`, `runner.rs`, and their tests `tests_store.rs`, `tests_submit.rs`, `tests_runner.rs`;
- `src/orchestrator/core/host_jobs.rs` (new);
- edits to `src/orchestrator/core/{mod.rs, run.rs, orchestrator.rs, event_handler.rs}`, `src/orchestrator/core/event_handler/{stage_takedown.rs, recover_hung.rs}`, `src/commands/stage/{state.rs, skip_retry.rs}` and `src/git/cleanup/batch.rs`.

W2 writes `src/host_verify/child.rs` and `tests_child.rs`; W3 writes `digest.rs`, `wait.rs`, `tests_digest.rs` and `tests_wait.rs`. You write `mod.rs` with every module declaration from the start: `pub mod store; pub mod submit; pub mod runner; pub mod digest; pub mod wait; pub mod child;`, plus `#[cfg(test)]` declarations for your own test files. Never edit their files. The crate compiles once all three of you return.

## Steps

1. **Records** (`mod.rs`), exactly as common.md 3.1.
   - `JobState::is_terminal`.
   - A `pub fn new_job_id() -> String` (uuid v4, simple form, lowercase), which W3's drain handler does not call (submit does).
   - Keep `mod.rs` small: types, `is_terminal`, module declarations.
2. **Store** (`store.rs`), common.md 3.2.
   - Validate `stage_id` with `crate::validation::validate_id`, and `job_id` as exactly 32 chars of `[0-9a-f]`.
   - Atomic writes: temp file in the same directory, then rename.
   - Refuse symlinks: open with `O_NOFOLLOW` on the target, and `symlink_metadata` on `verification/`, the stage directory and the job directory, refusing any that is a symlink. Follow the existing helpers in `src/fs/` (look for an `O_NOFOLLOW` atomic write such as the receipts append in `src/models/forward_receipt` or `src/fs/locking.rs`) and reuse one rather than writing a third.
   - `append_progress` refuses past 2 MiB of `progress.jsonl` by writing one final outcome `{label: "progress-truncated", passed: false, summary: "..."}` and ignoring later appends.
   - `pub fn save_criterion_log(work_dir, stage_id, job_id, label: &str, bytes: &[u8]) -> Result<String>` writes `<job dir>/<label>.log` (label validated as `[a-z0-9:_-]+`), capped at 4 MiB with a final `[truncated at 4 MiB]` line, and returns the file name. W2's child calls it.
   - Pruning: when a new job is queued, remove job directories other than the newest three by `queued_at` of their record. A job directory holds only its own files; the record lives in `current.json`, so read each dir's mtime instead.
3. **Submit and cancel** (`submit.rs`), common.md 3.3.
   - Both run under a lock on the stage directory. Use the locking primitive `daemon/server/control_complete.rs` uses (`fs::locking::locked_dir_update`).
   - `requests` keeps the last 16 ids.
   - The `detail` of a superseded record is `superseded by <new id>`.
4. **Runner** (`runner.rs`), common.md 3.4.
   - Hold `HashMap<String /*job id*/, (String /*stage*/, OwnedProcess)>`. Read `process/owned.rs` for its API: `spawn`, `try_wait` or equivalent, `retire`.
   - The child's stdout and stderr go to `child.log` in the job dir, opened for append. Cap it at 4 MiB by checking its size each tick and retiring the child with detail `child output exceeded 4 MiB` when it is over.
   - Environment: start from the daemon's own environment, then remove every variable whose name starts with `LOOM_SESSION` and `LOOM_STAGE_ID`, `LOOM_SCRATCH_DIR`, `LOOM_WORKTREE_PATH`, `LOOM_SESSION_TYPE`, `LOOM_HOOK_CONTEXT`, `LOOM_CONTROL_BROKER`. Set `LOOM_WORK_DIR` to the state root.
   - Deadline:
     - `crate::orchestrator::monitor::stall_line::completion_line_secs(&stage)`, with the stage loaded through `crate::verify::transitions::load_stage`.
     - If the stage cannot be loaded, the job fails with that error and never starts.
     - Raise the function's visibility if needed (it is `pub(crate)`, which is enough inside the crate).
   - `tick` returns records that became terminal during the call.
   - `recover_after_restart`:
     - Walk `verification/*/current.json`.
     - Turn `Running` into `Interrupted`.
     - Send SIGTERM to the recorded process group, then SIGKILL after 2 s, only when `ps -o command= -p <pid>` (run through `std::process::Command`, PATH `/bin/ps` then `ps`) contains `verify-host <stage> --job <id>`. Never kill a recycled pid.
   - Never block the tick: no `wait()` without a timeout, no sleeping except the 2 s kill grace in recovery, which runs once at startup.
5. **Daemon glue** (`orchestrator/core/host_jobs.rs`):
   - Add `impl Orchestrator { pub(super) fn tick_host_jobs(&mut self) }`. Call `self.tick_host_jobs()` in `run.rs::run_tick` right after `self.drain_session_inboxes();`.
   - `tick_host_jobs` logs each record that turned terminal at `info` (stage, job, state). Stage `daemon-completion` adds the apply call here. A runner error is logged with `tracing::warn!` and never propagated: a broken job must not stop the orchestrator.
   - Call `host_verify::runner::recover_after_restart` once in `run.rs::initialize_run`.
   - Where the runner lives:
     - `orchestrator.rs` is 399 lines, so a new field cannot be added there as is.
     - Either move an existing self-contained block out of `orchestrator.rs` into a new file to make room for `host_jobs: HostJobRunner` (field plus constructor line), or keep the runner in `host_jobs.rs` inside a `struct HostJobs` that `Orchestrator` reaches through an existing extension point.
     - Choose the option that keeps every file at or under 400 lines and every function at or under 50. Say which you chose in your report.
   - Graceful shutdown: when the orchestrator stops (`run.rs::run` returns), dropping the runner kills its children through `OwnedProcess::drop`. Before that, set each running record `Cancelled` with detail `daemon stopped` so waiters stop waiting.
6. **Cancellation call sites.** Each calls `crate::host_verify::submit::cancel(work_dir, stage_id, "<reason>")`, logs and ignores an error, and must stay line-neutral where the file is at its limit (move a helper out first if needed):
   - stage takedown, in `event_handler/stage_takedown.rs::take_down_stage_agents` and `retire_stage_agents` (reason `stage agents taken down`);
   - `event_handler.rs::begin_handoff` (`handoff began`);
   - `event_handler/recover_hung.rs`, the path that kills a hung session (`hung session recovered`);
   - `loom stage reset` in `commands/stage/state.rs`, and `loom stage retry` in `commands/stage/skip_retry.rs` (`stage reset` / `stage retried`);
   - worktree removal in `git/cleanup/batch.rs::drain_spool_before_removal`, or the function that removes the worktree right after it (`worktree removed`).

   A judge session's close does not cancel: a check it requested finishes harmlessly.

## Tests you write

- `tests_store.rs`: id validation; symlinked `verification`, stage dir and target file each refused; atomic save and load round trip; progress cap; pruning keeps three.
- `tests_submit.rs`: queued, joined (requests appended, capped at 16), superseded (old record cancelled with detail), cancel on a terminal record returns `None`.
- `tests_runner.rs` (use `RunnerConfig { program: "/bin/sh", args_prefix: ["-c", "<script>", "sh"], slots }`):
  - a child that writes a passing `result.json` turns `Passed`;
  - one that exits 1 with no result turns `Failed` with the log tail;
  - two queued jobs with `slots: 1` run one after the other;
  - a deadline (a stage whose `completion_line_secs` you shrink through its acceptance list, or a test-only override passed through `RunnerConfig`; prefer the override field only if the computed deadline cannot be made small) turns `Failed` and kills the child;
  - recovery leaves `Queued` alone.

  Each test writes the stage file the runner reads; reuse the stage fixtures other orchestrator tests use (`rg -n 'fn write_stage|fn stage_fixture' src/orchestrator`).

## Proof (run once, from `loom/`)

```bash
cargo test --lib host_verify:: 2>&1 | tail -30
```

The crate builds only once W2 and W3 have returned. If it fails only on their missing files, report that and stop; do not create their files.

## Report

Files changed; where the runner lives and why; each cancellation site with `file:line`; the proof output; anything surprising.
