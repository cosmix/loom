# orchestration-recovery / w2-spawn-publication: failed spawns keep their work, holds stop launches, no untracked replacement

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/git.md` section G1; `validation/orchestrator.md` sections R3 and CR-21.

## Findings covered

- G1: `block_failed_spawn` force-removes the worktree and force-deletes the stage branch after any spawn error, destroying a retry's commits, index and untracked files.
- R3: `loom stage hold` taken while a stage is provisioning is ignored; `mark_agent_executing` launches the held stage anyway.
- CR-21: when `insert_active_session` refuses to replace a tracked incumbent, the freshly spawned session runs untracked and the stage stays `Executing`.

## Files

Own (write):
- `loom/src/orchestrator/core/stage_spawn.rs`
- `loom/src/orchestrator/core/session_lifecycle.rs`
- `loom/src/orchestrator/core/session_adoption.rs`
- `loom/src/orchestrator/core/stage_executor.rs`
- `loom/src/orchestrator/core/stage_knowledge.rs`
- `loom/src/orchestrator/core/spawn_publication_tests.rs` (new)

Read only: `loom/src/orchestrator/core/stage_executor_tests.rs` (fixtures to copy; it is 398 lines, do not add to it), `loom/src/orchestrator/core/target_hold_tests.rs` (fixture shapes to copy; its items are private to `target_hold`), `loom/src/orchestrator/monitor/input_wait.rs` (`StaleWait` sentinel pattern), `loom/src/orchestrator/session_registry/identityless.rs`, `loom/tests/test_support/git_repo.rs` (`crate::test_git`).

Frozen (contract tests, never edit): `loom/src/orchestrator/core/spawn_publication_contract_tests.rs`.

## Changes

### 1. G1 (`stage_spawn.rs`)

- In `block_failed_spawn` (lines 238-257) delete the `if spawn_cleanup_allowed(..) { .. }` block (lines 246-254). Remove the `handover` parameter, which nothing else then reads, and make the method `pub(super)`. New signature: `pub(super) fn block_failed_spawn(&mut self, stage_id: &str, session_id: &str, spawn_err: &anyhow::Error)`.
- Rewrite its doc comment (lines 232-237): the stage is marked Blocked instead of the error ending the daemon; a failed spawn keeps its worktree and branch, because a retry reuses a registered worktree as it is (`get_or_create_worktree`) and recreates a branch only when it has no commits ahead (`recreate_or_reuse_branch`).
- Remove the `handover` parameter from `launch_agent` (line 188) and the `let handover = ..` line in `spawn_stage_agent` (line 83), both only used to feed it. Update the call at line 114 and the `block_failed_spawn` call at line 215.
- Delete `spawn_cleanup_allowed` (lines 304-316) and the `native_gate_tests` module (lines 366-379), which tests only that helper. Remove `use crate::git;` if nothing else in the file uses it. Do not touch `crate::commands::stage::native_gate::tests::seed_active_worker`.

### 2. R3 (`session_lifecycle.rs`, `stage_spawn.rs`)

- In `session_lifecycle.rs` add a private sentinel `pub(super) struct HeldAtLaunch;` with `Debug`, `Display` (text `the stage was put on hold before its launch`) and `std::error::Error`, as `StaleWait` is built (`monitor/input_wait.rs:86-96`). `update_stage` writes on `Ok`, so a refusal must be an `Err`.
- Extract the record removal at `session_lifecycle.rs:165-174` from `block_and_undo_session` into `pub(super) fn discard_write_ahead_record(&self, session_id: &str)` (same path, same `NotFound` tolerance, same warning) and call it from `block_and_undo_session`.
- `mark_agent_executing` (`stage_spawn.rs:148-177`): make it `pub(super)`. In the closure, right after computing `continuing`, add `if !continuing && current.held { return Err(super::session_lifecycle::HeldAtLaunch.into()); }`. In the `Err(e)` arm, first handle `e.is::<HeldAtLaunch>()`: call `self.discard_write_ahead_record(session_id)`, insert `BlockReason::Held` into `self.spawn_blocks` for the stage, print nothing else, and `return Ok(None)`. Every other error keeps today's `block_and_undo_session` path. A contract-to-implementation handover (`continuing`) is never refused: it is not a new launch.
- A reused recovery signal stays on disk (do not touch `signals/`), so the next attempt still briefs from it.
- Update the `Returns` line of the doc comment: `Ok(None)` when the stage was blocked or is on hold.
- Knowledge path, `write_ahead_knowledge_session` (`session_lifecycle.rs:99-115`): same check at the top of the closure (the knowledge path is always a fresh attempt), same handling in the `Err` arm (discard, `spawn_blocks` Held, `Ok(None)`). The knowledge baseline written by `knowledge_baseline::capture` before it stays: it is keyed by the discarded session id and nothing reads it.
- `spawn_stage_agent` (line 88) and `launch_knowledge_stage` already return `Deferred` / `Ok(())` on `None`; no change there.

### 3. CR-21 (`session_adoption.rs`, `stage_executor.rs`, `session_lifecycle.rs`, `stage_spawn.rs`, `stage_knowledge.rs`)

`w1-dead-worker-recovery` makes the crash handler drop a dead session's handle on every crash event naming it. This worker adds the pre-spawn check and the post-spawn refusal handling.

- In `session_adoption.rs` add `pub(super) fn tracked_incumbent_cleared(&mut self, stage_id: &str) -> bool`. With no `active_sessions` entry for the stage it returns `true`. Otherwise it probes `self.backend.is_session_alive(&incumbent)`:
  - `Ok(true)` or `Err(_)`: insert `BlockReason::OwnershipUncertain { detail }` into `spawn_blocks`, with `detail` = `tracked session '<id>' may still be running; it must end before a replacement starts` (or the probe error text appended for `Err`), and return `false`;
  - `Ok(false)` when `crate::orchestrator::session_registry::identityless_record_is_dead(&self.config.work_dir, &incumbent, Admission::Held, Utc::now())` holds: remove the entry, log `tracing::warn!` naming the stage and the incumbent, return `true`;
  - `Ok(false)` otherwise (a launch younger than the grace): the same `OwnershipUncertain` deferral, return `false`.
  The caller holds the stage's admission lock (`start_stage`), which is what makes `Admission::Held` correct.
- In `start_stage` (`stage_executor.rs:162-212`), immediately before `if stage.stage_type == StageType::Knowledge`, add `if !self.tracked_incumbent_cleared(stage_id) { return Ok(StartDisposition::Deferred); }`. This runs after both adoption points, so a live incumbent the adoption pass found is already handled there.
- In `session_lifecycle.rs` add `pub(super) fn track_spawned_session(&mut self, stage_id: &str, session: Session) -> bool`: read the incumbent id first, call `insert_active_session`; on `false`, call `self.block_stranded_stage(stage_id, format!("session '{new}' was spawned for stage '{stage_id}' while session '{incumbent}' is still tracked; stop the one that should not run, then retry the stage"))` and return `false`.
- `record_spawned_agent` (`stage_spawn.rs:299-300`): call `track_spawned_session`; insert into `active_worktrees` only when it returns `true`. `stage_knowledge.rs:57`: call `track_spawned_session` instead of `insert_active_session`. Leave `session_adoption.rs:112` on `insert_active_session`: adoption already handles a refusal.

Declare the new test file at the end of `stage_spawn.rs`: `#[cfg(test)] #[path = "spawn_publication_tests.rs"] mod spawn_publication_tests;`.

