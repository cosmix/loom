# W4: completion as a host job (stage daemon-completion)

Read `doc/plans/briefs/host-verification/common.md` first. Sections 4.1, 4.2, 4.4, 4.5 and 4.6 are your specification, and 3.7 is the session protocol you reuse. Paths are relative to `loom/`. Stage `host-jobs` merged `src/host_verify/`; read it before writing.

## Territory

You own:

- `src/host_verify/mod.rs`, `child.rs`, `apply.rs` (new), `tests_apply.rs` (new), `tests_child.rs`;
- `src/commands/stage/{complete.rs, complete_pipeline.rs, complete_resolved.rs, complete_verification.rs, complete_verification_v2.rs, knowledge_complete.rs, control_session.rs}` and `src/commands/stage/tests/{complete.rs, complete_knowledge_tests.rs}`;
- `src/relay/{kind.rs, payload.rs, matrix.rs}`;
- `src/orchestrator/core/inbox_drain/{apply.rs, mod.rs}` and `src/orchestrator/core/inbox_drain/complete.rs` (new);
- `../loom-hooks/loom-relay.sh`.

Do not touch `tests/daemon_completion_contracts*` (frozen contracts), or any file W5, W6 or W7 owns:

- W5: `host_verify/freeze.rs`, `orchestrator/core/host_jobs.rs`, the stall monitor, the freeze CLI, the freeze handlers;
- W6: `completion_evidence*`, `commands/stage/control_complete.rs`, `commands/stage/mod.rs`, the daemon completion RPC files, the hook `loom-control-complete.sh`;
- W7: doctrine.

W6 deletes the broker modules. You remove every call into them from your files, so that once both of you return nothing references them.

## Steps

1. **Kinds** (`host_verify/mod.rs`):
   - `JobKind::{Check, Complete, Freeze}`.
   - `JobResult.policy_hash` (common.md 4.1).
   - `pub mod apply; pub mod freeze;` declarations (W5 writes `freeze.rs`).
2. **Relay.**
   - `RequestKind::Complete`: wire `complete`, a control kind; `all()` grows to 12.
   - `RequestPayload::Complete(HostJobRequest)`.
   - Matrix: `Complete` is `Apply` for `Stage` and `Knowledge` sessions, `Refuse` otherwise.
   - `../loom-hooks/loom-relay.sh`: if `drop_control_kinds` lists kinds by name, add `complete`, and extend that hook's test in `../loom-hooks/tests/` that covers the drop (`rg -n 'drop_control_kinds|freeze-contracts' ../loom-hooks/tests`). That test file is W6's territory only if it is a `loom-control-complete*.sh` test; any other hook test is yours for this one edit.
3. **Drain.**
   - `inbox_drain/complete.rs::apply_complete(work_dir, stage_id, record, &HostJobRequest, request_id) -> Settle`:
     - the session must own the stage (`require_owner`), be a `Stage` or `Knowledge` session, and the stage must be `Executing`;
     - validate `head` and `tree_digest` as `check.rs` does;
     - the checkout HEAD must equal `head`;
     - then `submit` with kind `Complete`, and settle as `check.rs` does.
   - The `apply.rs` arm for `RequestPayload::Complete`.
   - The `StageRequest::FreezeContracts { head, tree_digest }` arm calls the new `crate::daemon::handle_freeze_contracts(work_dir, stage_id, &record.id, &head, &tree_digest, request_id)` (common.md 4.6).
4. **Child** (`child.rs`): dispatch on `record.kind`.
   - `Check`: unchanged.
   - `Freeze`: `crate::host_verify::freeze::run_freeze_job(work_dir, &record)`.
   - `Complete`, common.md 4.2:
     - A knowledge stage runs the snapshot plus acceptance in the repository root (`knowledge_complete.rs::resolve_knowledge_execution_paths` or its equivalent).
     - Any other stage:
       1. `VerificationSnapshot::capture(&stage, work_dir)` and `snapshot.commit == record.head`.
       2. Acceptance with the observer, as `Check` does.
       3. `complete_verification::run` with `daemon_gates: true, fail_closed: true`.
       4. Turn each check that `run` performs into an outcome. Have `run` return a `Vec<(label, Result<()>)>`, or wrap each call site; pick the smaller change.
       5. `snapshot.validate(&stage, work_dir)` at the end.
     - `result.policy_hash = Some(snapshot.policy)`.
     - A failing impact selection writes `reviews/<stage>/impact-observed.json` through the existing host-completion writer (`rg -n 'fn record_observed' src`).
