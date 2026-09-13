# W1 — Session reasons and faithful completion checkpoints

Lane: Codex `gpt-5.6-sol`, effort `xhigh`. Foundation wave; W2, W3, and W4 consume this finished contract in the same worktree. Do not run git. Own only the files listed below.

## Goal

Persist why a session ended separately from `SessionStatus`, and make the existing exact-session V2 handoff a monotonic, idempotent completion checkpoint. Keep `SessionStatus` and `StageStatus` unchanged. The sandbox completion CLI does not write this state; W2 supplies trusted evidence and W3 decides lifecycle transitions.

## Owned files

- `loom/src/models/session/types.rs`
- `loom/src/models/session/mod.rs`
- `loom/src/commands/stage/session.rs`
- `loom/src/commands/stage/tests/session.rs`
- `loom/src/models/session/methods.rs`
- `loom/src/models/session/transitions.rs`
- `loom/src/models/session/tests/helpers.rs`
- `loom/src/models/session/tests/session_status_transitions.rs`
- `loom/src/models/session/tests/session_transitions.rs`
- `loom/src/models/session/tests/session_workflows.rs`
- `loom/src/fs/session_files.rs`
- `loom/src/fs/session_files/exact.rs`
- `loom/src/handoff/schema/mod.rs`
- `loom/src/handoff/schema/types.rs`
- `loom/src/handoff/schema/v2.rs`
- `loom/src/handoff/schema/completion.rs` (new)
- `loom/src/handoff/generator/content.rs`
- `loom/src/handoff/generator/mod.rs`
- `loom/src/handoff/generator/lookup.rs`
- `loom/src/handoff/generator/formatter.rs`
- `loom/src/handoff/generator/tests.rs`
- `loom/src/handoff/mod.rs`
- `loom/src/orchestrator/continuation/context.rs`
- `loom/src/orchestrator/continuation/tests.rs`
- `loom/src/commands/handoff/create.rs`
- `loom/src/commands/handoff/create/tests.rs`
- `loom/src/orchestrator/monitor/handlers.rs`

## Contract

Add optional `Session.exit_reason: Option<SessionExitReason>` with kebab-case serde and variants `Completed`, `Crashed`, `ContextCeiling`, `Stalled`, `OperatorStop`, `CriteriaBlocked`, `Replaced`. Old session markdown must deserialize with `None`. Status still controls terminality and retry classification.

Export `SessionExitReason` through `models/session/mod.rs`. Ordinary `mark_completed`/`mark_crashed` transitions set their corresponding reason when absent; special retirement receives an explicit reason. First terminal reason wins and delayed generic events cannot overwrite it.

Route `commands/stage/session.rs::cleanup_session_resources` through the exact terminal-reason primitive, replacing its private direct Completed assignment. Both ordinary and knowledge completion use this helper. Test the real cleanup path and disk readback as `Completed/Completed`.

Add `mark_session_terminal_reason(work_dir, session_id, status, reason) -> Result<()>` in `fs/session_files/exact.rs`. It validates exact identity, re-reads under the session-directory lock, applies a valid terminal transition to a nonterminal record, never changes an already-terminal status, and fills `exit_reason` only when absent. Keep `mark_session_context_exhausted` as a compatibility wrapper until W3 migrates all production callers.

Create `handoff/schema/completion.rs` with `CompletionCheckpoint`, `VerificationCheckpoint`, `CompletionBlocker`, and `CompletionAttemptEvidence`. The exact shared field contract is:

- `CompletionAttemptEvidence`: version, stage_id, session_id, commit, check_definition_hash, exact_command, evidence_nonce (fresh attempt ID), verification, phase, external_failure_code, diagnostic_first_line, observed_at. Identity and nonce are nonempty validated strings; phase is a closed enum. Verification contains stable criterion IDs/results and bounded non-secret environment facts, not arbitrary environment variables.
- Phases: `ToolFailed`, `EvidenceMissing`, `VerifiedPendingAck`, `DaemonRejected`; an optional durable accepted receipt is separate. Only verified attempts with an external boundary failure code are actionable. ToolFailed/EvidenceMissing are diagnostic only.
- `CompletionBlocker::from_attempt` hashes length-prefixed canonical stage/session/commit/check-definition/command/criterion/environment-policy/failure-code fields with SHA-256. Exclude transport phase, nonce, prose and timestamps. Phase updates for one attempt never replace its identity or increase repeat count. Distinct failure codes/commits/check definitions create distinct blockers.
- `CompletionCheckpoint` retains current verification, bounded per-fingerprint distinct evidence nonces, first/last observation, current phase, blocker and optional accepted receipt `{ evidence_nonce, completion_nonce, commit }`. Persist deduplicated counts so restart does not reset escalation; conflicting content for one nonce invalidates actionability. Retain at most 32 distinct nonce entries per session without eviction. After saturation, report diagnostic-capacity exhaustion and require review rather than count untracked deliveries. Phase updates do not consume another slot.
- Define timestamp parsing as RFC3339 UTC and field length/read limits in the shared validator. W2/W3/W4 import these types directly; no independently evolving DTO is permitted.

Add `HandoffOrigin::CompletionEvidence` with its serde spelling `completion_evidence`; W2 always supplies that origin. Update the exhaustive generator match and round-trip tests. Preserve all existing origin semantics.

Add optional `completion_checkpoint` to `HandoffV2` and `HandoffContent`, serde-defaulted. Add `merge_session_handoff(session, stage, origin, incoming, work_dir) -> Result<(PathBuf, MergeOutcome)>`. Under the existing handoff-directory lock, read every valid V2 artifact for the exact stage/session, fold semantic content, and create a numbered atomic artifact only when the merged value changes. Union completed tasks, commits, files and live job ids in stable order; retain the newest non-empty verification; merge evidence for the same blocker fingerprint; a new fingerprint replaces only the blocker while retaining completed work. Empty delayed `session_end` content cannot erase a richer checkpoint. Preserve RedBand's context-token snapshot behavior and legacy no-predecessor lookup.

Update daemon context handoff generation and CLI handoff creation to merge, not blindly append poorer state. The daemon's generic handoff should include stage `completed_commit` when present and existing memory; it must never invent passed gates or completed workers.

## Lifecycle rules

- Creation: W2 creates verified boundary evidence; W1 only validates, hashes, merges, serializes, and retrieves it.
- Reset: a new assigned session starts with `exit_reason: None`; predecessor reasons remain on predecessor records. A changed stage commit creates a distinct fingerprint.
- Invalidation: exact stage/session mismatch, malformed checkpoint, or missing required fingerprint inputs is uncertainty and never eligible for lifecycle action.
- Continuation: `find_continuation_handoff` remains bound to `stage.session`'s outgoing identity. It returns the semantically richest exact-session checkpoint, never merely the highest numbered file.
- Dependencies: W2/W3/W4 use these exports after W1 finishes; read its edited files directly because the graph still shows the branch point. Do not clone private DTOs.

## Regressions and proof

Add tests for old serde, first-reason-wins, terminal-status preservation, deterministic/differentiating fingerprints, rich-stall then empty-SessionEnd merge, duplicate merge producing no artifact, changed blocker preserving completed work, malformed/wrong-session exclusion, and continuation selection.

Optional single scoped check: `cargo test --manifest-path loom/Cargo.toml --lib handoff::`.

Done means public exports compile, every owned direct struct literal is updated, and W2 can construct `CompletionAttemptEvidence` while W3/W4 can read the exact-session checkpoint without owning these files.
