# W1 — Claude worker lifecycle evidence

Plan stage: worker evidence foundation. Lane: Codex `gpt-5.6-sol`, effort `xhigh`. Run before W2 in the same stage.

Implement the versioned Loom-owned lifecycle model, locked journal, Claude `SubagentStop` and `TeammateIdle` producers, and exact-identity classifier consumers. The source root was renamed to `loom-hooks/` on 2026-09-13. Edit only the paths listed here.

Do not run git. Do not run the full test suite, formatter, linter, or type checker. You may run one narrow check once if certain. Report files changed, assumptions, and unresolved issues.

## Owned paths

New:

- `loom/src/subagent_lifecycle/mod.rs`
- `loom/src/subagent_lifecycle/model.rs`
- `loom/src/subagent_lifecycle/store.rs`
- `loom/src/subagent_lifecycle/claude.rs`
- `loom-hooks/teammate-idle.sh`
- `loom/tests/worker_evidence.rs`

Existing:

- `loom-hooks/subagent-stop.sh`
- `loom-hooks/subagent-start.sh`
- `loom/src/commands/subagents/classify.rs`
- `loom/src/commands/subagents/classify/entry.rs`
- `loom/src/commands/subagents/summary.rs`
- `loom/src/commands/subagents/render.rs`
- `loom/src/commands/subagents/table.rs`
- `loom/src/commands/subagents/ledger.rs`
- `loom/src/commands/subagents/ledger_tests.rs`
- `loom/src/commands/subagents/classify_tests.rs`
- `loom/src/commands/subagents/classify_recovery_tests.rs`
- `loom/src/hooks/config.rs`
- `loom/src/hooks/tests.rs`
- `loom/src/fs/permissions/constants.rs`
- `loom/src/fs/permissions/tests/constants_tests.rs`
- `loom/src/fs/permissions/tests/hooks_tests.rs`

Do not edit `loom/src/lib.rs`; W2 registers the module after this foundation exists. Do not edit `loom-hooks/tests/run-all.sh`; W2 owns the shared runner.

## Contracts

In `model.rs`, define serde types with explicit version and tagged variants:

- `WorkerIdentity::ClaudeSubagent { stage_id, loom_session_id, parent_session_id, agent_id, agent_type, transcript_path }`
- `WorkerIdentity::ClaudeTeammate { stage_id, loom_session_id, parent_session_id, team_name, teammate_name }`
- `WorkerIdentity::Codex { stage_id, loom_session_id, parent_session_id, forwarder_agent_id, unit_id, invocation_id, workspace_root, execution }`, where `CodexExecution` is `Companion { job_id }` or `Direct { thread_id, tool_use_id }`. Add `LifecycleProducer::{ClaudeSubagentStop, ClaudeTeammateIdle, CodexCompanion, CodexDirect}`. W1 owns the complete shared schema; W2 must not extend it.
- `LifecycleState::{TurnFinished, Idle, Running, Completed, Failed, Cancelled, Unknown}`.
- `LifecycleRecord { version: u16, event_id: String, producer, identity, observed_at, state, evidence: serde_json::Value }`. Replay validates evidence by producer: Claude carries transcript byte length and final-record digest; Codex carries requested model/effort, exact job/thread/turn ids, terminal timestamp, and bounded outcome. Unknown versions remain Unknown.
- `WorkerOutcome::{Active, Succeeded, Failed(String), Cancelled(String), Unknown(String)}` so callers cannot collapse terminal failure into success.

In `store.rs`, expose read/replay plus one crate-private trusted append API, never a public CLI:

- `pub(crate) fn append_locked(work_dir: &Path, record: &LifecycleRecord) -> Result<AppendOutcome>`.
- `pub fn replay(work_dir: &Path) -> Result<LifecycleIndex>`.
- `LifecycleIndex::claude_outcome(stage_id, loom_session_id, parent_session_id, agent_id, transcript_path) -> WorkerOutcome`.
- `LifecycleIndex::forwarded_outcome(stage_id, loom_session_id, parent_session_id, forwarder_agent_id) -> WorkerOutcome`. A matching authorization makes the real companion/direct invocation outcome authoritative for that forwarder. Its own Claude stop cannot settle it. Missing or conflicting child evidence stays Unknown. Test two parallel forwarders with explicit unit IDs and reversed finish order.

