# W4: completion as a host job (stage daemon-completion)

Read `doc/plans/briefs/host-verification/common.md` first. Sections 4.1, 4.2, 4.4, 4.5 and 4.6 are your specification, and 3.7 is the session protocol you reuse. Paths are relative to `loom/`. Stage `host-jobs` merged `src/host_verify/`; read it before writing.

## Territory

You own:

- `src/host_verify/mod.rs`, `store.rs`, `wait.rs`, `child.rs`, `apply.rs` (new), `apply_rejection.rs` (new, if `apply.rs` would pass 400 lines), `tests_apply.rs` (new), `tests_child.rs`;
- `src/commands/stage/{complete.rs, complete_pipeline.rs, complete_resolved.rs, complete_verification.rs, complete_verification_v2.rs, complete_verification_v2_tests.rs, complete_impact.rs (new), complete_relay.rs (new), knowledge_complete.rs, control_session.rs}` and `src/commands/stage/tests/{complete.rs, complete_knowledge_tests.rs}`;
- `src/relay/{kind.rs, payload.rs, matrix.rs, matrix_host.rs, tests_matrix.rs, mod.rs, emit.rs}` and `src/relay/emit/**`;
- `src/orchestrator/core/inbox_drain.rs`, `src/orchestrator/core/inbox_drain/{apply.rs, test_support.rs, tests_matrix.rs}` and `src/orchestrator/core/inbox_drain/complete.rs` (new);
- `../loom-hooks/loom-relay.sh` and `../loom-hooks/tests/loom-relay-kinds.sh`.

Do not touch `tests/daemon_completion_contracts*` or `tests/host_verification_contracts*` (frozen contracts), or any file W5, W6 or W7 owns:

- W5: `host_verify/freeze.rs`, `orchestrator/core/host_jobs.rs`, the stall monitor, the freeze CLI (`commands/stage/contracts.rs`, `contracts/freeze.rs`), the freeze handlers;
- W6: `completion_evidence*`, `commands/stage/control_complete.rs`, `commands/stage/mod.rs`, the daemon completion RPC files, `daemon/server/observer.rs`, the hook `loom-control-complete.sh`;
- W7: doctrine.

W6 deletes the broker modules at the same time as you work. You remove every call into them from your files, so that once both of you return nothing references them. When you need code from a file W6 deletes (`daemon/server/control_complete.rs`, `daemon/server/completion_evidence.rs`, `commands/stage/completion_evidence.rs`, `commands/stage/completion_evidence/{broker,impact_record}.rs`), read it with `git show HEAD:loom/<path>`, never from the working tree.

## Steps

1. **Kinds** (`host_verify/mod.rs`):
   - `JobKind::{Check, Complete, Freeze}`; update every `match` on `JobKind` (`wait.rs`, `child.rs`).
   - `pub mod apply; pub mod freeze;` and `#[cfg(test)] mod tests_apply; #[cfg(test)] mod tests_freeze;` (W5 writes `freeze.rs` and `tests_freeze.rs`).
2. **Relay.**
   - `RequestKind::Complete`: wire `complete`, a control kind, appended LAST in `all()` (12). The test `is_control_matches_the_seven_control_kinds` gains `RequestKind::Complete` at the end of its expected list.
   - `RequestPayload::Complete(HostJobRequest)`.
   - Matrix: add the six `Complete` rows to `relay/matrix_host.rs::HOST_JOB_MATRIX`: `Apply` for `Stage` and `Knowledge`, `Refuse` otherwise. Add a `COMPLETE_COLUMN` test in `relay/tests_matrix.rs`. `relay/matrix.rs` stays under 400 lines.
   - The `FreezeContracts` payload tests in `relay/payload.rs` follow the new shape (common.md 4.1).
   - `inbox_drain/tests_matrix.rs::classify_applied` and `inbox_drain/test_support.rs::payload_for` gain a `Complete` arm; `payload_for`'s freeze JSON becomes `{"request": "freeze_contracts", "head": .., "tree_digest": ..}`. The Contract/freeze cell of `inbox_drain/tests_matrix.rs` (about lines 140-142) asserted a freeze after one pass; it now asserts that a `Freeze` host job is current. This is an expected integrity event.
   - `relay/emit.rs`: remove `control_broker` (and `LOOM_CONTROL_BROKER`) from `EnvSnapshot` and `mode`, with `emit/tests_mode.rs`. With `LOOM_SESSION_ID` set, any mode that is not `Relay` is `Legacy`.
   - `relay/mod.rs`: fix the doc comment that names the broker (about line 47).
   - `../loom-hooks/loom-relay.sh::relay_kind_at`: add `stage:complete) echo complete ;;`; `drop_control_kinds`: add `complete`. In `../loom-hooks/tests/loom-relay-kinds.sh`: `loom stage complete s1` derives `complete` for the main agent, and a subagent's derived set drops it.
