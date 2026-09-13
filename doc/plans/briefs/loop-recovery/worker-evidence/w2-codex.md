# W2 — Correlated Codex job lifecycle evidence

Plan stage: worker evidence, sequential after W1. Lane: Codex `gpt-5.6-sol`, effort `xhigh`.

Add a separate Codex adapter and daemon reconciliation path that correlates one authorized logical unit with one real companion job, copies terminal evidence into Loom's lifecycle journal, and wires W1's module into the crate. Hook sources live under `loom-hooks/`; edit only the listed paths.

Do not run git. Do not run the full test suite, formatter, linter, or type checker. You may run one narrow check once if certain. Report files changed, assumptions, and unresolved issues.

## Owned paths

New:

- `loom/src/codex_lifecycle.rs`
- `loom/tests/codex_evidence.rs`

Existing:

- `loom/src/lib.rs`
- `loom/src/orchestrator/monitor/core.rs`
- `loom-hooks/codex-forward-guard.sh`
- `loom-hooks/codex-forward.sh`
- `loom-hooks/tests/codex-forward-records-model.sh` (current counterpart)
- `loom-hooks/tests/codex-forward-wrapper.sh` (current counterpart)
- `loom-hooks/tests/run-all.sh` (current counterpart; add both workers' new shell cases if any)
- `loom/src/commands/status/data/execution_models.rs`
- `agents/loom-codex-forwarder.md`
- `loom/src/orchestrator/signals/format/codex.rs`
- `loom/src/orchestrator/signals/tests_doctrine.rs`
- `loom/src/orchestrator/signals/tests_cache.rs`
- `loom-hooks/codex-forward-result.sh` (new trusted terminal-result producer)
- `loom-hooks/_codex-direct.py` (new bounded direct-process supervisor; uses existing Python 3)
- `loom/src/fs/permissions/constants.rs` (sequential handoff from W1)
- `loom/src/fs/permissions/hooks/config.rs`
- `loom/src/fs/permissions/tests/constants_tests.rs` (sequential handoff from W1)
- `loom/src/fs/permissions/tests/hooks_tests.rs` (sequential handoff from W1)

Do not edit any `loom/src/subagent_lifecycle/**` file or W1 test. Import its public/crate-private contracts.

## Grounded companion contract

The installed companion inspected for this plan is plugin `codex` v1.0.6:

- `scripts/codex-companion.mjs::handleTask` builds a unique task job; background mode returns its exact job ID, foreground mode still calls `runForegroundCommand` with that job.
- `scripts/lib/tracked-jobs.mjs::runTrackedJob` writes `queued/running`, then terminal `completed` when `exitStatus == 0`, otherwise `failed`; exceptions write `failed` plus `errorMessage`. Terminal records include `threadId`, `turnId`, `completedAt`, `result`, and `rendered`.
- `scripts/lib/state.mjs::{resolveStateDir,resolveJobFile}` keys state by canonical workspace-root hash under `CLAUDE_PLUGIN_DATA/state/<slug-hash>/jobs/<id>.json`; the summary retains at most 50 jobs and removes pruned job files.
- `scripts/codex-companion.mjs::buildTaskRequest` stores requested `model`, `effort`, `write`, prompt, resume flag, and job ID. `executeTaskRun` passes model/effort to `runAppServerTurn`; this is requested-model evidence, not a provider echo.

Parse this as a versioned adapter. Unknown plugin version, missing/pruned job, malformed JSON, unsupported status, or identity mismatch yields `WorkerOutcome::Unknown`; never infer completion from wrapper exit, newest mtime, final text, or model-ledger presence.

## Contracts and implementation

In `codex_lifecycle.rs`, define:

- `CodexAuthorization { stage_id, loom_session_id, parent_session_id, forwarder_agent_id, unit_id, invocation_id, model, effort, authorized_at, selected_companion, companion_version, effective_state_root }`. The guard records the forwarder agent id from the exact start row. The selected canonical companion and its manifest version are validated host-side; job JSON itself has no plugin-version field. For Loom forwards, choose the canonical `~/.codex/plugin-data` root before launch and bind it into authorization. Require the wrapper and every companion task/status invocation to use that exact `CLAUDE_PLUGIN_DATA`; remove the sandbox-dependent mkdir/fallback selection. If the fixed allowed root is unusable, fail before launching a job. Test host-writable/child-denied input paths and prove authorization and lookup roots still agree.
- `CompanionJob { id, session_id, workspace_root, job_class, write, request, status, thread_id, turn_id, completed_at, error_message, result }` with strict custom validation over the actual camelCase JSON.
- `pub fn reconcile_codex_jobs(work_dir: &Path, active_sessions: &[Session]) -> Result<ReconcileReport>`.
- `pub fn companion_outcome(work_dir: &Path, identity: &CodexAuthorization) -> WorkerOutcome`.

Add optional `--unit-id` to the exact forward command and guard parser. Updated orchestrator prompts supply stable unique unit IDs matching `[A-Za-z0-9._-]+`; legacy callers derive a unit from the exact forwarder agent id. Independently generate a fresh invocation nonce, so retrying one logical unit never revives its previous job. Update the forwarder agent and generated Codex doctrine together. Guard ledger rows include stage, Loom session, Claude parent UUID, unit, invocation, requested model/effort, canonical worktree, and authorization time. Set `CODEX_COMPANION_SESSION_ID` to an unambiguous encoding of that identity before invoking the companion. Use companion `task --background --json` to obtain its exact job ID, then `status <job-id> --wait --json`. Only the wrapper backgrounds the internal companion; the forwarder still makes one foreground Bash call. Preserve the final evidence trailer with exact unit, invocation and job.

Pass `--timeout-ms 540000` to companion status wait, leaving time within the outer 600000 ms budget. Parse `waitTimedOut` explicitly: an exit-zero active snapshot is Active, never terminal success. After it returns, daemon reconciliation continues owning the exact job. No model-driven status loop is permitted. Supported manifest versions are explicit adapter entries; unknown versions fail before forwarding instead of parsing against v1.0.6 assumptions.

Preserve the macOS direct branch with one foreground Bash call. `_codex-direct.py` supervises the exact `codex exec --json` child in its own tracked process group, streams bounded structured output, and waits at most 540000 ms. On timeout it terminates that owned group, allows a 5000 ms grace, then kills/reaps and verifies no tracked descendant survives. A failed signal/probe or survivor yields Unknown with retained ownership, never terminal success. Only a joined successful child with its structured terminal event is Succeeded; a confirmed timeout retirement is Cancelled. Do not require a second TaskOutput call. The forwarder remains Bash-only and returns once.

Bind thread/turn/tool-use identity and invocation nonce to that exact command and joined result; replace newest-rollout selection. Trusted `codex-forward-result.sh` handles the final Bash PostToolUse result, validates pinned wrapper command/start/authorization/persisted-output provenance/EOF trailer and the supervisor result, then appends `CodexDirect` evidence. Embed/install the supervisor and hook through the listed permission files. If the outer tool nevertheless returns early, that envelope cannot establish terminality: retain ownership/Unknown until daemon reconciliation sees the exact protected outer tool result. It does not schedule another forwarder tool call. Missing/conflicting/incomplete final results stay Unknown. Test normal direct completion, inner timeout, signal failure, survivor and unexpected outer early-return using a fake Codex executable on Linux; live macOS remains a separate platform smoke.

The daemon owns durable reconciliation. Wire `reconcile_codex_jobs` into `orchestrator::monitor::Monitor::poll` before subagent/session state classification, passing its real `work_dir` and loaded active sessions. It reads authorization rows, locates the companion job by exact encoded session identity, validates canonical worktree, `jobClass == "task"`, `write == true`, request job/model/effort, and terminal status, then uses W1's trusted locked append to copy a `CodexCompanion` lifecycle record. It must be bounded and best-effort per poll: one malformed/missing external record produces an observable unknown/report entry without failing the whole daemon tick. Journal writes occur only in the host daemon, never from the sandboxed wrapper or a public CLI.

Map companion states: queued/running → active; completed → succeeded; failed/cancelled → their distinct terminal outcomes; lost/pruned/mismatch → unknown. A wrapper/Claude-forwarder turn ending never releases the unit while its companion job is active. Restart replay reconstructs authorization→job→terminal state from durable ledgers. Duplicate reconciles are idempotent by deterministic event ID; contradictory terminal records fail closed.

Update `execution_models_for_stage` to read the requested model from correlated authorization/lifecycle evidence while retaining safe legacy display fallback. Never label a requested model as provider-observed. This consumer is currently silent JSON parsing, so malformed or unmatched rows must not gain attribution.

Register `pub mod subagent_lifecycle;` and `pub mod codex_lifecycle;` in `loom/src/lib.rs`. W1 completed the former before this sequential worker starts.

## Regression proof to add

`loom/tests/codex_evidence.rs` must use a fake companion state root matching v1.0.6 on-disk shape and drive the real guard/wrapper adapter plus `Monitor::poll` reconciliation. Cover two parallel units under one parent, two parents in one cwd, exact job selection despite newer unrelated mtimes, running across wrapper timeout, completed with exact requested model/effort and thread/turn IDs, failed, cancelled, malformed, pruned/lost, wrong worktree/session/unit/model, duplicate poll, contradictory replay, and daemon restart. Assert no Done/harvest/file release before real completed state and immediate explicit terminal failure afterward. Fake children must be joined.

Also pin the v1.0.6 schema assumptions in fixtures so a companion upgrade fails visibly at the adapter boundary instead of silently changing classification.

Done means each Codex unit has a durable exact job identity and requested-model record, real child terminal state controls lifecycle, daemon restart preserves it, and no sandboxed path can write shared lifecycle state.
