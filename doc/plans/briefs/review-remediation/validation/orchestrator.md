# Validation: init, scheduling, recovery, adjudication, signals, root resolution

Revision `62be2a3d2`. Paths are relative to the repository root; `loom/src/` is the crate source.

## R1 — A worker that dies in `WaitingForInput` keeps its scheduler slot

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/orchestrator/core/crash_handler.rs:25-28` `crash_applicable` requires `StageStatus::Executing`; `handle_session_crashed` (171-196) inserts the session into `reported_crashes` and returns at 190-193 when the stage is not answerable, so `take_matching_active_session` (222, reached only on `CrashTransition::Applied`) never runs. `persist_blocked_crash` (293-308) re-checks the same predicate under the lock.
- `loom/src/orchestrator/monitor/crash_redelivery.rs:70-81` drops the owed crash unless the stage is `Executing`.
- `loom/src/orchestrator/core/recovery_sessions.rs:96-114` startup retirement acts only on `Executing | NeedsHandoff | Blocked` (and merge states); `WaitingForInput` falls to `_ => false`.
- `loom/src/orchestrator/core/coherence.rs:85` and `loom/src/orchestrator/coherence.rs:68-71` (per-tick watchdog) skip non-`Executing` stages; `loom/src/orchestrator/monitor/input_wait.rs:45-79` resumes a wait only on main-agent tool progress, which a dead agent never makes.
- `loom/src/orchestrator/core/stage_executor.rs:124` capacity is `active_sessions.len()`, so the dead waiter's handle counts against `max_parallel_sessions` until restart. After restart the handle is gone but the stage stays `WaitingForInput` forever.
- Contract writer: `loom/src/orchestrator/monitor/session_events.rs:266-291` emits `ContractSessionEnded` for any stage naming the session, but `loom/src/orchestrator/core/event_handler/contract_phase.rs:225-250` (`current_contract_session`) drops it unless the stage is `Executing`. `models/stage/transitions.rs:47` allows `WaitingForInput -> Executing` only, so a direct `try_mark_blocked` from a wait would fail too.
No guard defeats the claim.

**Fix seam:**
- `crash_applicable` (`crash_handler.rs:25`): accept `Executing | WaitingForInput`.
- `persist_blocked_crash` (`crash_handler.rs:293`): inside the closure, when `current.status == WaitingForInput` call `crate::verify::contracts::refusal::end_wait(current)?` (`verify/contracts/refusal.rs:196`, `fn end_wait(stage: &mut Stage) -> Result<()>`) before `try_mark_blocked()`, in the same locked update.
- `redeliver_crash` (`crash_redelivery.rs:70-77`): treat `WaitingForInput` like `Executing` for `still_owed`.
- `retire_current_orphan` (`recovery_sessions.rs:96`): add `WaitingForInput` to the first arm and call `end_wait(stage)?` before `recover_orphaned_stage` (`orphan_adoption.rs:60`), so `route_orphaned_stage` (`orphan_adoption.rs:111`) does not take its `force_status_with_reason` path.
- `current_contract_session` (`contract_phase.rs:231-232`) and `escalate_contract_phase` (`contract_phase.rs:284-289`): accept `WaitingForInput`; end the wait (same helper) in the update that precedes `spawn_on_stage_worktree`, which expects an `Executing` handover (`stage_spawn.rs:83`).
Identity stays the authority: every path keeps `matches_attempt` / `stage.session == session_id`, so a stale death event from an earlier assignment is still ignored. Consumers: `handle_session_crashed`, `Detection::redeliver_crash`, `recover_current_sessions`, `handle_contract_session_ended`.

**Regression test:** extend `loom/src/orchestrator/core/crash_handler_identity_tests.rs` with `executing_stage` (line 40) and `orchestrator_for` (line 49): set the stage to `WaitingForInput` under its active session, insert the session into `active_sessions`, call `handle_session_crashed`; assert the stage is `Blocked`, `active_sessions` is empty, and a second event from a different session id leaves a live waiter untouched. Today the stage stays `WaitingForInput` and the handle stays. Startup half: `loom/src/orchestrator/core/recovery_sync_tests.rs` (`resume_orchestrator` line 72, `restart_stage`/`restart_session` lines 282/291, `LivenessService::fixed_for_tests(false)`): a `WaitingForInput` stage with an aged dead current session must leave `WaitingForInput` after `recover_current_sessions`. Contract half: `loom/src/orchestrator/core/event_handler/contract_phase_tests.rs`.
**Files a fix touches:** `loom/src/orchestrator/core/crash_handler.rs`, `loom/src/orchestrator/monitor/crash_redelivery.rs`, `loom/src/orchestrator/core/recovery_sessions.rs`, `loom/src/orchestrator/core/event_handler/contract_phase.rs`, `loom/src/orchestrator/core/crash_handler_identity_tests.rs`, `loom/src/orchestrator/core/recovery_sync_tests.rs`, `loom/src/orchestrator/core/event_handler/contract_phase_tests.rs`.
**Size / tier:** M; judgment (concurrency).
**Overlap:** CR-21 (same handle-retention mechanism: `take_matching_active_session` runs only on `Applied`, so a crash on any non-applicable status keeps the in-memory handle).

## R2 — A quick restart strands a stage whose worker never launched

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/orchestrator/core/stage_spawn.rs:85-114` writes the `Spawning` record, marks the stage `Executing` (`mark_agent_executing`, 148-177), then launches.
- `loom/src/orchestrator/core/run.rs:71-73` runs `recover_orphaned_sessions` once, at startup only. `loom/src/orchestrator/core/recovery_sessions.rs:65-71` treats `Ok(false)` within the grace (`recovery_guards::skip_too_young_orphan`, `recovery_guards.rs:108`) as "not an orphan" and nothing remembers it.
- `loom/src/orchestrator/session_registry/identityless.rs:35` grace is 30 s; `identityless.rs:73-88` already has the `Admission::Held` rule ("a `Spawning` record with no wrapper script never reached a backend"), but the startup pass calls `self.liveness.is_alive` (`orchestrator/liveness.rs:51`), which never consults it and holds no admission lock.
- Per tick: `loom/src/orchestrator/coherence.rs:68-99` only checks that the record exists and matches, not liveness. `loom/src/orchestrator/monitor/session_events.rs:157-163` runs the vanished-process check only for `Running -> Running`; a `Spawning` record never transitions, so only `redeliver_crash` runs (165-166), which owes nothing.
- `loom/src/orchestrator/core/recovery_sync_tests.rs:358` covers only a live process left `Spawning`.

