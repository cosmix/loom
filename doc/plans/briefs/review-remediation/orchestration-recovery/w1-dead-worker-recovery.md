# orchestration-recovery / w1-dead-worker-recovery: dead waiters, never-launched records, retry isolation

Tier: opus
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/orchestrator.md`, sections R1, R2, CR-21 (overlap note), CR-30 (residual only); `validation/security.md` section CR-01 (the crash-handler call site only).

## Findings covered

- R1: a worker that dies while its stage is `WaitingForInput` is ignored by every crash path, so the stage never leaves the wait and its in-memory handle holds a scheduler slot until restart.
- R2: a `Spawning` record whose launch never reached a backend is judged only once, at startup, inside the 30 s grace; nothing re-checks it, so the stage stays `Executing` with no process.
- CR-30 residual: one Blocked stage whose retry write fails makes `sync_authoritative_stages` return `Err`, which ends the daemon.
- CR-01 call site: `sync_crashed_session_permissions` calls a function `w6-permission-foldback` removes.

## Files

Own (write):
- `loom/src/orchestrator/core/crash_handler.rs`
- `loom/src/orchestrator/core/crash_outcome.rs` (new)
- `loom/src/orchestrator/core/crash_handler_identity_tests.rs` (only the two-line child-module declaration below)
- `loom/src/orchestrator/core/crash_wait_tests.rs` (new)
- `loom/src/orchestrator/monitor/crash_redelivery.rs`
- `loom/src/orchestrator/core/recovery_sessions.rs`
- `loom/src/orchestrator/core/recovery_sync.rs`
- `loom/src/orchestrator/core/recovery_sync_tests.rs` (only the two-line child-module declaration below)
- `loom/src/orchestrator/core/recovery_orphan_tests.rs` (new)
- `loom/src/orchestrator/core/coherence.rs`
- `loom/src/orchestrator/core/orchestrator.rs`
- `loom/src/orchestrator/core/event_handler/contract_phase.rs`
- `loom/src/orchestrator/core/event_handler/contract_phase_tests.rs` (only the two-line child-module declaration below)
- `loom/src/orchestrator/core/event_handler/contract_phase_wait_tests.rs` (new)

Read only: `loom/src/verify/contracts/refusal.rs` (`end_wait`), `loom/src/orchestrator/session_registry/identityless.rs`, `loom/src/orchestrator/core/orphan_adoption.rs`, `loom/src/orchestrator/core/recovery_guards.rs`, `loom/src/fs/stage_admission.rs`, `loom/src/orchestrator/terminal/native/pid_tracking.rs`.

Frozen (contract tests, never edit): `loom/src/orchestrator/core/dead_worker_contract_tests.rs`.

## Changes

### 1. R1, crash path (`crash_handler.rs`)

`crash_handler.rs` is 397 lines. First move the three free functions `maybe_disable_remote_control`, `note_remote_control_disabled` and `announce_crash_outcome` (lines 324-389) unchanged into the new `crash_outcome.rs`, as `pub(super) fn`. Declare it in `crash_handler.rs`, after the `use` block: `#[path = "crash_outcome.rs"] mod crash_outcome;` and `use crash_outcome::{announce_crash_outcome, maybe_disable_remote_control, note_remote_control_disabled};`. Move only the imports those functions need.