## Interfaces shared with other workers

- Pinned for the frozen contract file: `pub(super) fn block_failed_spawn(&mut self, stage_id: &str, session_id: &str, spawn_err: &anyhow::Error)` and `pub(super) fn mark_agent_executing(&mut self, stage_id: &str, session_id: &str) -> Result<Option<Stage>>`, both in `stage_spawn.rs`.
- `w1-dead-worker-recovery` drops a crashed session's handle in `crash_handler.rs`; nothing here calls into it.

## Tests

All in `spawn_publication_tests.rs` (a child of `stage_spawn`, so private items are visible). Build the work dir as `stage_executor_tests.rs:22-44` does (`write_terminal_config` with `SessionBackendKind::Tmux`, `OrchestratorConfig`). For the git cases build the repository as `target_hold_tests.rs:41-55` and `:77-91` do, from `crate::test_git::{TestRepo, git, commit_file}`, with `base_branch: Some("main")`, and create the worktree with `crate::git::get_or_create_worktree("s", root, Some("main"), None)`.

- `a_failed_spawn_keeps_the_branch_commits_index_and_untracked_files`: commit `work.txt` inside the worktree, then stage a change to `seed.txt` and write `notes.txt` untracked; record the `loom/s` tip and `git diff --cached` output; save a stage `s` `Executing` under `sess`; call `block_failed_spawn("s", "sess", &anyhow::anyhow!("boom"))`. Assert the worktree directory exists, the tip and the cached diff are unchanged, `notes.txt` holds its bytes, and the stage is `Blocked`. Today the worktree and branch are gone.
- `a_hold_taken_after_the_queue_check_prevents_the_launch`: `Queued` stage with `held = true`, a saved write-ahead record; `mark_agent_executing` returns `Ok(None)`; the stage is still `Queued`, `held`, `session == None`; the record file is gone; `spawn_blocks` holds `BlockReason::Held`. Today it returns the stage `Executing`.
- `a_handover_is_not_refused_by_a_hold`: an `Executing` stage with `held = true` and a saved record: `mark_agent_executing` returns `Some` and assigns the session.
- `a_held_knowledge_stage_is_not_launched`: a `Knowledge` stage, `Queued`, `held = true`, in a repository with one commit (the baseline capture needs `HEAD`); `write_ahead_knowledge_session` returns `Ok(None)`, the stage keeps no session and no record file remains.
- `a_live_tracked_incumbent_defers_the_spawn`: track an incumbent whose PID identity names this test process (`write_test_pid_identity`, as `spawn_a_live_agent` does) for a `Queued` stage; `tracked_incumbent_cleared` returns `false`, the handle stays, `spawn_blocks` holds `OwnershipUncertain`.
- `a_dead_aged_incumbent_is_dropped_before_the_spawn`: incumbent with no PID file and `created_at` ten minutes ago; `tracked_incumbent_cleared` returns `true` and the entry is gone. Today nothing removes it and `insert_active_session` decides after the spawn.
- `a_refused_insert_blocks_the_stage`: a live tracked incumbent, then `track_spawned_session` with a second session returns `false`, the stage is `Blocked`, its failure evidence names both ids.

Contracts to turn green: `failed-spawn-keeps-worktree-work`, `hold-during-provisioning-stops-launch` (file `loom/src/orchestrator/core/spawn_publication_contract_tests.rs`).

## Proof

From `loom/`: `cargo test --lib orchestrator::core::`.

## Traps

- Keeping the cleanup "only when this launch created the worktree" is the rejected alternative (G1 evidence): `find_prior_stage_work` fails open on a git error. Delete the cleanup outright.
- Checking `held` again in `start_stage` before provisioning is the check that already exists; the refusal has to sit inside the locked `update_stage` closure that marks the stage `Executing`.
- Returning `Err` from `mark_agent_executing` for a hold would block the stage through the caller; the hold must defer (`Ok(None)`), never block.
- `tracked_incumbent_cleared` must run while `start_stage` holds the admission lock; calling it from `start_ready_stages` would make `Admission::Held` a false claim.
- Do not edit `stage_executor_tests.rs`; its two `insert_active_session_*` tests must keep passing unchanged.