5. **`VerificationChecks`** (`complete_verification.rs`, `complete_verification_v2.rs`):
   - Replace `control_session: Option<&str>` with `daemon_gates: bool` and `fail_closed: bool`.
   - The operator host path (`complete_pipeline.rs::complete_normal_stage` with no session) passes `false, false`.
   - `complete_resolved.rs::verify_resolved_completion` passes `false, false`.
6. **Apply** (`apply.rs`), exactly common.md 4.4.
   - Move `validate_active_identity` and the session-retirement half of `retire_completed_session` here from `daemon/server/control_complete.rs`. W6 deletes that file, so copy what you need now.
   - Persist the receipt through `crate::handoff::record_accepted_handoff`, as `daemon/server/completion_evidence.rs::persist_acceptance_receipt` did.
   - Write failure checkpoints through `crate::handoff::completion::record::record_attempt_handoff` with a `CompletionAttemptEvidence` that passes its own `validate()`:
     - `version = COMPLETION_EVIDENCE_VERSION`;
     - `commit` = the job's head;
     - `check_definition_hash` = `result.policy_hash` or the current policy hash;
     - `exact_command = "loom stage complete <stage>"`;
     - `evidence_nonce` = job id;
     - `observed_at` = now, RFC 3339.
   - Look at how `broker.rs::record_diagnostic` filled the same struct before W6 deletes it, and match it.
   - `DaemonRejected` for failed bindings carries the binding's message as `diagnostic_first_line`.
   - `Freeze` arms call `crate::host_verify::freeze::apply_freeze_result`.
7. **In-session `loom stage complete`** (`complete.rs`, `control_session.rs`, `knowledge_complete.rs`, `complete_pipeline.rs`):
   - In `Relay` mode (`relay::emit::mode`) for a stage or knowledge session:
     - Refuse `--no-verify`, `--force-unsafe` and `--assume-merged` with the existing message for sandboxed privileged flags.
     - Refuse a dirty tree with `Commit first: completion verifies committed content only.` (exit 1).
     - Run `host_verify::wait::wait_or_request` with kind `Complete`, the checkout being the worktree root or, for a knowledge session, the repository root.
     - On `Finished` with `Passed`, poll the stage file up to the remaining budget:
       - `Completed` → print `Stage '<stage>' was completed by the daemon. End the session now.`, exit 0;
       - a `DaemonRejected` checkpoint newer than the job → print its diagnostic, exit 1;
       - neither → exit 3 with the pending text.
     - Other outcomes take the codes of common.md 3.7.
   - Delete from your files everything that existed only for the broker:
     - `handle_broker_request`, `load_and_run_broker`, `read_broker_input*`, `require_wrapper_identity` (if only the broker used it) in `control_session.rs`;
     - `emit_control_completion_evidence` and the `control_session` threading in `complete_pipeline.rs`;
     - `print_sandboxed_completion_pending_notice` and the in-session `after_failed_completion` impact printing in `complete.rs`;
     - `verify_knowledge_for_broker` in `knowledge_complete.rs`.
   - Keep `sandbox_control_session`'s identity checks if the new route still needs them (session, stage and worktree agreement); rename it if its name now misleads.
   - The operator path (no session) is unchanged.
   - `control_session.rs` is 393 lines; it should shrink.
8. **Tests.**
   - `tests_apply.rs`:
     - passing job completes (stage `Completed`, `completed_commit` = head, receipt persisted, session record `Completed`);
     - failing job writes `ToolFailed` with every label;
     - moved head writes `DaemonRejected`;
     - policy amended after the run writes `DaemonRejected`;
     - session no longer the stage's session writes `DaemonRejected`;
     - knowledge stage sets `merged`;
     - a `Check` record is `Ignored`.
   - `tests_child.rs`: a `Complete` job on a committed fixture passes; a dirty tree fails at capture.
   - `commands/stage/tests/complete.rs` and `complete_knowledge_tests.rs`: drop tests that pinned evidence output (`verified_evidence_record_has_broker_boundaries` and similar). Add one per refusal (privileged flag, dirty tree) using a hand-built `EnvSnapshot` (`relay/emit.rs` returns an empty one under `cfg(test)`; build the `RelayMode` by hand as other relay tests do).

## Proof (run once, from `loom/`, after W5 and W6 have also written; if the build fails only on their missing items, report that and stop)

```bash
cargo test --lib host_verify:: commands::stage:: relay:: orchestrator::core::inbox_drain:: 2>&1 | tail -40
```

## Report

Files changed and deleted; how `complete_verification::run` reports per-check outcomes; the receipt and checkpoint fields; every broker reference you removed (`file:line` before); the proof output.
