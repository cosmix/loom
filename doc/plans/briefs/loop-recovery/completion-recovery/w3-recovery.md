# W3 — Graph resume, explicit takedown reasons, and bounded blocker parking

Lane: Codex `gpt-5.6-sol`, effort `xhigh`. Starts after W1; may run parallel with W4. W2 owns all completion CLI/control/protocol/daemon bridge files. Do not edit them and do not run git.

## Owned files

- `loom/src/orchestrator/core/event_handler.rs`
- `loom/src/orchestrator/core/event_handler/stage_takedown.rs`
- `loom/src/orchestrator/core/event_handler/recover_hung.rs`
- `loom/src/orchestrator/core/event_handler/recover_hung_tests.rs`
- `loom/src/orchestrator/core/event_handler/governor_tests.rs`
- `loom/src/orchestrator/core/event_handler/governor_retry_tests.rs`
- `loom/src/orchestrator/core/event_handler/governor_tests_restart.rs`
- `loom/src/orchestrator/core/event_handler/takedown_identity_tests.rs`
- `loom/src/orchestrator/core/event_handler/verdict_retirement_tests.rs`
- `loom/src/orchestrator/core/recovery.rs`
- `loom/src/orchestrator/core/recovery_sync_tests.rs`
- `loom/src/orchestrator/core/completion_handler.rs`
- `loom/src/orchestrator/monitor/core.rs`
- `loom/src/orchestrator/monitor/mod.rs`
- `loom/src/orchestrator/monitor/session_events.rs`
- `loom/src/orchestrator/monitor/completion_blockers.rs` (new)
- `loom/src/orchestrator/monitor/events.rs`
- `loom/src/orchestrator/monitor/detection.rs`
- `loom/src/plan/graph/mod.rs`
- `loom/src/plan/graph/tests.rs`
- `loom/src/orchestrator/core/judge_close.rs`
- `loom/src/orchestrator/core/verdict_apply.rs`
- `loom/src/orchestrator/core/verdict_apply_tests.rs`
- `loom/src/orchestrator/core/event_handler/stalled_judge.rs`
- `loom/src/orchestrator/core/event_handler/stalled_judge_tests.rs`
- `loom/src/orchestrator/core/merge_handler.rs`
- `loom/src/orchestrator/core/merge_handler_attempt_tests.rs`
- `loom/src/commands/stage/state.rs`

## Graph resume

Add `ExecutionGraph::mark_resumed(stage_id) -> Result<()>`: accept `WaitingForInput -> Executing` and idempotent `Executing`; reject every other predecessor. Keep `mark_executing` Queued-only. In the `StageResumedExecution` event arm, load the durable stage, require disk it is `Executing`, call `mark_resumed`, and persist through the orchestrator's existing graph persistence path. In restart synchronization, disk Executing plus graph WaitingForInput uses `mark_resumed`; do not route unrelated mismatches through it.

Test real ordering: waiting stage file/event -> resumed stage file/event -> graph persisted Executing -> restart sync stays coherent -> accepted completion marks Completed and a merged dependency becomes ready. Add duplicate resume, resume-before-stale-wait event, newer non-Executing disk status, and illegal graph predecessor cases.

## Explicit takedown reasons

Change `take_down_stage_agents` and `take_down_agents` to require `SessionExitReason`. Migrate every sibling caller in the owned files: agent/daemon ceiling uses `ContextCeiling`; hung recovery uses `Stalled`; accepted adjudication retirement uses `Replaced`; operator stop uses `OperatorStop` if a caller exists in this territory. `stage_takedown::record_context_exhausted` calls W1's locked terminal-reason primitive. Preserve its proof rule: no requeue until every assigned stage agent is confirmed gone; missing PID identity remains uncertainty and a survivor.

Successful ordinary completion in `completion_handler.rs` records `Completed` on the exact session before or during cleanup without letting failed best-effort teardown block merge/graph progress. W1 sets ordinary crash/completion defaults at the Session transition primitives. Migrate `session_events::persist_session_status`, `judge_close::close_adjudication_session` and its verdict/stalled-judge callers, operator reset in `commands/stage/state.rs`, and stale-writer cleanup in `merge_handler.rs` to the exact locked reason primitive. Killing an uncertain process is never evidence of successful retirement.

Use this complete status/reason matrix: normal success `Completed/Completed`; unexpected exit `Crashed/Crashed`; ceiling `ContextExhausted/ContextCeiling`; hung retirement `ContextExhausted/Stalled`; verified completion blocker `ContextExhausted/CriteriaBlocked`; adjudicated replacement or stale-writer replacement `ContextExhausted/Replaced`; operator kill/reset `ContextExhausted/OperatorStop`. W3 explicitly persists CriteriaBlocked before parking. Old records without reasons remain readable. Cover all these entry paths in existing module tests and the new blocker module.

Change the actual fallthrough branches: `state::reset` must return an error without clearing stage ownership until every target is re-probed as confirmed gone. `merge_handler` must retain its active-session entry and signal and skip spawning on kill failure, unknown identity, surviving writer, or liveness-probe error. Move removal/spawn after successful retirement proof; a failed lookup is uncertainty, not permission to spawn. Persist OperatorStop/Replaced only after that confirmation. Tests must drive these error branches and assert no new session or cleared ownership.

## Diagnostic evidence lifecycle

W2 records trusted boundary attempts into W1's exact-session checkpoint only. W2 never changes stage status because of diagnostic evidence. Register `completion_blockers` in monitor/mod.rs and call it from `Monitor::poll` before ordinary hung detection; emit typed events handled by the owned event-handler. Restart replay reads the exact current stage/session and commit, not in-memory counters. ToolFailed/EvidenceMissing never qualify for blocker action; only a verified attempt with an external boundary failure code does.

On the first verified completion boundary failure, immediately expose pending diagnostics; do not wait for heartbeat staleness. Count distinct trusted attempt nonces, never duplicate delivery. On the second identical fingerprint for the same session and commit, establish writer disposition. An authoritative active child forbids takedown. A single verified boundary failure with no active child gets at most one existing idle budget before this same disposition/escalation path, preventing a lone final-turn failure from buying another three budgets:

- confirmed gone: atomically accumulate attempt time, set `NeedsHumanReview`, set bounded `review_reason` carrying blocker short fingerprint and next action, retain the checkpoint, and do not requeue;
- confirmed alive: request/perform the existing safe takedown, then park only after confirmed death;
- unknown identity or survivor: retain diagnostic state, emit immediate escalation, and never reassign or mark parked as writer-free.

No path automatically starts a successor for a repeated verified external failure. `recover_hung` must not spend a stall recovery on repeated trusted blocker evidence. A healthy silent foreground child has no trusted completion fingerprint and retains the existing three-budget protection. A first failure is visible immediately; it never silently begins another 90-minute cycle.

Clear/invalidate diagnostic actionability when the stage session changes, commit changes, stage completes, an operator reset starts a new attempt, or evidence fails exact identity/schema validation. Preserve historical checkpoint evidence for a later operator-approved continuation; clearing actionability does not delete history.

## Regressions and proof

Use fake clocks and fake backend/process identity. Prove first failure surfaces without transition; second identical + confirmed death parks; alive writer is killed and confirmed before parking; unknown identity never parks/requeues; changed commit/session/fingerprint resets repetition; parked blocker is ignored by generic hung recovery; healthy no-fingerprint foreground work is not killed early; stall and ceiling persist distinct exit reasons; delayed lifecycle events cannot relabel the reason.

Optional single scoped check: `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::core::`.

Done means disk stage, graph, session reason, checkpoint identity, and process ownership agree across live event and daemon restart paths.