3. **Drain.**
   - `inbox_drain/complete.rs::apply_complete(work_dir, stage_id, record, &HostJobRequest, request_id) -> Settle`, declared in `inbox_drain.rs`:
     - the session must own the stage (`require_owner`), be a `Stage` or `Knowledge` session, and the stage must be `Executing`;
     - validate `head` and `tree_digest` as `check.rs` does;
     - the checkout HEAD must equal `head`;
     - then `submit` with kind `Complete`, and settle as `check.rs` does (every `Applied` detail starts with `host job <id>`).
   - The `apply.rs` arm for `RequestPayload::Complete`.
   - The `StageRequest::FreezeContracts { head, tree_digest }` arm calls the new `crate::daemon::handle_freeze_contracts(work_dir, stage_id, &record.id, &head, &tree_digest, request_id)` (common.md 4.6) and settles as 4.6 says.
4. **Child** (`child.rs`): dispatch on `record.kind`.
   - `Check`: unchanged.
   - `Freeze`: `crate::host_verify::freeze::run_freeze_job(work_dir, &record)`.
   - `Complete`, common.md 4.2:
     - A knowledge stage runs the snapshot plus acceptance in the repository root (`acceptance_runner::resolve_knowledge_execution_paths`).
     - Any other stage:
       1. `VerificationSnapshot::capture(&stage, work_dir)` and `snapshot.commit == record.head`.
       2. Acceptance with the observer, as `Check` does.
       3. `complete_verification::run` with `daemon_gates: true, fail_closed: true`.
       4. Turn each check that `run` performs into an outcome. Have `run` return a `Vec<(label, Result<()>)>`, or wrap each call site; pick the smaller change.
       5. `snapshot.validate(&stage, work_dir)` at the end.
     - `result.policy_hash = Some(snapshot.policy)`.
     - Every failed run of a v2 standard stage writes `reviews/<stage>/impact-observed.json` through `verify::impact_tests::observed::record_observed`: the failures when the impact selection failed, an empty `ObservedImpact` otherwise.
5. **`VerificationChecks`** (`complete_verification.rs`, `complete_verification_v2.rs`, `complete_verification_v2_tests.rs`):
   - Replace `control_session: Option<&str>` with `daemon_gates: bool` and `fail_closed: bool`.
   - The operator host path (`complete_pipeline.rs::complete_normal_stage` with no session) passes `false, false`.
   - `complete_resolved.rs::verify_resolved_completion` passes `false, false`.
   - The test literal in `complete_verification_v2_tests.rs` (about line 144) follows.
6. **Apply** (`apply.rs`), exactly common.md 4.4.
   - Port `validate_active_identity` and the session-retirement half of `retire_completed_session` from `daemon/server/control_complete.rs` (via `git show HEAD:`).
   - Gates: `crate::daemon::check_completion_gates(work_dir, stage_id)` (W6 makes it reachable).
   - Receipt: first `record_attempt_handoff` a `VerifiedPendingAck` attempt whose `evidence_nonce` is the job id and whose criteria are the outcomes, then `crate::handoff::record_accepted_handoff` the `AcceptedReceipt`, as `daemon/server/completion_evidence.rs::persist_acceptance_receipt` did.
   - Failure checkpoints (`Failed` only) through `crate::handoff::completion::record::record_attempt_handoff` with a `CompletionAttemptEvidence` that passes its own `validate()`:
     - `version = COMPLETION_EVIDENCE_VERSION`;
     - `commit` = the job's head;
     - `check_definition_hash` = `result.policy_hash` or the current policy hash;
     - `exact_command = "loom stage complete <stage>"`;
     - `evidence_nonce` = job id;
     - `observed_at` = now, RFC 3339.

     Match how `broker.rs::record_diagnostic` filled the same struct.
   - `Cancelled` and `Interrupted` Complete jobs: `Recorded`, no checkpoint.
   - A `Failed` job whose session no longer owns the stage: `Ignored("session no longer owns the stage")`, nothing written (common.md 4.4).
   - Durability, common.md 4.4: binding and ownership failures are `Ok`, never `Err`; a re-application finding the stage already `Completed` at `result.head` finishes only the missing receipt and session retirement, writes the marker and returns `Ignored("already completed")`.
   - `DaemonRejected` for a failed binding: port `commands/stage/completion_evidence.rs::with_boundary_failure` (failure code `daemon_rejected`) into `host_verify`, so repeated-blocker escalation keeps counting it, and carry the binding's message as `diagnostic_first_line`.
   - `Freeze` records go to `crate::host_verify::freeze::apply_freeze_result`.
   - After applying any record, write the job's `applied` marker (`store::save_job_file`). Add `pub fn unapplied_terminal_records(work_dir) -> Result<Vec<JobRecord>>` (terminal current records without the marker) for W5's startup call.
