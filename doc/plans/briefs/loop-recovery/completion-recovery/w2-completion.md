# W2 — Exact completion bridge, durable evidence, and confined environment

Plan: `doc/plans/PLAN-loop-recovery.md` · Stage: `completion-recovery` · Wave 2 (after W1; parallel with W3 and W4).
Lane: Codex `gpt-5.6-sol`, effort `xhigh`, foreground timeout 600000 ms.

## Boundary

Own only the files listed below. Do not edit W3's recovery/graph/monitor files or W4's daemon wire tests. Do not run git. The orchestrator verifies and commits.

This brief uses the `loom-hooks/` source root, renamed on 2026-09-13. Preserve the exact-command authorization pin and commit `b2cedf776aaa49e66d39ba6dcfea9a01983fdf7c`, which stopped exporting sccache into sandboxed session wrappers.

W1 lands first and owns the shared recovery contract in `w1-checkpoints.md`:

- `loom/src/models/session/types.rs`: `SessionExitReason` and optional `Session.exit_reason`.
- `loom/src/fs/session_files/exact.rs`: `mark_session_terminal_reason(work_dir, session_id, status: SessionStatus, reason: SessionExitReason) -> Result<()>`.
- `loom/src/handoff/schema/completion.rs`: `CompletionCheckpoint`, `VerificationCheckpoint`, `CompletionBlocker`, `CompletionAttemptEvidence`, and `CompletionBlocker::from_attempt`.
- `HandoffV2`/`HandoffContent` gain optional `completion_checkpoint`.
- `loom/src/handoff/generator/mod.rs`: `merge_session_handoff(session, stage, origin, incoming, work_dir) -> Result<(PathBuf, MergeOutcome)>` performs semantic upsert.

Re-read W1's committed brief/code before editing. Construct `CompletionAttemptEvidence` and use these exact exports; do not recreate any W1 type or helper in W2.

- W2 produces checkpoints. W3 consumes them in monitor/recovery and parks repeated identical trusted external failures. W4 renders them and owns daemon wire-test changes.

## Files

Modify:

- `loom-hooks/loom-control-complete.sh`
- `loom-hooks/tests/loom-control-complete.sh`
- `loom-hooks/tests/loom-control-tokenized-prefilter.sh`
- `loom/src/process/environment.rs`
- `loom/src/verify/criteria/tests/confine_tests.rs`
- `loom/src/commands/stage/mod.rs`
- `loom/src/commands/stage/complete.rs`
- `loom/src/commands/stage/control_complete.rs`
- `loom/src/commands/stage/control_session.rs`
- `loom/src/daemon/protocol.rs`
- `loom/src/daemon/server/mod.rs`
- `loom/src/daemon/server/client.rs`
- `loom/src/daemon/server/self_service.rs`
- `loom/src/daemon/server/control_complete.rs`
- `loom/src/daemon/server/tests/self_service_client.rs`

Create:

- `loom/src/commands/stage/completion_evidence.rs`
- `loom/src/daemon/server/completion_evidence.rs`
- `loom/src/daemon/server/completion_dispatch.rs`
- `loom/tests/completion_replay.rs`

Untouched but read for contracts: `loom-hooks/_common.sh`, `loom-hooks/tests/run-all.sh`, `loom/src/daemon/wire.rs`, `loom/src/fs/locking.rs`, `loom/src/fs/stage_request/{mod.rs,spool.rs}`, `loom/src/fs/permissions/{constants.rs,hooks/config.rs,tests/hooks_tests.rs}`, `loom/src/verify/criteria/confine.rs`, `loom/src/orchestrator/terminal/native/{build_cache.rs,wrapper.rs,tests_wrapper_env.rs,spawner.rs}`, `loom/src/orchestrator/terminal/tmux/mod.rs`, `loom/src/handoff/schema/completion.rs`, and `loom/src/handoff/generator/mod.rs`.

## 1. Exact attempt recognition

Keep one broad fail-safe detector and one narrow authorizer in `loom-control-complete.sh`.

The detector tokenizes through `_common.sh`'s existing tokenizer, scans every command position (including after assignments and separators), and recursively tokenizes one `sh|bash|zsh -c|-lc|-cl` payload. Its result must distinguish: unrelated command, exact pinned command, unsupported completion attempt with a reason. A malformed token stream containing loom/stage/complete indicators is unsupported, never unrelated.

