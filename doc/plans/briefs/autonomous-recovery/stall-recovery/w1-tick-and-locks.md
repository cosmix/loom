# stall-recovery W1: the tick names its operation, and lock waits are bounded

Tier: sonnet. Wave 2, after C1. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `src/orchestrator/stall_contract_tests.rs`.

## Edits (you own these files)

1. `src/process/mod.rs` (`run_bounded`): `let _activity = crate::process::activity::enter(
   <label>)` for the whole wait, where the label is the operation `run_bounded_output` was
   given, or the program name when `run_bounded` is called directly. Thread the operation
   through (an internal `run_bounded_labelled(command, timeout, label)` that both call). Keep
   the file under 400 lines (364 before the live-test-lane plan's confinement-hardening H1 added
   its three environment re-exports; run `wc -l`).
2. `src/orchestrator/tick.rs`
   - The tick file gains a third line, the operation stack joined with ` > `, cut to 256
     bytes on a char boundary, control characters dropped. `pub fn record_operation(work_dir,
     phase: Phase, operations: &[String])` writes all three lines; `record` writes an empty
     third line. `Tick` gains `pub operation: Option<String>` (`None` for an empty line or a
     two-line file). Write with the same best-effort `std::fs::write` as today.
   - `pub fn install_recorder(work_dir: &Path)` installs an activity recorder for the CURRENT
     thread. `record` also stores its phase in a thread-local `Cell<Phase>` in `tick.rs`; the
     recorder rewrites the tick file with `record_operation(work_dir, <that phase>, stack)` on
     every activity change. `pub fn uninstall_recorder()` calls `activity::uninstall`.
3. `src/orchestrator/core/run.rs`: `run_until_stopped` installs the recorder for the
   orchestrator thread before `initialize_run` and uninstalls it on the way out; `run_tick`
   wraps each sync step in `activity::enter(<step name>)` (`check_target_guard`,
   `reconcile_and_update_graph`, `sync_graph_with_stage_files`, `sync_queued_status_to_files`,
   `check_pending_disputes`, `apply_pending_verdicts`, `spawn_merge_resolution_sessions`,
   `drain_stage_spools`, `drain_session_inboxes`, `tick_host_jobs`, `sweep_merged_leftovers`,
   and the spawning steps). The labels are the function names. Keep each function under 50
   lines: a small helper `step(name, f)` is fine. (spawn-recovery added the binary check to
   `run_until_stopped`; keep it.)
4. `src/fs/locking.rs`: `lock_parent_dir` waits at most `LOCK_WAIT_LIMIT` (30 s): try the
   non-blocking lock, sleep with backoff from 10 ms to 200 ms, and fail with
   `"timed out after 30s waiting for the {shared|exclusive} lock on <dir>"`. Expose
   `pub(crate) fn lock_parent_dir_within(path: &Path, exclusive: bool, limit: Duration) ->
   Result<File>` (the contracts use a short limit) and wrap each wait in
   `activity::enter("waiting for the lock on <dir file name>")`.
5. The other blocking locks the orchestrator thread takes: `src/plan/amendment/journal.rs`
   (`PlanVersionsLock::acquire`), `src/fs/safe_fs.rs` (the blocking `flock` near line 685;
   find its callers with `loom map --impact`), `src/fs/memory/storage.rs`,
   `src/fs/jsonl_spool.rs`. Each gets the same bounded wait (reuse one helper in
   `fs/locking.rs`: `pub(crate) fn wait_for_lock(file: &File, exclusive: bool, limit: Duration,
   what: &str) -> Result<()>`) and an activity label. Callers already return `Result`; a
   timeout is an error the tick logs and retries next tick.
6. `src/orchestrator/scheduling_report.rs` (`stalled_alert`): `orchestrator loop stalled: no
   tick for <age>s in <phase>[: <operation>]; loom restarts the daemon after <limit>s` where
   `<limit>` is `crate::orchestrator::watchdog::STALL_RESTART_SECS` (W3's constant). When
   `crate::orchestrator::restart_budget::last_refusal(work_dir)` returns a refusal, the alert
   ends with `; the daemon will not restart again: <refusal>` instead. Drop the "restart with
   `loom stop`, then `loom run`" instruction: the daemon recovers itself.
   Update the existing tests in that file for the new text (list each changed assertion line)
   and add one for the refused-restart ending.

## Traps

- Hook at the primitive: a git command anywhere on the orchestrator thread reaches
  `run_bounded`, so it is named without touching any git call site.
- The bounded lock wait applies to CLI callers too. 30 s is long enough for every legitimate
  holder (each critical section is a short read-modify-write) and turns a hung holder into an
  error instead of a hang.
- The tick file is diagnostic: a failed write never interferes with scheduling (keep it
  best-effort).

## Done

Your one check after the stage compiles: `cargo test --lib orchestrator::tick::`.
