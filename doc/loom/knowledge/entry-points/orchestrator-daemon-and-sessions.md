---
---

# Orchestrator Daemon And Sessions

>

## Orchestrator Core

- `orchestrator/core/orchestrator.rs` - Main loop (5s polling)
- `orchestrator/core/stage_executor.rs` - Worktree creation, signal gen, session spawn
- `orchestrator/core/event_handler.rs` - Dispatches StageCompleted, SessionCrashed, etc.
- `orchestrator/core/crash_handler.rs` - Failure classification, exponential backoff
- `orchestrator/core/completion_handler.rs` - Auto-merge BEFORE marking completed
- `orchestrator/core/merge_handler.rs` - Auto-merge, the resolver spawn loop (resolvers run in the stage worktree) and the blocked-merge retry (`merge_handler/blocked_retry.rs`)
- `orchestrator/core/merge_handler/landing.rs` - `land_stage_merge`: `merge_stage` with the gate enforced, record; `merge_gate.rs` - the fail-open pre-filters; `resolver_spawn.rs` - includes `clean_merge_settled`
- `orchestrator/core/merge_handler/leftover_sweep.rs` - `sweep_merged_leftovers`: finishes cleanup of merged stages each tick and at startup
- `orchestrator/core/inbox_drain/merge_resolved.rs` - `resolve_merge_from_inbox`, with the pure `settle_for_landing` reply mapping
- `orchestrator/merge_lifecycle.rs` - `finish_verified_merge`, `CleanupOutcome::Deferred { reason }`
- `orchestrator/core/persistence.rs` - Load/save state to disk

## Data Models

- `models/stage/types.rs` - Stage struct, StageStatus enum (13 states, including NeedsAdjudication)
- `models/stage/transitions.rs` - State transition validation
- `models/stage/methods.rs` - Stage operations (try_mark_executing, try_complete, timing)
- `models/session/types.rs` - Session struct, SessionStatus enum (6 states)
- `models/failure.rs` - FailureType enum (10 variants, retryable vs non-retryable)

## Daemon

- `daemon/server/core.rs` - `DaemonServer` state and bounded-client constants
- `daemon/server/launch.rs` - starts the daemon child: `spawn_daemon` re-executes the binary as `loom run --daemon-child <root>` and `await_ready` reads its readiness pipe (`0x02` log active, `0x01` ready); see [Daemon Launch and the Socket Path](../architecture/orchestrator-loop.md)
- `daemon/server/lifecycle.rs` - `start` (delegates to `launch`) and `serve`, the daemon child's body: `setsid`, the authoritative singleton lock (held for the process lifetime), pid and token publication, the log redirect, socket binding, accept loop, and shutdown
- `daemon/server/environment.rs` - the allowlisted environment the daemon child starts with (`DaemonEnvironment`)
- `daemon/socket.rs` - `socket_path`, `socket_path_fits`, `socket_path_problem`: the one place a client resolves the socket
- `commands/run/daemon_child.rs` - the hidden `--daemon-child` entry: captures the signing environment, then `DaemonServer::serve`
- `daemon/protocol.rs` - IPC request/response and capability types
- `daemon/wire.rs` - Fixed authentication preface plus bounded JSON framing (64 KiB requests, 2 MiB responses)
- `daemon/server/admission.rs` - Absolute-deadline reads and the global in-flight byte budget
- `daemon/server/pool.rs` - Fixed worker pool and bounded admission queue
- `daemon/server/storage.rs` - No-follow, mode-0600 control-file publication under the mode-0700 `.loom/work/` directory
- `daemon/server/broadcast.rs` - Status/log streaming to clients

## Monitor Subsystem

- `orchestrator/monitor/core.rs` - Coordinates detection, heartbeat, checkpoints
- `orchestrator/monitor/detection.rs` - Stage/session state change detection, budget checks
- `orchestrator/monitor/heartbeat.rs` - Hung detection (300s timeout)
- `orchestrator/monitor/context.rs` - `context_health(tokens, ceiling)`: Green `<60%`, Yellow `60-90%`, Red `>=90%` of the resolved absolute `context_ceiling_tokens` (not a fixed 200k window — see architecture.md "Context Budget Enforcement")
- `orchestrator/monitor/crash_redelivery.rs` - Delivers a reported crash again until its stage has moved on, with doubling waits
- `orchestrator/monitor/foreground.rs`, `foreground_progress.rs` - Bounded foreground work evidence, shared by stall decisions and the activity badge
- `orchestrator/monitor/direct_worker.rs` - Host-observed progress of an authorized direct Codex worker