7. **In-session `loom stage complete`** (`complete.rs`, `control_session.rs`, `knowledge_complete.rs`, `complete_pipeline.rs`):
   - `Legacy` mode refuses with the message in common.md 4.5, exit 1. Test it with a hand-built `EnvSnapshot`.
   - `Relay` mode for a stage or knowledge session lives in a new `src/commands/stage/complete_relay.rs` (declared and called from `complete.rs` as `complete_relay::..`), which calls `crate::host_verify::wait::wait_or_request(` itself; the stage's wiring checks both:
     - Refuse `--no-verify`, `--force-unsafe` and `--assume-merged` with the existing message for sandboxed privileged flags.
     - Refuse with `Commit first: completion verifies committed content only.` and the error, exit 1, when `handoff::completion::content::committed_content(&stage, repo_root)` fails.
     - Run `host_verify::wait::wait_or_request` with kind `Complete`, the checkout being the stage worktree or, for a knowledge session, the repository root.
     - On a `Passed` job, poll the stage file and the session checkpoint up to the remaining budget: the stage `Completed` → `Host job: completed (complete, <short head>)` and `Stage '<stage>' was completed by the daemon. End the session now.`, exit 0; a `DaemonRejected` attempt whose `evidence_nonce` is the job id, read with `load_session_checkpoint` → `Host job: rejected (...)` and its diagnostic, exit 1; neither → `Host job: pending (...)`, exit 0, scratch state kept.
     - Other outcomes take the codes of common.md 3.7.
   - Delete from your files everything that existed only for the broker:
     - `handle_broker_request`, `load_and_run_broker`, `read_broker_input*`, `require_wrapper_identity` (if only the broker used it) in `control_session.rs`;
     - `emit_control_completion_evidence` and the `control_session` threading in `complete_pipeline.rs`;
     - `print_sandboxed_completion_pending_notice`, the in-session `after_failed_completion` impact printing, and the `mod control_complete;` declaration (about `complete.rs:35-36`) in `complete.rs`;
     - `verify_knowledge_for_broker` in `knowledge_complete.rs`.
   - The operator path keeps its impact bookkeeping: port `completion_evidence/impact_record.rs::after_failed_completion` (turn `ImpactTestsFailed` into `ObservedImpact` through `record_observed`, clear the record on other failures) into `src/commands/stage/complete_impact.rs` and call it where `complete.rs` calls it today (`.inspect_err(..)`).
   - Keep `sandbox_control_session`'s identity checks if the new route still needs them (session, stage and worktree agreement); rename it if its name now misleads.
   - The operator path (no session) is otherwise unchanged.
   - `control_session.rs` is 393 lines; it should shrink.
8. **Tests.**
   - `tests_apply.rs`:
     - passing job completes (stage `Completed`, `completed_commit` = head, attempt and receipt persisted, session record `Completed`, `applied` marker written);
     - failing job writes `ToolFailed` with every label;
     - cancelled job writes no checkpoint;
     - moved head writes `DaemonRejected`;
     - policy amended after the run writes `DaemonRejected`;
     - a passing job whose session no longer owns the stage writes `DaemonRejected`; a failing one is `Ignored` and writes nothing;
     - a v2 standard stage with no review round writes `DaemonRejected` (the completion gates);
     - knowledge stage sets `merged`;
     - a `Check` record is `Ignored`;
     - an applied record is not applied twice;
     - crash window: a stage set `Completed` at the head with no receipt and no marker (simulate by running steps 2-5 only) → `apply_finished` records the receipt, retires the session, writes the marker and returns `Ignored("already completed")`, and no `DaemonRejected` attempt appears;
     - an unwritable checkpoint directory makes `apply_finished` return `Err` and write no marker; restoring it and calling again completes.
   - `tests_child.rs`: a `Complete` job on a committed fixture passes; a dirty tree fails at capture (both behind the confinement guard of common.md section 5).
   - `commands/stage/tests/complete.rs` and `complete_knowledge_tests.rs`: drop tests that pinned evidence output (`verified_evidence_record_has_broker_boundaries` and similar). Add one per refusal (Legacy, privileged flag, uncommitted tree) using a hand-built `EnvSnapshot` (`relay/emit.rs` returns an empty one under `cfg(test)`; build the `RelayMode` by hand as other relay tests do). No test spells `LOOM_CONTROL_EVIDENCE` (common.md section 5).

## Proof (run once each, from `loom/`, after W5 and W6 have also written; if the build fails only on their missing items, report that and stop)

```bash
cargo test --lib host_verify:: 2>&1 | tail -20
cargo test --lib commands::stage:: 2>&1 | tail -10
cargo test --lib relay:: 2>&1 | tail -10
cargo test --lib orchestrator::core::inbox_drain:: 2>&1 | tail -10
cd .. && bash loom-hooks/tests/loom-relay-kinds.sh 2>&1 | tail -5
```

## Report

Files changed and deleted; how `complete_verification::run` reports per-check outcomes; the attempt, receipt and checkpoint fields; every broker reference you removed (`file:line` before); the proof output.