The only authorized bytes remain:

    <resolved non-symlink trusted absolute loom> stage complete <LOOM_STAGE_ID>

PreToolUse rejects every other actual attempt with exit 2 and a clear `LOOM_CONTROL_ERROR`: relative `loom`; `env -u RUSTC_WRAPPER`; `NAME=value`; pipeline/separator/redirection/background/newline; shell `-c` wrapper; variable/symlink/other binary; quoted, concatenated, escaped, or line-spliced verb/path; extra flags or arguments. Quoted prose and a path argument merely mentioning the words remain unrelated and allowed. Do not normalize or newly accept wrappers: trusted confined-environment normalization below makes `env -u` unnecessary.

PostToolUse repeats exact equality. It must report command error, missing/invalid marker, durable evidence failure, daemon transport/rejection, and verified-but-unacknowledged completion separately. Preserve the validated Claude persisted-output path checks.

## 2. Evidence and completion transport

Extract the marker into a bounded, structured EOF record emitted only after all acceptance and goal-backward checks return green:

    LOOM_CONTROL_EVIDENCE_V1 {single-line JSON}
    LOOM_CONTROL_EVIDENCE_EOF

The JSON carries stage id, session id, current commit, a fresh evidence nonce, the verification/check-definition fingerprint, and outcome `verified_pending_ack`. The hook accepts exactly one record immediately followed by the EOF marker; reject duplicate records, trailing marker-like text, invalid ids/nonces, mismatched wrapper identity, non-current commit, or wrong fingerprint. Keep ordinary criterion output untrusted: a forged marker must never authorize mutation.

Use W1's exact evidence/checkpoint schema and closed phase enum. The accepted receipt is written only by successful daemon mutation. Preserve the attempt nonce through phase updates; stable blocker identity excludes phase. Tool failures and missing evidence never become actionable verified external failures.

Add these APIs, adjusted only to W1's exact type/module names:

- `completion_evidence::record_host_fallback(session: &Session, stage: &Stage, evidence: CompletionAttemptEvidence, work_dir: &Path) -> Result<(PathBuf, MergeOutcome)>`, delegating to W1's `merge_session_handoff`
- `completion_evidence::send_to_daemon(evidence: &CompletionAttemptEvidence, work_dir: &Path) -> Result<Response>`
- `daemon::server::completion_evidence::handle_record(work_dir: &Path, stage_id: &str, session_id: &str, evidence: CompletionAttemptEvidence) -> Result<Response>`, converting through `CompletionBlocker::from_attempt` and `merge_session_handoff`
- `commands::stage::control_complete::send_completion(stage_id: &str, session_id: &str, completion_nonce: &str, evidence_nonce: &str, work_dir: &Path) -> Result<()>`

The trusted host hook invokes the fixed Loom binary for evidence recording. The sandboxed stage CLI never writes shared state. First attempt the daemon RPC. If the daemon is offline or its ACK is lost, the host-side broker durably semantic-upserts the same checkpoint through W1's locked handoff helper with `HandoffOrigin::CompletionEvidence`; it must not append duplicates or downgrade a richer checkpoint. The fallback is available only in broker mode after wrapper stage/session identity, current worktree membership, EOF evidence, commit, and nonce validation. It is not a general CLI verb.

Add `Request::RecordCompletionEvidence` deliberately to protocol capability, credential redaction, server dispatch, and `self_service_session`. Validate active stage/session ownership. The daemon persists through W1's semantic-upsert helper. The diagnostic request must never call `try_complete`, alter stage status, consume a completion nonce, trigger dependents, or authorize `CompleteStage`.

For verified completion, record `verified_pending_ack`, then send `CompleteStage` with separate completion and evidence nonces. Extend `Request::CompleteStage` and its handler together: under the same transition lock, require a trusted verified checkpoint matching stage/session/current commit/check definitions/evidence nonce before `try_complete`, retain existing peer/replay checks, and persist the accepted receipt. A client-only nonce field is insufficient. Lock order is session transition lock before handoff lock; fallback takes only the handoff lock.

An explicit daemon `Response::Ok` is accepted. Lost ACK is ambiguous: reconcile exact durable completed stage plus accepted evidence receipt. If both prove this attempt committed, report acceptance reconciled from durable state; if still Executing, preserve verified-pending/transport-unknown detail; if inconsistent, report uncertainty. Never overwrite Completed with a rejected blocker. Rejection before dispatch leaves Executing. W3 owns all parking decisions.