Then:
- `crash_applicable` (line 25): accept `matches!(stage.status, StageStatus::Executing | StageStatus::WaitingForInput)`. Keep the `matches_attempt` check first.
- `persist_blocked_crash` (lines 283-321): inside the `update_stage` closure, after the `crash_applicable` check and before `current.accumulate_attempt_time`, add `if current.status == StageStatus::WaitingForInput { crate::verify::contracts::refusal::end_wait(current)?; }`. `WaitingForInput -> Blocked` is not a transition edge (`models/stage/transitions.rs:47` allows only `-> Executing`), so the wait must end in the same locked update or `try_mark_blocked` fails and the crash stays a `RetryableFailure` forever.
- Handle eviction. The in-memory handle is dropped today only on `CrashTransition::Applied`. This worker owns that mechanism for the whole stage (CR-21 relies on it): also call `self.take_matching_active_session(&sid, session_id)` in the not-answerable branch of `handle_session_crashed` (lines 190-193) and in the `CrashTransition::Stale` arm (line 232). `take_matching_active_session` already compares ids, so a crash from an older session never removes a successor's handle. Do not drop the handle on `CrashTransition::RetryableFailure` or when `stage_answerable_for_crash` returns `Err`: those stay retryable (test `failed_locked_crash_read_keeps_event_and_handle_for_one_retry` pins it).
- CR-01 call site: rewrite `sync_crashed_session_permissions` as `fn sync_crashed_session_permissions(&self, sid: &str)`. Its body calls `crate::fs::permissions::fold_back_approvals(&self.config.repo_root)` (signature below) and, when the returned count is above zero, prints `Recorded {n} approved permission(s) after the crash of stage '{sid}'` with `eprintln!`. Drop the worktree-exists check and the `working_dir_path` line: the fold-back no longer reads the worktree. Update the caller at line 225 to `self.sync_crashed_session_permissions(sid);` and the arm to `CrashTransition::Applied(_)` if `updated` is then unused.

### 2. R1, redelivery (`monitor/crash_redelivery.rs`)

In `redeliver_crash` (lines 58-96), `still_owed` accepts `matches!(stage.status, StageStatus::Executing | StageStatus::WaitingForInput)` with the same `stage.session == session.id` check. Update the doc comment's "while its stage still runs under it" to name both statuses.

### 3. R1, contract writer (`event_handler/contract_phase.rs`)

- `current_contract_session` (lines 225-250): `assigned` accepts `Executing | WaitingForInput`. Update its doc comment.
- `on_contract_session_ended` (lines 193-219): before `self.spawn_on_stage_worktree(stage, AgentKind::Contract)`, when `stage.status == StageStatus::WaitingForInput`, replace `stage` with the result of `self.update_stage(stage_id, |current| { if current.status == StageStatus::WaitingForInput && current.session.as_deref() == Some(session_id) { end_wait(current) } else { Ok(()) } })?`. `spawn_on_stage_worktree` reloads and requires `Executing`.
- `escalate_contract_phase` (lines 283-293): the guard accepts `Executing | WaitingForInput`; inside the closure call `end_wait(stage)?` when the status is `WaitingForInput`, then `request_contract_review(stage)`.
- Declare the new test file at the end of `contract_phase_tests.rs`: `#[path = "contract_phase_wait_tests.rs"] mod wait_tests;` (a child module, so it reaches the private fixtures there through `super::`).

### 4. R1 and R2, startup recovery (`recovery_sessions.rs`)

- Add to `Orchestrator` (`orchestrator.rs`, after `spawn_blocks`): `pub(super) deferred_orphans: HashMap<String, String>` (stage id to session id), with a doc comment: current records found dead but too young to judge at startup, re-judged each scheduling pass; in memory only. Initialise it in `Orchestrator::new`.
- `recover_current_sessions` (lines 17-59): after the merge branch, `continue` unless `session_is_current_for_stage(stage, &session)`. Then take `crate::fs::stage_admission::lock_for_daemon(&self.config.work_dir, &stage.id)`; on `Err`, log `tracing::warn!` and insert `(stage.id, session.id)` into `deferred_orphans`, then `continue`. Hold the guard through `current_session_is_orphan` and `retire_current_orphan`. Extract the record and signal removal at lines 49-55 into `fn discard_retired_session(&self, session_id: &str)` and call it here and from step 5.
- `current_session_is_orphan` (lines 61-78): the `Ok(false)` arm becomes: when `crate::orchestrator::session_registry::identityless_record_is_dead(&self.config.work_dir, session, Admission::Held, Utc::now())` is true, the record is an orphan; otherwise insert it into `deferred_orphans` and return `false`. This replaces the `recovery_guards::skip_too_young_orphan` call. `skip_too_young_orphan` keeps its test caller in `recovery_guard_tests.rs`; if it has no non-test caller left, mark it `#[cfg(test)]` rather than deleting it.
- `retire_current_orphan` (lines 80-124): add `StageStatus::WaitingForInput` to the first match arm and call `crate::verify::contracts::refusal::end_wait(stage)?` before `recover_orphaned_stage` when the status is `WaitingForInput`, so `route_orphaned_stage` never takes its `force_status_with_reason` path.

