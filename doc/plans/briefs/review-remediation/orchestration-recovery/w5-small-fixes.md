# orchestration-recovery / w5-small-fixes: verdict guard, worker start gate, one stall bound

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/orchestrator.md` section CR-08; `validation/runtime.md` section CR-74, sub-claim (d) only; `validation/cli-tui-webserver.md` section CR-77, the `MAX_STALL_RECOVERIES` bullet only.

## Findings covered

- CR-08: `persist_verdict_result` refuses a verdict whenever `updated_at` moved, so a hold, release or output write during adjudication costs an apply attempt and three in a row escalate the dispute.
- CR-74(d): `apply_with_backend` admits a native worker `Start` for a lead whose stage is no longer `Executing`.
- CR-77 (one bullet): `MAX_STALL_RECOVERIES` is defined twice, in `event_handler/recover_hung.rs` and `commands/stage/skip_retry_silence.rs`.

## Files

Own (write):
- `loom/src/orchestrator/adjudication/apply.rs`
- `loom/src/orchestrator/adjudication/apply_persistence_tests.rs`
- `loom/src/orchestrator/core/inbox_drain/worker_control.rs`
- `loom/src/orchestrator/core/inbox_drain/worker_control_tests.rs`
- `loom/src/orchestrator/core/event_handler/recover_hung.rs`
- `loom/src/orchestrator/core/event_handler.rs`
- `loom/src/orchestrator/core/mod.rs`
- `loom/src/commands/stage/skip_retry_silence.rs`

Read only: `loom/src/orchestrator/core/inbox_drain/apply.rs` (`require_owner`, `end_turn`), `loom/src/orchestrator/core/inbox_drain/test_support.rs`, `loom/src/native_worker/store.rs`, `loom/src/stage_service/dispute.rs`.

Frozen: none for this worker.

## Changes

### 1. CR-08 (`adjudication/apply.rs`)

`apply.rs` is 399 lines: every edit here must keep the count the same or lower.
- `apply_verdict_once` (line 181): replace `let loaded_updated_at = stage.updated_at;` with `let loaded_dispute_count = stage.dispute_count;` and pass it at line 190.
- `persist_verdict_result` (lines 209-243): the parameter `loaded_updated_at: chrono::DateTime<chrono::Utc>` becomes `loaded_dispute_count: u32`; the guard at line 227 becomes `s.status == StageStatus::NeedsAdjudication && s.dispute_count == loaded_dispute_count`. Keep the receipt check (line 223) before it and the capacity check (line 230) after it, and keep the error text `stage lifecycle changed while applying dispute {dispute_id}; refusing stale verdict`.
- Rewrite the last paragraph of its doc comment (lines 207-208): a new adjudication round (`dispute_count`, bumped by every filing, `stage_service/dispute.rs:124`) or a status change refuses the commit; other concurrent writes (hold, release, outputs) are kept, which is what the field-by-field re-apply above is for.

### 2. CR-74(d) (`inbox_drain/worker_control.rs`)

In `apply_with_backend` (lines 32-73), after `require_owner` returns the stage and before taking the admission lock, refuse a `Start` unless the stage is `Executing`: `if matches!(payload, WorkerControlPayload::Start { .. }) && stage.status != StageStatus::Executing { return Settle::Refused(format!("stage '{stage_id}' is {:?}, not Executing; a native worker starts only while its stage runs", stage.status)); }`. `Message` and `Cancel` stay allowed in every status: a stage that is winding down must still be able to cancel its workers. Import `StageStatus`.

### 3. CR-77, the stall bound (`recover_hung.rs`, `event_handler.rs`, `core/mod.rs`, `skip_retry_silence.rs`)

`models/stage/types.rs` is in the maintainability baseline at 567 lines and `models/stage/mod.rs` re-exports `types` item by item, which is outside this stage; so the shared constant lives with its owner, the daemon's stall handler:
- `recover_hung.rs:55`: `const MAX_STALL_RECOVERIES` becomes `pub(crate) const MAX_STALL_RECOVERIES` (doc comment unchanged).
- `event_handler.rs`: next to `use recover_hung::HungReport;` (line 31) add `pub(crate) use recover_hung::MAX_STALL_RECOVERIES;`.
- `core/mod.rs`: beside `pub(crate) use crash_classification::spawn_failure_type;` (line 60) add `pub(crate) use event_handler::MAX_STALL_RECOVERIES;`.
- `skip_retry_silence.rs`: delete the private const and its four-line doc comment (lines 20-24, the "revisit when that const is exported" note included) and import `crate::orchestrator::core::MAX_STALL_RECOVERIES`.

## Interfaces shared with other workers

None. `core/mod.rs` is yours alone; other workers of this stage declare their new modules with `#[path]` inside their own files. Loom adds the `mod` lines for the stage's contract test files itself; if you find such lines in `core/mod.rs`, leave them alone.

## Tests

- `apply_persistence_tests.rs`: add `a_concurrent_hold_does_not_refuse_the_verdict`: save a fresh copy of the cached `NeedsAdjudication` stage with `held = true` and `updated_at` one second later, call `persist_verdict_result(.., cached.dispute_count, ..)`; assert `Ok`, the receipt recorded (`tally.request_receipts` non-empty), the marker written and `held` still true. Today it fails with "lifecycle changed". Add `a_new_dispute_round_refuses_the_old_verdict`: the fresh copy has `dispute_count + 1`; assert the same error text and no receipt. Change the five existing calls (lines 21, 54, 96, 144, 173) to pass `cached.dispute_count` instead of `cached.updated_at`; their assertions stay as they are, and `a_reset_after_prepare_refuses_the_old_verdict_commit` still refuses on status.
- `worker_control_tests.rs`: add `a_start_for_a_stage_that_left_execution_is_refused`: `fx.stage(StageStatus::Blocked, Some(&record.id))` with `start_payload()`; assert `Settle::Refused(_)` whose text names `Blocked`, and no invocation directory under `store::stage_dir` for the stage. Add `a_cancel_still_applies_after_execution_ended`: the pattern of the existing cancel test in that file with the stage `Blocked`; it is not refused for the status. Today the start is admitted.
- The stall bound has no runtime test: a single definition is the proof. Keep `recover_hung_tests.rs` and `recover_hung_park_tests.rs` unchanged and passing.

## Proof

From `loom/`: `cargo test --lib orchestrator::adjudication::`, `cargo test --lib orchestrator::core::inbox_drain::`, `cargo test --lib orchestrator::core::event_handler::`, `cargo test --lib commands::stage::`.

## Traps

- Comparing `review_reason` or `issue_history` instead of `dispute_count` reintroduces a refusal for writes the verdict is meant to keep.
- Dropping the status half of the guard lets a verdict land on a stage a reset already moved on (the reset test pins it).
- Refusing `Cancel` for a non-`Executing` stage strands workers of a stage that is winding down.
- `pub(crate) use` of an item from a private module compiles only when the item itself is `pub(crate)`; change the const's visibility first.