Security invariant: untrusted diagnostics cannot create verified authority, complete or park a stage. Successful mutation requires exact pin, trusted binary/EOF evidence, stage/session/commit/check-definition binding, distinct nonces, peer identity, and the daemon's locked decision. The hook's acceptance message requires its ACK or exact durable accepted receipt.

## 3. Confined environment

Remove ambient `RUSTC_WRAPPER` from `process::environment::STAGE_HOST_ENV_ALLOWLIST`. Keep `SCCACHE_DIR` and `SCCACHE_CACHE_SIZE`: inert without a selected wrapper and still useful when launch explicitly adds the approved wrapper. Do not change `apply_stage_environment`'s signature; all callers remain compiler checked. Update the old survival test to pin exclusion.

This closes the remaining path: `verify::criteria::confine::spawn_confined` calls `apply_stage_environment`, while the existing launch fix only governs wrapper generation. Preserve `build_cache::sccache_usable_in`, `rustc_wrapper_candidate`, the `rustc_wrapper_allowed` carrier, and wrapper tests.

## 4. Regressions

Shell tests drive the installed hook script itself. For each unsupported family, assert PreToolUse exit 2, reason-bearing `LOOM_CONTROL_ERROR`, and no broker/evidence call. Preserve prose/path non-attempts. PostToolUse cases assert tool failure and missing/duplicate/trailing/mismatched EOF records produce durable diagnostics and never `CompleteStage`.

`loom/tests/completion_replay.rs` exercises the real built Loom verification producer and installed hook parser with temporary roots and joined children. Capture actual EOF evidence and malformed variants. Real daemon-offline fallback exercises persistence without a socket. This is one segment of a composed proof, not a socketless hook-to-daemon subprocess chain.

Extract `completion_dispatch.rs`: a transport-independent authenticated dispatcher called by real `client.rs` after existing authentication and peer checks. Module tests feed actual serialized requests through in-memory framing and this dispatcher, using an authorization context whose production constructor is private to the authenticated connection handler. Assert real stage/receipt mutation and all rejection branches. Do not add a test-auth CLI or weaken peer checks. Connect the proof segments using byte-for-byte fixtures of the real producer/hook request shape. W4 owns codec tests; W2 owns dispatch tests beside its new module. Real Unix-connect/peer-credential smoke may skip under this sandbox; core framing, authorization decisions and state assertions never skip. UnixStream::pair is still a socket.

Required replay cases:

1. Producer/hook segment: exact command produces real EOF evidence and a durable verified checkpoint. Dispatch segment: that request shape reaches the actual authenticated completion handler, Completed state, accepted receipt and Response::Ok. Shell ACK-parser tests cannot substitute for this mutation test.
2. Acceptance failure using a real failing criterion → `tool_failed`; no completion request and stage remains Executing.
3. Fake executable ambient `RUSTC_WRAPPER` exits with EPERM text while the real criterion beneath it succeeds. Drive actual `spawn_confined`, join it, assert criterion sentinel exists and wrapper invocation sentinel does not.
4. Rejection or loss before dispatch leaves Executing; loss after committed completion preserves Completed and reconciles the exact accepted receipt. Offline fallback retains session/commit/fingerprint. Duplicate delivery never increments attempts; two distinct failed attempt nonces do. Lost ACK is never proof that mutation did not happen.
5. Forged success marker/JSON, stale session, cross-stage, wrong commit, replayed nonce, and evidence nonce used as completion nonce: none mutates lifecycle.
6. Restart/readback: W1's handoff parser returns the checkpoint W3 will consume; a successor session does not inherit authority from predecessor evidence.

Add focused unit cases beside `completion_evidence.rs`, protocol/self-service, and confined environment. Test exact function names chosen by implementation; do not invent module filters in plan acceptance.

## Done

- Exact command pin preserved; unsupported real attempts fail clearly.
- Verification evidence is EOF-delimited, identity/commit/fingerprint bound, durable across daemon offline/lost ACK, and cannot authorize mutation.
- Acceptance comes from real daemon ACK or exact durable reconciliation of its accepted receipt.
- W1 checkpoint semantic upsert is used; W3/W4 files remain untouched.
- Ambient `RUSTC_WRAPPER` cannot enter confined criteria; launch-owned sccache policy remains intact.
- Composed replay covers the real producer/installed hook and actual authenticated dispatcher in separate joined segments. Core assertions never skip; live Unix transport is a separate platform smoke.