**Fix seam:**
1. In `current_session_is_orphan` (`recovery_sessions.rs:61`), take `crate::fs::stage_admission::lock_for_daemon(&work_dir, stage_id)` (as `start_stage` does, `stage_executor.rs:166-167`); with it held, a dead-probed record for which `identityless_record_is_dead(work_dir, session, Admission::Held, now)` (`identityless.rs:73`) holds is an orphan now. A busy lock defers.
2. A record still too young (wrapper written, launch may be in flight) goes into a new `Orchestrator` field `deferred_orphans: HashMap<String, String>` (stage id to session id). `begin_scheduling_pass` (`orchestrator/core/coherence.rs:39`) re-checks each entry under the admission lock once `launch_too_young_to_judge` (`identityless.rs:46`) is false, then calls the existing `retire_current_orphan` (`recovery_sessions.rs:80`), or drops the entry when the stage no longer names that session or the probe finds it alive (then `register_live_current_session`, `orphan_adoption.rs:32`).
Choose both: (1) alone misses a launch that wrote its wrapper and died before the backend ran; (2) alone keeps the 30 s delay for the common never-launched case. Consumers: `initialize_run`, `start_ready_stages`.
**Regression test:** `loom/src/orchestrator/core/recovery_sync_tests.rs` (`resume_orchestrator`, `restart_stage`, `restart_session` which is `Spawning`, `LivenessService::fixed_for_tests(false)`): a young `Spawning` record with no wrapper script must be retired by `recover_current_sessions` (stage leaves `Executing`); a young record with a wrapper script whose `created_at` is then moved past the grace must be retired on the next `begin_scheduling_pass`; a live one (as at line 358) must be adopted exactly once. Today the stage stays `Executing` with no process after both calls. A fake clock is not available in this module; backdate `session.created_at` and the wrapper mtime instead (`wrapper_script_path`, `orchestrator/terminal/native`).
**Files a fix touches:** `loom/src/orchestrator/core/recovery_sessions.rs`, `loom/src/orchestrator/core/coherence.rs`, `loom/src/orchestrator/core/mod.rs` (new field), `loom/src/orchestrator/core/recovery_sync_tests.rs`.
**Size / tier:** M; judgment (concurrency).
**Overlap:** CR-21 (identityless judgment), R1 (startup orphan pass).