### 5. R2, the per-tick re-check (`recovery_sessions.rs`, `coherence.rs`)

Add `pub(super) fn recover_deferred_orphans(&mut self)` to `recovery_sessions.rs`. For each `(stage_id, session_id)` of a clone of `deferred_orphans`:
1. `lock_for_daemon`; on `Err` keep the entry and go on.
2. `crate::fs::stage_files::find_stage_file(&work_dir.join("stages"), &stage_id)` and `super::recovery::load_stage_at_path`; `crate::fs::session_files::load_session_exact(&work_dir, &session_id)`. A missing stage file or record drops the entry; a read error keeps it.
3. Drop the entry when `!session_is_current_for_stage(&stage, &session)`.
4. `self.liveness.is_alive(&session)`: `Ok(true)` registers it exactly as `current_session_is_orphan` does (`settled_live_session` then `register_live_current_session`) and drops the entry; `Err` keeps it; `Ok(false)` with `identityless_record_is_dead(.., Admission::Held, Utc::now())` calls `retire_current_orphan(&path, &session)`: `Ok(true)` calls `discard_retired_session` and drops the entry, `Ok(false)` drops it, `Err` logs and keeps it. `Ok(false)` while not yet dead keeps it.

Keep every function under 50 lines; split the per-entry body into `fn recover_deferred_orphan(&mut self, stage_id: &str, session_id: &str) -> bool` (true = drop the entry). In `begin_scheduling_pass` (`coherence.rs:39`), call `self.recover_deferred_orphans();` first, before `reconcile_executing_stages`.

### 6. CR-30 residual (`recovery_sync.rs`)

Change `apply_retry_if_due` (line 66) to take `stage: &Stage` and return `Result<Stage>` (`Ok(stage.clone())` when not eligible). In `sync_authoritative_stages` (lines 23-25), match its result: `Ok(updated) => stage = updated`, `Err(error) => tracing::warn!(stage_id = %stage.id, error = %format!("{error:#}"), "Cannot apply a due retry; keeping the stage Blocked until the next pass")`, leaving `stage` as loaded.

## Interfaces shared with other workers

- Uses, from `w6-permission-foldback` (`loom/src/fs/permissions/sync.rs`, re-exported from `crate::fs::permissions`): `pub fn fold_back_approvals(main_repo_path: &Path) -> usize`. It returns how many rules were newly recorded and never fails. If it does not exist yet when you compile, w6 has not finished: finish everything else, then compile again. Never add it yourself.
- Provides `pub(super) fn recover_deferred_orphans(&mut self)` and the field `pub(super) deferred_orphans: HashMap<String, String>` on `Orchestrator`, both called by the frozen contract file.
- `w2-spawn-publication` relies on step 1's handle eviction; it adds a pre-spawn incumbent check of its own in other files.

## Tests

- `crash_wait_tests.rs` (declared at the end of `crash_handler_identity_tests.rs` as `#[path = "crash_wait_tests.rs"] mod wait_tests;`, with `#[serial]` on every test since `orchestrator_for` sets `LOOM_TERMINAL`). Use `super::orchestrator_for` (`crash_handler_identity_tests.rs:49`). Tests:
  - `a_dead_waiter_is_blocked_and_its_handle_dropped`: save a `WaitingForInput` stage under `session-dead`, insert a handle with that id and `created_at` ten minutes ago, call `handle_session_crashed`; assert `Blocked`, `retry_count == 1`, no handle. Today: still `WaitingForInput`, handle kept.
  - `a_stale_crash_leaves_a_live_waiter_alone`: stage `WaitingForInput` under `session-live`, handle `session-live`, crash event from `session-old`; assert status, session and handle unchanged.
  - `a_stale_crash_drops_only_its_own_handle`: stage `Completed` naming `session-x`, handle `session-x`, crash from `session-x`; assert status `Completed` and the handle gone. Today the handle stays.