Journal path is `.loom/work/subagents/<stage>/lifecycle.jsonl`. Use the repository's lock-directory and symlink-refusal conventions; cap reads and ignore only a torn final line. Duplicate `event_id` with identical content is idempotent; the same ID with different content is a conflict and yields `Unknown`, never last-writer-wins. Never infer identity from mtime or basename alone. Claude event IDs include transcript length and final-record digest; growth after the stop invalidates that turn’s terminality. Codex IDs include invocation and job identity. Duplicate delivery never increments a distinct-attempt count.

In `claude.rs`, parse and validate raw hook payloads:

- `validate_subagent_stop(payload, env, starts, active_stage_session) -> Result<LifecycleRecord, ClaudeEvidenceError>`.
- `validate_teammate_idle(payload, env, active_stage_session) -> Result<LifecycleRecord, ClaudeEvidenceError>`.

The Claude parent UUID and Loom runtime session ID are distinct. For stop, require `payload.session_id == starts.parent_session_id`, `starts.loom_session_id == LOOM_SESSION_ID`, matching stage/agent/type, and current stage→Loom-session binding. Read the worker path from `agent_transcript_path`; `transcript_path` is the parent transcript. Require its `agent-<id>.jsonl` basename to agree with direct `agent_id`. Reject stale, malformed, mismatched, unsafe, symlinked, and cross-stage data. For idle, record `(parent UUID, team_name, teammate_name)` as `Idle`; it is not terminal and cannot settle a wait.

Update the renamed shell hooks to emit the validated versioned shape through trusted hook-side file writes. Do not add `loom lifecycle ingest`: stage agents can invoke Loom commands and must not be able to forge completion. Preserve heartbeat locking and require the active Loom-session ownership check before a stop/idle refresh. A delayed predecessor event neither writes authoritative success nor refreshes its successor.

Replace `classify::has_authoritative_termination`, which currently scans every stage for `<agent>.json`, with exact lifecycle lookup using the resolved transcript's parent UUID, active stage, Loom session, agent ID, and normalized transcript path. Extend `SubagentState` for failed and cancelled terminal outcomes; update every exhaustive/silent consumer in `classify/entry.rs`, `summary.rs`, `render.rs`, and `table.rs`. `watch` succeeds only when every owned worker succeeded, returns a distinct nonzero terminal result immediately for failure/cancellation, and times out on unknown. `harvest` prints explicit terminal failure evidence rather than “nothing harvestable.” Keep text debounce solely for clearly labeled legacy diagnostic display. It cannot settle an owned-worker wait or authorize reassignment. Add `LifecycleIndex::outcome(&WorkerIdentity) -> WorkerOutcome` and bind the classifier to it for Claude and Codex; W2 records reach that same consumer without W1 importing the adapter.

Update start-ledger indexing so exact `(stage,parent UUID,Loom session,agent)` resolution is reusable by the lifecycle validator. Preserve the existing rule that ambiguous legacy `session_id` rows never become join evidence.

Register `TeammateIdle` beside `SubagentStop` in `HookEvent`, `Display`, `script_name`, `all`, generated settings, embedded hook constants, and registration-consistency tests. Current install closure is pinned by `LOOM_HOOKS` and `fs/permissions/tests/{constants_tests,hooks_tests}.rs`; missing any site makes the hook dead.

## Regression proof to add

`loom/tests/worker_evidence.rs` must construct generated session settings, invoke the installed hook scripts with documented payloads where parent and worker transcript paths differ, and then exercise the real classifier/list/watch path. Cover exact success, parent UUID distinct from Loom session, wrong active Loom session, stale stop replay, wrong stage/type/path/id, malformed and duplicate/conflicting events, delayed heartbeat protection, and teammate idle remaining nonterminal. Use joined processes and fixture clocks; never detach a child. Cargo discovers this integration target automatically.

Official contracts: <https://code.claude.com/docs/en/agent-sdk/typescript#subagentstophookinput> and <https://code.claude.com/docs/en/agent-sdk/typescript#teammateidlehookinput>.

Done means all owned consumers use exact identity, the documented producer path reaches the journal and classifier, stale evidence fails closed, and W2 can register `pub mod subagent_lifecycle` without changing W1 files.