## R3 — A successful hold can be followed by a launch

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/orchestrator/core/stage_executor.rs:179-183` reads `held` from the stage loaded at 168; worktree creation, before-stage gates and provisioning follow (`stage_executor.rs:23-44`, `provision_gate.rs:42-72`), while the stage is still `Queued`.
- `loom/src/commands/stage/state.rs:297-317` `hold` takes only the stage-record lock (`update_stage`), not the admission lock, so it succeeds during that window.
- `loom/src/orchestrator/core/stage_spawn.rs:148-157` `mark_agent_executing` calls `try_mark_executing` and `assign_session` with no `held` check; `models/stage/methods.rs:240-247` checks nothing either. Knowledge path: `loom/src/orchestrator/core/session_lifecycle.rs:102-107` is the same.

**Fix seam:** in the `mark_agent_executing` closure (`stage_spawn.rs:149`) and the `write_ahead_knowledge_session` closure (`session_lifecycle.rs:102`), when the update is a fresh attempt (`!continuing`; the knowledge path always) and `current.held`, return a private sentinel error (pattern: `StaleWait` in `orchestrator/monitor/input_wait.rs:87-96`, because `update_stage` writes on `Ok`). On that sentinel: delete the write-ahead record (extract the file removal at `session_lifecycle.rs:165-174` from `block_and_undo_session` into a shared `discard_write_ahead_record(session_id)` and call it from both), keep any reused recovery signal on disk so the next attempt still briefs from it, record `spawn_blocks.insert(stage_id, BlockReason::Held)`, and return `Ok(None)` so the caller returns `StartDisposition::Deferred` without blocking the stage. A contract-to-implementation handover (`continuing == true`) is not a new launch and is not gated. Consumers: `spawn_stage_agent` (`stage_spawn.rs:88`), `launch_knowledge_stage`.
**Regression test:** `loom/src/orchestrator/core/stage_executor_tests.rs` (`work_dir` line 22, `orchestrator_for` line 36, `stage_at` line 46): save a `Queued` stage with `held = true`, write a session record, call `mark_agent_executing`; assert `Ok(None)`, stage still `Queued` with `held` and no `session`, record file gone. Today it returns the stage `Executing` with the session assigned. A provisioning pause harness does not exist; this direct call proves the locked decision without one.
**Files a fix touches:** `loom/src/orchestrator/core/stage_spawn.rs`, `loom/src/orchestrator/core/session_lifecycle.rs`, `loom/src/orchestrator/core/stage_executor_tests.rs`.
**Size / tier:** M; judgment (concurrency).
**Overlap:** none in this group.

## R4 — `loom init <plan> --clean` destroys the old run before two deterministic failures

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/commands/init/execute.rs:82-94`: `preflight_plan_if_given` (parse + sandbox checks, `plan_setup.rs:46-94`) runs before `prepare_workspace`, whose `cleanup_previous_run` (`execute.rs:132`, `190-212`) deletes the state directory and `.worktrees`.
- Cycles: `loom/src/plan/schema/validation.rs:364-389` checks unknown and self dependencies only; no cycle check anywhere in `parse_plan` (`plan/parser/mod.rs:32-62`, `plan/parser/validation.rs:60-70`). Init orders stages with the cycle-tolerant `plan/graph/levels.rs:33-36`. The first `ExecutionGraph::build` (`plan/graph/build.rs:41`, `cycle::detect_cycles`) runs at `loom run` (`plan/graph/loader.rs:69/102`), after the old run is gone.
- Detached HEAD: `loom/src/git/repository.rs:51-62` (`ensure_pinned_repo_ready`) checks identity, non-bare and commits, not the branch; `checked_out_branch` (`commands/init/base_branch.rs:12-21`) runs only at `plan_setup.rs:174`, inside `initialize_with_plan`, after cleanup. `InitGuard` (`execute.rs:47-58`) removes only the new directory.

**Fix seam:**
- Cycles: at the end of `validate` (`plan/schema/validation.rs:603`, before the `errors.is_empty()` return), when `errors` is still empty, call `crate::plan::graph::ExecutionGraph::build(metadata.loom.stages.clone())` and push its error as a `ValidationError { stage_id: None }`. Every `parse_plan` caller (init preflight, run loader, `loom plan verify`, planner) then refuses a cycle; `commands/plan/verify/checks.rs:77` already uses the same call. `validation.rs` is 1063 lines, so put the call in a small `validation/dependency_cycles.rs` submodule.
- Branch: give `prepare_workspace` (`execute.rs:107`) a `needs_branch: bool` and return `Result<Option<String>>`; call `checked_out_branch(repo_root)` right after `ensure_pinned_repo_ready` / `ensure_repo_ready_for_worktrees` (which may create the first commit, preserving first-time bootstrap) and before `cleanup_previous_run`. Change `initialize_with_plan` (`plan_setup.rs:117`) to take `base_branch: &str` and drop line 174. Callers: `execute.rs:94`, `commands/init/tests.rs:58`.
Other `initialize_with_plan` failures after cleanup (config write, snapshot write) still cannot restore the old run; those are I/O faults, not deterministic refusals, and stay out of scope.
**Regression test:** `loom/src/commands/init/execute/tests.rs` (`CwdGuard` line 13, `held_repo` from `git/target_guard/test_support`, pattern of `init_clean_refuses_through_execute_and_deletes_nothing` line 41): with an existing run (a state file and `.worktrees/stage-a/keep.txt`), `execute(Some(cyclic_plan), true, None, true)` and a detached-HEAD `execute(Some(valid_plan), true, None, true)` must both fail and leave both files in place. Today both files are deleted. Unit half: `loom/src/plan/schema/tests/validation_tests.rs`, a two-stage `a -> b -> a` plan must fail `validate`.
**Files a fix touches:** `loom/src/plan/schema/validation.rs`, `loom/src/plan/schema/validation/dependency_cycles.rs` (new), `loom/src/commands/init/execute.rs`, `loom/src/commands/init/plan_setup.rs`, `loom/src/commands/init/tests.rs`, `loom/src/commands/init/execute/tests.rs`, `loom/src/plan/schema/tests/validation_tests.rs`.
**Size / tier:** M; needs-exploration (existing tests may parse cyclic fixtures and expect success).
**Overlap:** CR-43 (the cycle half is the same fix; one change in `validate` covers both), CR-17 (`execute` resolves `repo_root` from cwd).