- `monitor/crash_redelivery.rs` tests module: `a_crash_under_a_waiting_stage_is_still_owed`, built from `with_owed_crash` and `stage_under_session` with the status set to `WaitingForInput`; assert one `SessionCrashed` event.
- `contract_phase_wait_tests.rs`: `a_dead_contract_writer_of_a_waiting_stage_is_replaced`, the pattern of `contract_session_exit_without_freeze_respawns` (`contract_phase_tests.rs:317`, `HooksDirGuard`, `handoff_work_dir`, `manual_orchestrator`, `v2_stage(StageStatus::WaitingForInput, ..)`, `track_worktree`): one `on_contract_session_ended` leaves the stage `Executing` under a new `Contract` session. Today it is dropped.
- `recovery_orphan_tests.rs` (declared at the end of `recovery_sync_tests.rs` as `#[path = "recovery_orphan_tests.rs"] mod orphan_tests;`; uses `super::{resume_orchestrator, restart_stage, restart_session, minimal_stage_definition}` and `LivenessService::fixed_for_tests(false)`; mark the graph node queued then executing first, as `sync_leaves_an_already_executing_node_executing_without_erroring` does):
  - `a_never_launched_record_is_retired_at_startup`: young `Spawning` record, no wrapper script; after `recover_current_sessions` the stage is not `Executing` and names no session. Today it stays `Executing`.
  - `a_young_launched_record_is_retired_on_a_later_pass`: write the wrapper at `wrapper_script_path(work, &pid_key)` (`NativeBackend::window_title_and_pid_key`); startup leaves the stage `Executing` and records it in `deferred_orphans`; backdate `created_at` by 120 s (save the record again) and the wrapper mtime (`File::set_modified`, pattern at `codex_lifecycle/foreground_tests.rs:211`); `begin_scheduling_pass` retires it and empties `deferred_orphans`. Mark it `#[serial]`: `begin_scheduling_pass` strips process environment variables.
  - `a_dead_waiter_is_recovered_at_startup`: `WaitingForInput` stage, record `created_at` ten minutes ago, no wrapper; after `recover_current_sessions` the stage is not `WaitingForInput` and names no session.
  - `a_failed_retry_write_does_not_end_the_pass`: a `Blocked` stage with `failure_info` of `FailureType::SessionCrash`, `retry_count` 0 and `last_failure_at` an hour ago (see `is_retry_pending`, `recovery.rs:105`); make the stages directory `0o555` after one successful `update_stage` so a lock file already exists; `sync_graph_with_stage_files()` returns `Ok`; restore `0o755` before asserting. Skip when `libc::geteuid() == 0` (pattern `tokens/tracker_tests.rs:363`). If the snapshot read itself fails under `0o555`, stop and report instead of choosing another fault.
- Contracts to turn green: `dead-waiter-keeps-no-slot`, `young-spawning-record-retired-later` (file `loom/src/orchestrator/core/dead_worker_contract_tests.rs`).

## Proof

From `loom/`: `cargo test --lib orchestrator::core::` and `cargo test --lib orchestrator::monitor::`.

## Traps

- Widening `crash_applicable` without `end_wait` compiles and passes a test that only checks the handle; the stage stays `WaitingForInput` because `try_mark_blocked` errors inside the closure.
- Taking the admission lock for every record in `sessions/`, historical ones included, makes startup slow; lock only after `session_is_current_for_stage`.
- `flock` locks belong to the open file description: never call `lock_for_daemon` for a stage while this thread already holds its guard (no nested call from `recover_deferred_orphan` into a function that locks again).
- `retire_current_orphan` calls `self.graph.mark_status(..)?`; in tests the node must exist and be `Executing`, or the error hides the behaviour under test.
- A record with a PID file that the backend reports alive must be adopted, never retired: keep `Ok(true)` first.