## Signal System

- `orchestrator/signals/generate.rs` - Signal file creation (generate_signal_with_skills)
- `orchestrator/signals/cache.rs` - Stable prefix generation (4 stage-type variants, SHA-256 hash)
- `orchestrator/signals/format/mod.rs` - Full signal formatting (Manus 4-section KV-cache pattern)
- `orchestrator/signals/format/sections.rs` - Section formatters (stable, semi-stable, dynamic, recitation)
- `orchestrator/signals/format/budget.rs` - `## Model Budget` section: fresh quota windows plus one routing line ([Signal Generation](../architecture/signal-generation.md#model-budget-section-formatbudgetrs))
- `orchestrator/signals/format/skills.rs` - Recommended Skills section: per-skill invocations plus one combined `loom-skills` loader call
- `orchestrator/signals/helpers.rs` - write_signal_file() (disk I/O)
- `orchestrator/signals/types.rs` - EmbeddedContext, DependencyStatus, SandboxSummary
- `orchestrator/signals/knowledge.rs` - generate_knowledge_signal() (knowledge stages)
- `orchestrator/signals/crud.rs` - Signal file CRUD
- `orchestrator/signals/merge.rs` - Resolver signal: the resolver works in the stage worktree, never the main checkout
- `orchestrator/signals/recovery.rs` - Recovery signal generation

## Signal Generation — Key Files

| File                                      | Purpose                                                                                                                   |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `orchestrator/signals/generate.rs`        | Entry point: `generate_signal_with_skills()`; `generate/context.rs::build_embedded_context_with_stage_and_session()`      |
| `orchestrator/signals/cache.rs`           | 4 stable-prefix generators + `append_*` helpers + SignalMetrics SHA-256                                                   |
| `orchestrator/signals/format/mod.rs`      | `format_signal_with_metrics()` — selects stable prefix by stage type, assembles 4 sections                                |
| `orchestrator/signals/format/sections.rs` | Semi-stable (`format_semi_stable_section`), Dynamic (`format_dynamic_section`), Recitation (`format_recitation_section`)  |
| `orchestrator/signals/types.rs`           | `EmbeddedContext`, `DependencyStatus`, `SandboxSummary`                                                                   |
| `orchestrator/signals/knowledge.rs`       | Knowledge-stage signal path: `generate_knowledge_signal()`                                                                |
| `orchestrator/signals/recovery.rs`        | Recovery signal: recovery context header, last known state, recovery actions                                              |
| `orchestrator/signals/recovery_format.rs` | Recovery signal rendering                                                                                                 |
| `orchestrator/signals/helpers.rs`         | `write_signal_file()` disk I/O                                                                                            |
| `orchestrator/signals/crud.rs`            | Signal file CRUD                                                                                                          |

**Insertion point for new shared helper:** `orchestrator/signals/cache.rs` (the "Shared content blocks" cluster). Call it from each of the 4 generator functions.

## Stage Completion (CLI)

- `commands/stage/complete.rs` - Top-level CLI completion entry; **`route_complete_for_conflicts` is the pure routing test seam** that decides Proceed vs ForceUnsafeAssumeMergedVerified vs SpawnResolver vs RevertAndSpawnResolver vs Refuse before any persistence.
- `commands/stage/merge.rs` - `loom stage merge [--resolved]`; wires `merge_verify` for ancestry check.
- `commands/stage/merge_resolver.rs` - CLI-side resolver spawn in the stage worktree; uses `find_live_merge_session_for_stage` for single-resolver-per-stage guard.
- `commands/stage/merge_verify.rs` - `verify_or_derive_completed_commit` (read-only ancestry check shared by `--assume-merged` and `--resolved`).
- `orchestrator/merge_attribution.rs` - `attribute_main_repo_merge` and `reconcile_main_repo_active_merge` (free functions; the daemon-recovery test seam — no `Orchestrator` instance required).

## Terminal Backend

- `orchestrator/terminal/backend.rs` - `SessionBackend`: the facade over the tmux lane for spawn, kill and liveness; `require_tmux`, `tmux_on_path` and `TMUX_MISSING_MESSAGE`
- `orchestrator/terminal/mod.rs` - terminal module root; re-exports the backend and `TmuxBackend`
- `orchestrator/terminal/tmux/` - `TmuxBackend`, one tmux server per session (`mod.rs`, `backend.rs`, `socket.rs`, `viewer.rs`, `reconcile.rs`, `history.rs`)
- `orchestrator/terminal/launch/mod.rs` - `build_claude_command` and the re-exports of the launch machinery
- `orchestrator/terminal/launch/prepare.rs` - assembles one session's launch (capsule, settings, wrapper, build cache)
- `orchestrator/terminal/launch/pid_tracking.rs` - Wrapper script paths, PID tracking, env vars
- `orchestrator/liveness.rs` - LivenessService: wraps the shared SessionBackend; fixed_for_tests() stub for unit tests

## Orchestrator Core Recovery Functions

| Function                            | File                                  | Called From                                                      |
| ----------------------------------- | ------------------------------------- | ---------------------------------------------------------------- |
| `reconcile_and_update_graph()`      | `orchestrator/core/recovery.rs`       | `orchestrator/core/run.rs::initialize_run` and `::run_tick`      |
| `sync_graph_with_stage_files()`     | `orchestrator/core/recovery.rs`       | `orchestrator/core/run.rs::initialize_run` and `::run_tick`      |
| `sync_queued_status_to_files()`     | `orchestrator/core/recovery.rs`       | `orchestrator/core/run.rs::initialize_run` and `::run_tick`      |
| `recover_orphaned_sessions()`       | `orchestrator/core/recovery.rs`       | `orchestrator/core/run.rs::initialize_run` only                  |
| `spawn_merge_resolution_sessions()` | `orchestrator/core/merge_handler.rs`  | `orchestrator/core/run.rs::initialize_run` and `::run_tick`      |
| `start_ready_stages()`              | `orchestrator/core/stage_executor.rs` | `orchestrator/core/run.rs::run_tick`                             |

## Daemon Credentials and Operator Proofs

Daemon startup generates independent user and admin secrets. Both are published with no-follow,
mode-0600 creation beneath the mode-0700 `.loom/work/` directory. The user secret authenticates Ping,
status/log subscriptions, Unsubscribe, and DisputeCriteria. Authentication is checked from a fixed,
allocation-free request preface before the bounded JSON body is accepted.

Startup also publishes `.ignore` and `ripgreprc` at the state root, before either token, via
`daemon/server/tokens.rs::publish_fresh_tokens`. The session wrapper
(`orchestrator/terminal/launch/wrapper.rs`) exports `RIPGREP_CONFIG_PATH` pointing at the published
`ripgreprc`, but only when that file already exists at spawn time; a `loom run --foreground`
orchestrator runs no daemon, so it publishes neither tokens nor exclusions and its sessions get no
export. A sandboxed agent's `rg`/`fd`/`ag` opening `admin.token` or `user.token` hits the sandbox's
own deny rule and stalls auto mode on an operator prompt, so both files keep ordinary sweeps and
`-uu`/`--no-ignore` sweeps away from the credential files. No `Read(...)` permission deny is
written for the tokens at all; the OS-level `sandbox.filesystem.denyRead` list and
`loom-hooks/credential-guard.sh` cover them, for the reason in concerns.md § "No `Read(...)` Deny Rule
May Exist in Any Settings File".

Privileged actions (`loom stop`, `--no-verify`, `--force-unsafe`, `--assume-merged`) authorize
themselves: `admin_proof::authorize` derives the proof from `.loom/work/admin.token`, which the
operator can read and a sandboxed stage agent cannot (the sandbox denies it to the whole process
tree, and `refuse_operator_inside_a_session` turns a session away first). The operator mints and
carries nothing. Proofs are HMAC-SHA256-bound to the project, action, stage (when applicable), and
privileged flag set, then consumed through a private atomic replay marker. A `LOOM_ADMIN_PROOF` in
the environment wins over the derived one: `loom stage admin-proof` (secret through
`LOOM_ADMIN_TOKEN`) exists only to mint a narrowly scoped capability for another process.

`daemon/server/client.rs` verifies the preface in constant time and fails closed on missing or
malformed credentials. `commands/stage/admin_proof.rs` owns minting, exact-request verification,
and replay protection.

## Dispute Criteria — Current Implementation

`commands/stage/dispute_criteria.rs` is a **thin RPC client**, not a state mutator; the socket, relay and spool logic it shares with `dispute-findings`, `dispute-contract` and `dispute-integrity` lives in `commands/stage/dispute_transport.rs`:

```rust
pub fn dispute_criteria(
    stage_id: String,
    field: CriterionField,
    criterion_indices: Vec<usize>,
    reason: String,
    evidence_commit: Option<String>,
    failure_output_path: Option<PathBuf>,
) -> Result<()>
```

- CLI: `loom stage dispute-criteria <stage-id> [--field acceptance|wiring|wiring-tests|artifacts] --criterion-index N [--criterion-index N ...] [--owner-stage <id>] --reason <text> [--evidence-commit <sha>] [--failure-output <path>]`. `--field` defaults to `acceptance`; each index is into the list the field names, repeated or given as a list, sorted and deduplicated (`cli/types_stage_disputes.rs`, `stage_service/dispute.rs`, which range-checks every index). `--owner-stage` (wiring only, integration-verify only) makes the indices name a completed stage's wiring list, the one the aggregated re-verification failed.  An accepted verdict may replace or delete only the filed indices (`orchestrator/adjudication/verdict_indices.rs`); it may insert at any index.
- Sends `Request::DisputeCriteria { auth_token, stage_id, session_id, field, criterion_indices, reason, evidence_commit, failure_output }` (`daemon/protocol.rs`) over the daemon socket. The **daemon** checks the stage transition, writes `.loom/work/disputes/<stage>/<n>/request.md` (its `kind: criterion` record carries `field`) and transitions the stage to `NeedsAdjudication`, then returns the allocated id.
- **Credentials: a missing `.loom/work/user.token` is the NORMAL case here, not an error.** The sandbox denies a stage agent that read by design (S-1: the token authorizes every User RPC, not just the ones a stage agent is entitled to). The client presents `daemon::rpc::user_credential()`, which falls back to a non-empty placeholder, and names the session it is running inside via `LOOM_SESSION_ID`. The daemon authorizes it by the connection instead — see `daemon/server/self_service.rs`.
- `--failure-output` is a path; the client loads it and truncates to 4KB on a UTF-8 char boundary. `models/dispute.rs` holds the one `truncate_to_byte_limit` and `FAILURE_OUTPUT_MAX_BYTES`; the daemon truncates again defensively.
- The agent never writes `.loom/work/disputes/<stage>/<n>/verdict.md` or `applied.marker` — both are daemon-only.
- With no daemon listening the dispute cannot be filed at all (the daemon is what persists it), and the command says so rather than reporting a bare connect error.
- Server-side handler: `daemon/server/dispute.rs`. On-disk schema: `models/dispute.rs`. Verdict rules per field: [adjudication-lifecycle](../architecture/adjudication-lifecycle.md#dispute-kinds-criterion-findings-contract-integrity-impact-tests).
- Open: `DisputeField` (`cli/types_stage_disputes.rs`), `AmendField` and `CriterionField` mirror one enum three times (each names `artifacts` beside `acceptance`, `wiring` and `wiring_tests`), and `dispute_criteria_with_mode` takes nine arguments under `allow(too_many_arguments)` (see [code-quality-and-hook-debt](../concerns/code-quality-and-hook-debt.md)).

## Fix Attempts Counter — Current Usage

`models/stage/types.rs::Stage` — `fix_attempts: u32` field:

- Incremented: `commands/stage/merge.rs` for an actual merge retry, using locked `update_stage`
- Reset to 0: `commands/stage/human_review.rs::handle_approve` on human approve
- Default max: 3 (via `get_effective_max_fix_attempts()` in methods.rs)
- Warning printed when limit reached with hint to `loom stage dispute-criteria`

Alongside it on the `Stage` struct (all shipped): `dispute_count` (`models/stage/types.rs::Stage`), `evidence_rounds` and `amendments_applied` (`models/stage/dispute_budgets.rs`).

## TruthCheck / before_stage / after_stage / code_review

| Location                                                         | Purpose                                                                                                                                                                                                                                                |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `models/stage/checks.rs`                                         | `TruthCheck` (`command`, `stdout_contains`, `stdout_not_contains`, `stderr_empty`, `exit_code`, `description`), `WiringCheck` (with `literal: bool`) and `AcceptanceCriterion`, moved out of `types.rs`; `crate::models::stage::*` paths are unchanged |
| `plan/schema/types.rs::StageDefinition`                          | `before_stage: Vec<TruthCheck>`, `after_stage: Vec<TruthCheck>`, `code_review: Option<CodeReviewConfig>`                                                                                                          |
| `plan/schema/types.rs::CodeReviewConfig`                         | `CodeReviewConfig`: `dimensions: Vec<String>`, `require_all: bool`                                                                                                                                                                                     |
| `models/stage/methods.rs::Stage::from_definition`                | Canonically copies before/after checks, `code_review`, sandbox, and all execution policy into persisted Stage state                                                                                                                                    |
| `orchestrator/core/stage_preflight.rs::before_stage_gate_passed` | Executes before_stage checks BEFORE session spawn; failure → stage Blocked. Skips the checks when `find_prior_stage_work` shows the workspace already holds work                                                                                       |
| `verify/before_after.rs::find_prior_stage_work`                  | Pristine-workspace probe: commits on `loom/<id>` beyond base, or non-scaffold worktree changes → `Some(evidence)` (skip the gate)                                                                                                                      |
| `git/branch/status.rs::list_working_tree_changes`                | `git status --porcelain` paths INCLUDING untracked (`has_uncommitted_changes` excludes them)                                                                                                                                                           |
| `git/worktree/settings.rs::is_worktree_scaffold_path`            | Discounts loom-planted `.loom/work` / `.claude/` / `CLAUDE.md` when judging whether a worktree holds agent work                                                                                                                                        |
| `commands/stage/complete_verification.rs::run_after_checks`      | Executes after_stage checks AFTER acceptance criteria; failure → stage stays Executing                                                                                                                                                                 |
| `verify/before_after.rs`                                         | `run_before_stage_checks()` + `run_after_stage_checks()` — both delegate to `verify_truth_checks()`                                                                                                                                                    |
| `verify/goal_backward/truths.rs::verify_truth_checks`            | `verify_truth_checks(checks, working_dir)` → `Vec<VerificationGap>`, 30s timeout per check                                                                                                                                                             |

## Post-Tool Heartbeat

`loom-hooks/post-tool-use.sh` writes only private heartbeat metadata under `.loom/work/heartbeat/`. It does not persist tool names, commands, output, byte counts, or previews. This prevents credentials and private source printed by tools from becoming durable shared state.

The legacy `ToolEvent` reader remains able to consume an older `.loom/work/tool-events.jsonl`, but no production hook creates or appends that file. New stuck detection therefore relies on heartbeat/session liveness rather than tool-output heuristics. If event observability is restored, it must use a bounded no-follow Rust writer and metadata-only records.

## Status Command (commands/status/)

- `commands/status.rs` - Entry point; dispatches to 3 modes
- `status/data/collector.rs` - `collect_status_data()` — loads stages, sessions, plan into `StatusData` (module root `status/data/mod.rs` re-exports it)
- `commands/status/render/` - Renderers: `render_progress()`, `render_graph()`, `render_merge_status()`, `render_compact()`, `render_attention()`
- `commands/status/ui/` - TUI for `--live` mode (subscribes to daemon via IPC); entry point `commands/status/ui/tui/mod.rs` (`run_tui`)
- `commands/status/diagnostics.rs` - `check_directory_structure()`, `check_parsing_errors()` for `loom status validate` / `doctor`
- `commands/status/display/mod.rs` - `count_files()` helper
- `commands/status/merge_status.rs` - Merge section data
- `commands/status/validation.rs` - `validate_markdown_files()`

**3 display modes:**

| Mode             | Flag        | Behavior                                                                                  |
| ---------------- | ----------- | ----------------------------------------------------------------------------------------- |
| Static (default) | none        | Snapshot: logo → plan name → daemon indicator → progress bar → stage graph → merge status |
| Compact          | `--compact` | Single-line scripting output via `render_compact()`                                       |
| Live             | `--live`    | TUI subscribed to daemon IPC; requires daemon running (`DaemonServer::is_running()`)      |

**Verbose mode (`--verbose`):** Shows `render_attention()` — detailed failure information for blocked/failed stages.

## Token Flow (`loom/src/tokens/`)

- `loom/src/tokens/model.rs` - wire and file model (`TokenFlow`, `TokenUsage`, `read_flow`, size and staleness consts)
- `loom/src/tokens/tracker.rs` - `spawn_token_tracker`, the 5 s pass, `flow.json` and checkpoint writes
- `loom/src/tokens/pass_memo.rs` - `PassMemo`: what a pass reuses from the last (checkpoint, unchanged flow, quota time left)
- `loom/src/tokens/discover.rs`, `tail.rs`, `accumulate.rs`, `claude_line.rs`, `codex_line.rs` - sources, incremental reads, dedupe, line parsers
- `loom/src/tokens/flow_build.rs`, `quota_left.rs`, `quota_points.rs`, `checkpoint.rs` - buckets to rates and series, time-left estimate, restart state
- `loom/src/status/data/sanitize_token_flow.rs` - collector-side hygiene of the flow
- `loom/src/commands/status/ui/tui/ledger/tokens_pane.rs`, `tokens_models.rs`, `layout_tokens.rs` - the `t` pane, its by-model block and the row budget
- `web/src/lib/tokens.ts`, `web/src/components/token-flow-panel.tsx`, `token-models.tsx`, `stage-tokens.tsx`, `graph/node-tokens.tsx` - web helpers and surfaces

Design and limits: [architecture/usage-evidence.md](../architecture/usage-evidence.md).

## Subagent Verification Guard

| Path                                                             | Role                                                                                                                                                         |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `loom-hooks/subagent-verify-guard.sh` | PreToolUse:Bash guard: role and stage detection, runner matching, block output and token walk in one file |
| `loom-hooks/_common.sh`                                          | `loom_is_subagent()` — payload-first detection gate, process-tree walk as fallback; 619 lines, also over the Rule 17 cap — see [concerns.md](../concerns.md) |
| `loom/src/orchestrator/signals/tests_doctrine.rs` | Pins doctrine blocks across signals, the subagent preamble and the guard |
| `loom/tests/integration/hooks_subagent_verify_guard.rs`          | harness: process-tree construction, env scrubbing, payload building                                                                                          |
| `loom/tests/integration/hooks_subagent_verify_guard_cases.rs`    | `BLOCK_CASES` / `ALLOW_CASES` table data                                                                                                                     |
| `loom/tests/integration/hooks_subagent_verify_guard_carveout.rs` | integration-verify carve-out **refusal** directions (decoy, wrong type, missing)                                                                             |

Split into three files because they grow for different reasons; wired with `#[path]` submodules
so the children reach the parent's private helpers via `use super::*` without widening visibility.

## Native Codex Worker Files (`loom/src/native_worker/`, `codex_backend.rs`)

- `codex_backend.rs` - `CodexBackend`, `CodexLane`, `configured_backend`, `codex_lane`; the `codex.backend` key is registered in `user_config/keys.rs`.
- `native_worker/index.rs` - the only read API for consumers: `stage_disposition`, `unit_state`, `stage_units`, `stage_threads`, `stages`. `store.rs` and `store_io.rs` write the record directory; `model.rs` and `ids.rs` hold the types and constants.
- `native_worker/admission.rs` (+ `admission_brief.rs`) - daemon-side `admit_start`, `accept_message`, `accept_cancel`; called from `orchestrator/core/inbox_drain/worker_control.rs`. `runner.rs` and `runner_recover.rs` start and settle supervisors.
- `native_worker/supervisor/`, `machine/`, `protocol/` - the per-worker loop, the pure reducer and the app-server wire. `boundary/`, `proxy/`, `bridge.rs` - the bubblewrap boundary, allowlist proxy and in-boundary forwarder.
- `commands/subagents/native_start.rs`, `native_control.rs` and `wait/native.rs` - the CLI verbs and the wait binding; `commands/stage/native_gate.rs` - the shared gate; `relay/payload_native.rs` - the `subagents:control` payload; `loom-hooks/loom-control-subagents.sh` - the command pin.

Design and gates: [Native Codex Workers](../architecture/codex-concurrency.md).