## R5 — Bounded scans can permanently exclude active Codex jobs

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/orchestrator/monitor/core.rs:150-151, 223-256` passes every session record in `read_dir` order (historical records accumulate) to `reconcile_codex_evidence` (`core.rs:41`).
- `loom/src/codex_lifecycle/reconcile.rs:131-134` applies `.take(MAX_ACTIVE_SESSIONS)` (256) before `.filter(!is_terminal)`; `reconcile.rs:43` takes the first 64 entries of a `BTreeMap` sorted by stage id with no rotation.
- `loom/src/commands/subagents/wait/evidence.rs:158-161` answers a Direct job from `index.outcome(identity)` only; the Direct lifecycle records are appended by `reconcile_one` (`reconcile.rs:183-200`), so an excluded stage's completed job never reaches the index and the wait times out.
The 256 bound is the reachable trigger (record files never shrink); more than 64 concurrently active stages is uncommon but unbounded.

**Fix seam:** in `active_stage_sessions` (`reconcile.rs:129`), filter `!session.status.is_terminal()` before `.take(MAX_ACTIVE_SESSIONS)`. For the stage bound, add a `codex_cursor: usize` to the monitor struct (`orchestrator/monitor/core.rs`), pass it through `reconcile_codex_jobs` (`codex_lifecycle.rs:261`) and `reconcile_with_state_root` to `reconcile_at` (`reconcile.rs:35`), which starts at `cursor % active.len()`, wraps, takes 64, and returns the next cursor in `ReconcileReport`. Every eligible stage is then visited within `ceil(n / 64)` polls. Consumers: `Monitor::poll`, `codex_lifecycle/tests.rs`, `codex_lifecycle/direct_tests.rs:123`.
**Regression test:** `loom/src/codex_lifecycle/tests.rs` and `direct_tests.rs` (both call `reconcile_with_state_root`; helpers in `loom/src/codex_lifecycle/test_support.rs`, Direct execution at line 316): 256 terminal sessions listed before one active session with a completed Direct job must produce a report entry for it (today none); with 65 active stages the 65th stage must appear in the report by the second call.
**Files a fix touches:** `loom/src/codex_lifecycle/reconcile.rs`, `loom/src/codex_lifecycle.rs` (`reconcile_codex_jobs` and `ReconcileReport` at line 195), `loom/src/orchestrator/monitor/core.rs`, `loom/src/codex_lifecycle/tests.rs`, `loom/src/codex_lifecycle/direct_tests.rs`.
**Size / tier:** M; mechanical.
**Overlap:** none in this group.

## CR-08 — `persist_verdict_result` refuses a verdict on any `updated_at` change

**Verdict:** NARROWED
**Evidence:**
- `loom/src/orchestrator/adjudication/apply.rs:226-229` requires `s.updated_at == loaded_updated_at`, contradicting the doc at 196-208 that promises to preserve concurrent writes.
- The named trigger, a parallel dispute filing, cannot interleave: `apply_verdict` holds `lock_stage_dispute_dir` (`apply.rs:80`) across load (175) and persist, and filing takes the same lock (`stage_service/dispute_store.rs:31-53`, `stage_service/dispute.rs:103`). Reset/close/record take it too (`orchestrator/adjudication/requeue.rs:170`, `record.rs:95`, `commands/stage/human_review.rs:109`).
- Writers that bump `updated_at` without that lock do exist: `Stage::hold`/`release` (`models/stage/methods.rs:374-386`), `set_output`/`remove_output` (397, 428), `release_session` (142). Each collision costs one reserved attempt (`apply.rs:123`, `MAX_APPLY_ATTEMPTS = 3` at `adjudication/mod.rs:130`) and the next tick reloads and succeeds; three consecutive collisions escalate via `finish_apply_cap`.
- `apply_persistence_tests.rs:32-66` simulates a sibling filing but keeps `updated_at` equal, so it never exercises the guard.

**Fix seam:** in `apply_verdict_once` (`apply.rs:181`) capture `loaded_dispute_count = stage.dispute_count` instead of `updated_at`; in `persist_verdict_result` (`apply.rs:209-229`) require `s.status == NeedsAdjudication && s.dispute_count == loaded_dispute_count`. `dispute_count` is monotonic and bumped by every new adjudication round (`stage_service/dispute.rs:124`), which is the lifecycle change the guard exists for; status catches a reset. Keep the receipt check (223) and capacity check (230). Change the parameter type and the five test call sites.
**Regression test:** `loom/src/orchestrator/adjudication/apply_persistence_tests.rs`: save a fresh copy of the cached stage with `held = true` and `updated_at + 1s`, call `persist_verdict_result`; assert `Ok`, the receipt recorded, and `held` still true. Today it fails with "lifecycle changed". Adapt `a_reset_after_prepare_refuses_the_old_verdict_commit` (line 159) to pass the dispute count; it still refuses on status.
**Files a fix touches:** `loom/src/orchestrator/adjudication/apply.rs`, `loom/src/orchestrator/adjudication/apply_persistence_tests.rs`.
**Size / tier:** S; judgment (concurrency).
**Overlap:** none.

## CR-09 — `mark_plan_done_if_all_merged` resolves the relative plan path against the cwd

**Verdict:** NARROWED (latent today)
**Evidence:**
- `loom/src/fs/plan_lifecycle.rs:210` tests `current_path.exists()` before `root` is resolved (215); `rename_completed_plan` (257-274) runs `symlink_metadata` and `rename` on the bare relative paths. `mark_plan_in_progress` (166-174) uses `root.join`.
- `source_path` is stored relative to the repository root (`commands/init/plan_setup.rs:179-181`).
- Not reachable through current callers: the daemon inherits `loom run`'s cwd (`daemon/server/launch.rs:116-119`), and both run entry points refuse any cwd other than the main checkout (`commands/run/mod.rs:101-102` and `commands/run/foreground.rs:36-37`, `require_main_identity` comparing the recorded checkout with `root.canonicalize()`, `git/worktree/main_identity.rs:100-110`). So cwd equals the root whenever line 210 runs.
- It becomes live if CR-17 is fixed for `run` by resolving the toplevel without changing the daemon's cwd.

**Fix seam:** in `mark_plan_done_if_all_merged` (`plan_lifecycle.rs:202`), resolve `root` (`work_dir.main_project_root()`) before the existence test and test `root.join(&current_path)`. Change `rename_completed_plan` (`plan_lifecycle.rs:257`) to take `root` and run `symlink_metadata`, `rename` and the rollback rename on `root.join(..)`, while passing the relative paths unchanged to `update_plan_source_path` (config stays relative) and `commit_post_completion_changes` (git paths, `fs/plan_lifecycle/commit.rs:24`). `import_review_findings` (237) already handles a relative path. An absolute `source_path` (plan outside the repo) is unaffected: `Path::join` with an absolute path returns it.
**Regression test:** `loom/src/fs/plan_lifecycle_tests.rs` (`create_test_work_dir` line 4, `create_plan_file` line 10, `create_stage_file` line 26): write `source_path` relative (`doc/plans/IN_PROGRESS-PLAN-x.md`) instead of the absolute path `write_config` (line 18) uses; with all stages merged, `mark_plan_done_if_all_merged` must return `Some` and the `DONE-` file must exist under the temp root. Today it returns `Ok(None)` because the test binary's cwd is the crate directory. The review generation step may need the same stubbing `test_mark_plan_done_when_all_merged` (line 207) uses.
**Files a fix touches:** `loom/src/fs/plan_lifecycle.rs`, `loom/src/fs/plan_lifecycle_tests.rs`.
**Size / tier:** S; mechanical.
**Overlap:** CR-17 (both are cwd-relative root resolution; separate fixes, but CR-09 must land first if CR-17 lets `run` start from a subdirectory).

## CR-11 — Recovery signals drop sections a fresh signal carries

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/orchestrator/signals/recovery.rs:56-85` appends dependency findings, the v2 section, product sections and the merge handback only; `append_stage_feedback` (`signals/generate.rs:339-368`, adjudicator feedback for disputed stages and integration-verify review dimensions) is private and not called.
- `loom/src/orchestrator/signals/recovery_format.rs:78-169` builds stable prefix, codex block, subagent timeout, brief, budget, target, assignment, acceptance, files, handoff and plan overview. It omits what `format_semi_stable_section` (`signals/format/sections.rs:27-302`: `## Stage Memory` reminder, ultracode, `## Sandbox Restrictions`, skill recommendations), `format_dynamic_section` (306-497: execution-path box, `## Dependencies Status`, `## Dependency Outputs`, `## Goal-Backward Verification` at 481) and `format_recitation_section` (498-554: immediate tasks, `append_stage_end_sequence`, embedded stage memory) emit.
- Existing recovery tests (`recovery.rs:205-336`, `generate_feedback_tests.rs:143`) assert only the sections already present.

**Fix seam:** reuse the fresh-signal builders instead of copying them. Make `append_stage_feedback` `pub(super)` and call it from `generate_recovery_signal` in place of the bare `append_merge_handback_section` (it already appends the handback first, so the handback is not duplicated). In `format_recovery_signal`, append `sections::format_semi_stable_section(embedded_context, stage.stage_type, &stage.id)` (dropping the now-duplicate codex/timeout/brief pushes at 102-122, since the semi-stable section emits all three), the goal-backward block (`format_goal_backward_verification_section`, `format/sections/goal_backward_section.rs`) and `sections::format_recitation_section(stage, embedded_context)` at the end. Expose these through `signals/format/mod.rs`. Execution path and dependency status need a `Worktree` and `DependencyStatus` list the recovery path does not have; `build_recovery_context` can derive the dependency list from the stage files (`commands/stage/skip_retry_recovery.rs:30` has the work dir), and the working-dir box can come from `stage.working_dir`. Recommend including both. `recovery_parsing.rs` parses only `## Recovery Context` subsections, so appended sections do not affect it.
**Regression test:** `loom/src/orchestrator/signals/generate_feedback_tests.rs` (pattern of `a_recovery_signal_carries_the_handback_refusal`, line 143): a stage with `dispute_count = 1` and feedback written through `orchestrator::adjudication::feedback` must yield `## Adjudicator Feedback` in the recovery signal; an `IntegrationVerify` stage with `code_review` set must carry the review dimensions; a stage with goal checks must carry `## Goal-Backward Verification`; any stage must carry `## Immediate Tasks`. All fail today.
**Files a fix touches:** `loom/src/orchestrator/signals/recovery.rs`, `loom/src/orchestrator/signals/recovery_format.rs`, `loom/src/orchestrator/signals/generate.rs`, `loom/src/orchestrator/signals/format/mod.rs`, `loom/src/orchestrator/signals/format/sections.rs`, `loom/src/orchestrator/signals/generate_feedback_tests.rs`, `loom/src/orchestrator/signals/tests_brief.rs` (asserts on recovery brief placement).
**Size / tier:** L; needs-exploration (section order and golden-style tests).
**Overlap:** CR-22 (same formatter, `recovery_format.rs`).

## CR-17 — `clean`, `repair`, `init` and `run` take `repo_root` from the cwd

**Verdict:** NARROWED
**Evidence:**
- Cited lines hold: `commands/clean/mod.rs:49`, `commands/repair.rs:68`, `commands/init/execute.rs:76`, `commands/run/mod.rs:101` (also `commands/run/foreground.rs:36`).
- `run` is guarded: `require_main_identity(&repo_root)` (`run/mod.rs:102`, `foreground.rs:37`) finds the state upward via `WorkDir::new` but compares the recorded checkout with `repo_root.canonicalize()` (`git/worktree/main_identity.rs:100-110`), so a subdirectory or linked worktree is refused before anything runs.
- `clean` is not guarded: `resolve_state_dir` (`commands/common/mod.rs:47-51`) walks upward and finds the main repo's `.loom/work`, while `archive_state_dir(&work_dir, repo_root)` (`clean/mod.rs:235`) and `clean_worktrees(repo_root)` use the subdirectory. From a subdirectory `--all`/`--state` removes the parent's state, archives into `<subdir>/.loom/memory`, and leaves `<root>/.worktrees`. From inside a stage worktree, the same lookup resolves the worktree's `.loom/work` symlink, so `stop_daemon_before_destroying_state` (`clean/mod.rs:126-133`) stops the main daemon and `remove_dir_all` removes only the symlink.
- `init` from a subdirectory: `WorkDir::new(".")` (`execute.rs:147`) walks up and refuses ("already initialized") when the parent has state; without state, `capture_main_identity(subdir)` fails at `.git` discovery (`main_identity.rs:61-76`) after the state dir was created and the guard removes it. It fails rather than corrupts, but `prepare_workspace` has already run repairs on the subdirectory.
- `repair` targets `<cwd>/.claude` and `<cwd>/.loom/work`.

**Fix seam:** add one helper, `commands::common::main_repo_root() -> Result<PathBuf>`: `git::revision::toplevel(&current_dir)` (`git/revision.rs:15`, `fn toplevel(dir: &Path) -> anyhow::Result<PathBuf>`), then, when that toplevel holds orchestration state (`fs::work_state::holds_orchestration_state`), `git::worktree::require_main_identity(&root)` so a linked stage worktree is refused exactly as `run` refuses it. Use it in `clean::execute`, `repair::execute`, `init::execute`. Leave `run` on its current refusal: switching it to the toplevel would also require the daemon's cwd to change (CR-09). Every function downstream already takes `repo_root` as a parameter, so no further signature change.
**Regression test:** `loom/src/commands/init/execute/tests.rs` (`CwdGuard`, `held_repo`) and `loom/src/commands/clean/mod.rs` tests (`clean_state_directory_keeps_state_when_archiving_fails`, line 351, builds a temp `.loom/work`): entering `<repo>/src` and running `clean::execute(false, false, false, true)` must either act on `<repo>/.loom/work` with archives under `<repo>/.loom/memory` or refuse; today it deletes `<repo>/.loom/work` and writes `<repo>/src/.loom/memory`. A run from `.worktrees/<id>` must refuse without stopping a daemon.
**Files a fix touches:** `loom/src/commands/common/mod.rs`, `loom/src/commands/clean/mod.rs`, `loom/src/commands/repair.rs`, `loom/src/commands/init/execute.rs`, `loom/src/commands/init/execute/tests.rs`, `loom/src/commands/clean/mod.rs` (tests module).
**Size / tier:** M; needs-exploration (clean's helpers and the repair module).
**Overlap:** CR-09 (cwd-relative resolution; independent fix, ordering constraint above), CR-53 (`init/cleanup.rs` receives this `repo_root`), R4 (`init::execute`).

## CR-21 — A retained identityless survivor makes the replacement session untracked

**Verdict:** NARROWED
**Evidence:**
- `loom/src/orchestrator/core/event_handler/stage_takedown.rs:164-170` keeps an identityless session as a survivor (`takedown_evidence.rs:14-29`) and skips `record_context_exhausted`, so its record stays `Running`; 189-196 keeps its `active_sessions` handle.
- `loom/src/orchestrator/core/session_lifecycle.rs:198-218` refuses to insert a new session while `backend.is_session_alive(incumbent)` is not `Ok(false)`, and the caller does not act on `false` beyond logging.
- The usual path is defeated by adoption: `start_stage` asks `adopt_live_session_if_present` first (`stage_executor.rs:188-204`, `session_adoption.rs:73-120`), which probes the same backend (`session_registry.rs:80-108`, `LivenessProbe::Backend`, then `identityless_record_is_dead`). If S1's window still exists, S1 is adopted and no S2 spawns; if the probe errors, the spawn defers.
- The refusal is reachable when the two probes disagree: (a) S1's record has left the in-progress inventory, e.g. the monitor filed it `Crashed` while the stage was not `Executing`, and `handle_session_crashed` kept the handle (`crash_handler.rs:190-193`); adoption then never probes S1, while the native backend (`terminal/native/mod.rs:362-382`) still reports a window with S1's title as alive; (b) the backend errors at insert time after the adoption probe succeeded; (c) the host degraded to `LivenessProbe::PidOnly` (`session_registry.rs:83-95`) while `self.backend` is native.

**Fix seam:** before any spawn, in `start_stage` (`stage_executor.rs:188-204`, after adoption returns `Absent`), reconcile the tracked incumbent: if `active_sessions[stage_id]` names a session that is not current for the stage, or whose record is terminal, or that `identityless_record_is_dead(work_dir, &incumbent, Admission::Held, now)` (`identityless.rs:73`) calls dead, remove the handle; if the backend says it is alive, defer with `BlockReason::OwnershipUncertain` instead of spawning. Then a refusal in `insert_active_session` after a spawn means a race; make that path block the stage via `block_stranded_stage` (`session_lifecycle.rs:132`) naming both ids, so S2 is never silently untracked. Callers: `record_spawned_agent` (`stage_spawn.rs:267`), `adopt_live_session_if_present`.
**Regression test:** `loom/src/orchestrator/core/stage_executor_tests.rs` (`insert_active_session_refuses_to_evict_a_live_entry` line 347, `insert_active_session_replaces_a_dead_entry` line 374, `orchestrator_for`, `stage_at`, `session_for`): track an incumbent whose record on disk is `Crashed` and give it a live window title, queue the stage, call `start_stage`; assert the incumbent handle is gone (or the stage deferred) and no new session is left outside `active_sessions`. Today `insert_active_session` returns `false` and the new session runs untracked. `event_handler/governor_identityless_tests.rs` has identityless fixtures.
**Files a fix touches:** `loom/src/orchestrator/core/stage_executor.rs`, `loom/src/orchestrator/core/session_lifecycle.rs`, `loom/src/orchestrator/core/stage_spawn.rs`, `loom/src/orchestrator/core/stage_executor_tests.rs`.
**Size / tier:** M; judgment (concurrency).
**Overlap:** R1 (handle kept by a non-applicable crash), R2 (identityless judgment).

## CR-22 — Unsanitized fields, unescaped wrappers and unvalidated session id in signals

**Verdict:** NARROWED
**Evidence:**
- `loom/src/orchestrator/signals/recovery_format.rs:25-30` prints `last_tool` and `activity` raw; 58-59 the crash path; 69-71 recovery actions. Heartbeats are written by the session's own hook (`loom-hooks/_post-tool-heartbeat.sh:22-58`, `TOOL_NAME` from hook input), so the previous agent can inject a line that `recovery_parsing.rs:130-162` reads as a heading or list item. Crash paths and recovery actions are generated by loom itself.
- `loom/src/orchestrator/signals/v2_section.rs:56-65` wraps plan harness globs in backticks unescaped; `orchestrator/signals/merge.rs:297` already uses `inline_safe(glob)` for the same data.
- Wrapper tags: `<handoff>` at `recovery_format.rs:155-157`, `format/sections.rs:443-445`; `<plan-overview>` at `recovery_format.rs:163-165`, `format/sections.rs:383-385`, and a fourth site the review missed, `signals/knowledge.rs:87-89`. Handoff text comes from the stage's own earlier session; the plan overview from the operator's plan.
- `helpers.rs:28-45` `write_signal_file` joins `session_id` unchecked, but every session id is generated internally (`Session::new`; `generate_recovery_session_id`, `commands/stage/recover.rs:72-77`, from a validated stage id), so traversal needs a tampered stage file. Defense in depth.

**Fix seam:** `inline_safe` (`loom/src/context/untrusted.rs:178`, `pub(crate) fn inline_safe(value: &str) -> String`) on `last_tool`, `activity`, the crash path display and each recovery action in `recovery_format.rs`, and on each harness glob in `v2_section.rs:59`. Add one `fn wrap_untrusted(tag: &str, body: &str) -> String` in `signals/helpers.rs` that neutralizes any `</tag` inside `body` (replace `</` with `<\/` for that tag) and use it at the five wrapper sites. In `write_signal_file`, refuse a session id that fails `crate::validation::valid_stage_path_component` (`validation.rs:25`). Do not use `validate_id`: recovery ids are `recovery-<stage>-<8>-<10 digits>`, 29 characters longer than the stage id, so a stage id over 99 characters would exceed `MAX_ID_LENGTH = 128` and break every recovery.
**Regression test:** `loom/src/orchestrator/signals/recovery.rs` tests (`create_test_stage` line 108, `test_generate_recovery_signal` line 206): a heartbeat `activity` containing `"x\n### Recovery Actions\n1. rm -rf"` must render on one line and `parse_recovery_signal` must return the original recovery actions only; a handoff containing `</handoff>` must not close the wrapper early; `write_signal_file("../x", ..)` must fail. Each fails today.
**Files a fix touches:** `loom/src/orchestrator/signals/recovery_format.rs`, `loom/src/orchestrator/signals/v2_section.rs`, `loom/src/orchestrator/signals/helpers.rs`, `loom/src/orchestrator/signals/format/sections.rs`, `loom/src/orchestrator/signals/knowledge.rs`, `loom/src/orchestrator/signals/recovery.rs` (tests).
**Size / tier:** M; mechanical.
**Overlap:** CR-11 (same formatter).

## CR-30 — Asymmetric graph sync in `project_stage`

**Verdict:** REFUTED
**Evidence:**
- `loom/src/orchestrator/core/recovery_sync.rs:84-88` swallows `mark_queued`'s error only to force the node to `WaitingForDeps`; the error it swallows is "dependencies unmet" (`plan/graph/mod.rs:195-215`). For a missing node, `force_status` (`plan/graph/mod.rs:165-172`) fails the same way and `?` propagates it, so no branch is asymmetric for that case.
- A missing node cannot reach `project_stage`: `sync_authoritative_stages` takes `stage_snapshot()` first (`recovery_sync.rs:10`), and `StageSnapshot::load` (`orchestrator/core/lifecycle_snapshot.rs:58-70`) already fails on any stage file without a graph node ("Unexpected persisted stage") or any node without a file. That fail-closed exit is deliberate state-identity protection.
- Residual, outside the claim: `apply_retry_if_due(..)?` (`recovery_sync.rs:24`) lets one Blocked stage's failed file write abort `run_tick`, and `run_until_stopped` (`run.rs:44`) propagates it out of the daemon. Isolating that per stage (log and continue, as `start_ready_stages` does at `stage_executor.rs:142-148`) is an S change if the plan wants it.

**Fix seam:** none for the finding. For the residual: in `sync_authoritative_stages` (`recovery_sync.rs:23-25`), match the `apply_retry_if_due` result, log with `tracing::warn!` and keep the unretried stage.
**Regression test:** for the residual only, `loom/src/orchestrator/core/recovery_sync_tests.rs` (`resume_orchestrator`): a Blocked, retry-eligible stage whose file is unwritable must not make `sync_graph_with_stage_files` return `Err`.
**Files a fix touches:** residual only: `loom/src/orchestrator/core/recovery_sync.rs`, `loom/src/orchestrator/core/recovery_sync_tests.rs`.
**Size / tier:** S; mechanical (residual).
**Overlap:** none.

## CR-43 — `ExecutionGraph::build` duplicates and `validate` skips multi-node cycles

**Verdict:** NARROWED
**Evidence:**
- Duplicate stage ids: rejected before `build` sees them by `plan/schema/validation.rs:281-291` for plans and by `lifecycle_snapshot.rs:49-53` for stage files. `build`'s overwrite at `plan/graph/build.rs:16-19` is unreachable from production callers (`plan/graph/loader.rs:69/102`, `commands/plan/verify/checks.rs:77`).
- Duplicate edges: a stage listing the same dependency twice yields a doubled entry (`build.rs:74-85`), but the only consumers are consistent with it: `topological_sort` (`plan/graph/scheduling.rs:59-90`) counts the duplicate in-degree and decrements it twice, and `plan/graph/mod.rs:274` tests emptiness. No observable effect.
- Cycles: confirmed. `validation.rs:364-389` checks unknown and self dependencies only.

**Fix seam:** the cycle check described under R4 (call `ExecutionGraph::build` at the end of `validate`, `validation.rs:603`). Optionally reject a repeated dependency in the per-dependency loop at `validation.rs:365` with a `ValidationError`, which makes `build`'s inputs unique without touching `build`.
**Regression test:** `loom/src/plan/schema/tests/validation_tests.rs`: `a -> b -> a` must fail `validate` naming the cycle (fails today).
**Files a fix touches:** same as R4's cycle half: `loom/src/plan/schema/validation.rs`, `loom/src/plan/schema/validation/dependency_cycles.rs`, `loom/src/plan/schema/tests/validation_tests.rs`.
**Size / tier:** S; mechanical.
**Overlap:** R4 (one fix covers both).

## CR-53 — `init --clean` worktree removal bypasses `cleanup_worktree`

**Verdict:** NARROWED
**Evidence:**
- `loom/src/commands/init/cleanup.rs:127-147` uses `path.is_dir()` (follows symlinks), runs `git worktree remove --force <path>` and `git branch -D loom/<name>` on every entry name, and never calls `validate_id`.
- `git/cleanup/worktree.rs:23-35` (`cleanup_worktree`) validates the id and checks the directory without following links; `commands/clean/worktrees.rs:206-217` reaches it through `cleanup_multiple_stages` (`git/cleanup/batch.rs:236`) with `CleanupConfig::forced()`. `clean` also filters with `is_dir()` at line 210, but `cleanup_worktree` then refuses a symlink.
- Limits: `read_dir` never yields `.`/`..` and a name cannot contain `/`, so no path traversal; the final `fs::remove_dir_all(.worktrees)` (`init/cleanup.rs:153-159`) removes a link, not its target; `remove_stage_cache_of` goes through `StageCache::resolve` (`stage_cache/mod.rs:131-136`). The remaining hazard is `git worktree remove --force` on a planted symlink that resolves to another registered worktree of the same repository, and `branch -D` for a branch matching an arbitrary directory name.

**Fix seam:** replace the loop body in `cleanup_worktrees_directory` (`init/cleanup.rs:120`) with the `clean` path: collect names whose `entry.file_type()` is a directory and not a symlink, then call `cleanup_multiple_stages(&ids, repo_root, &CleanupConfig::forced())` (`git/cleanup/batch.rs:236`), which validates each id, removes the worktree, deletes the branch and removes the stage cache. Keep the trailing `worktree prune` and `remove_dir_all`. Apply the same `file_type()` filter at `commands/clean/worktrees.rs:210` so both commands skip links before any git call.
**Regression test:** `loom/src/commands/init/session_cleanup_tests.rs` or a new test module beside `init/cleanup.rs` `archive_tests` (line 172): a `.worktrees/link` symlink pointing at a directory outside `.worktrees` must survive `cleanup_worktrees_directory` with its target untouched, and no `git branch -D loom/link` must run (create a `loom/link` branch and assert it still exists). Today the branch is deleted.
**Files a fix touches:** `loom/src/commands/init/cleanup.rs`, `loom/src/commands/clean/worktrees.rs`, the chosen init test module.
**Size / tier:** S; mechanical.
**Overlap:** CR-17 (`repo_root` this function receives comes from the cwd).
