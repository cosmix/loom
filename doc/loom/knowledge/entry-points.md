# Entry Points

> Key files to read first
>
> **Related files:** [architecture.md](architecture.md) for system overview, [patterns.md](patterns.md) for design patterns.

## CLI Entry Point

`main.rs`/`lib.rs` entry, command dispatch, plan parsing/validation/graph, the
verification pipeline, and `.loom/work/` config file paths.

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Command Dispatch (cli/types.rs)

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Orchestrator Core

Orchestrator main loop, stage/session data models, the daemon, monitor subsystem,
signal generation, merge/completion routing, terminal backends, recovery functions,
daemon credentials, dispute-criteria RPC, `TruthCheck`/before-after gates, the
subagent verification guard, and the status command.

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Data Models

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Plan Parsing Pipeline

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Git Operations

Git worktree/merge/branch operations, `.loom/work/` filesystem state, the handoff
system, sandbox config generation, the remote-control gate, WorkDir directory
helpers, ANTHROPIC_API_KEY env hygiene, the self-update HTTP client, and the
tiered knowledge-base module layout.

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## File System State

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## Daemon

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Monitor Subsystem

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Signal System

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Stage Completion (CLI)

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Terminal Backend

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Handoff System

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## Sandbox

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## Hooks

The full hook roster — every script in `loom-hooks/`, the event it binds to, what it blocks — plus
`loom-hooks/_common.sh`'s shared helpers and the registration sites a new hook must be added to.

→ [Hook Entry Points](entry-points/hooks.md)

## Schema-to-Runtime Conversion

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## CLI Subcommand Registration Pattern

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Remote Control Module

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## Other Modules

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## Key Config Files

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Verification System

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Plan Validation Functions (plan/schema/validation.rs)

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Plan Parser Module (plan/parser/mod.rs)

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Execution Graph Build (plan/graph/mod.rs)

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Status Command (commands/status/)

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Post-Tool Heartbeat

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Orchestrator Core Recovery Functions (Exact Locations)

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Plan Graph Loader — Stage File Preference (Critical)

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Plan Schema — StageDefinition Amendable Fields

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## WorkDir Directory Helpers (Existing vs. Missing)

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## Sandbox Settings — ANTHROPIC_API_KEY

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## HTTP Client Pattern — self_update/client.rs

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## Daemon Credentials and Operator Proofs

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Dispute Criteria — Current Implementation

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Fix Attempts Counter — Current Usage

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Remote Control & Permission Mode Integration Points

Every file and call site involved in remote-control capability detection and permission-mode
resolution, with line references.

→ [Remote Control & Permission Mode](entry-points/remote-control.md)

## Signal Generation — Key Files and Line References

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## TruthCheck / before_stage / after_stage / code_review

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## `loom pressure` — Plan Pressure-Testing Files

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Tiered Knowledge Base (2026-07-28)

→ [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md)

## Subagent Verification Guard (2026-07-28)

→ [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md)

## Terminal Backends and `loom attach` (2026-08-08)

The dispatcher/lanes, `[terminal]` config, `--backend` flag, and `loom attach` entry
points all live in one topic file now.

Full detail: [terminal-backends.md](architecture/terminal-backends.md).

## Context Retrieval Subsystem (2026-08-17)

The context-retrieval pipeline (`context/mod.rs` is the one entry point) and the
source-graph retrieval channel: extraction, ranking, fusion, packing, delivery
dedupe, the overlay lifecycle, and `loom map`'s views and flags (`--callers`, `--references`, `--window`, `--census`, `--eval-edges`, …).

→ [Context Retrieval and Source Graph](entry-points/context-and-source-graph.md)

## Execution Containment (2026-08-17)

| Path                                          | What it owns                                                                                                            |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `verify/criteria/confine.rs`                  | `spawn_confined` (the single leaf primitive for every plan-authored command), `resolve_confinement`, `plan_confinement` |
| `process/environment.rs`                      | `STAGE_HOST_ENV_ALLOWLIST`, `apply_stage_environment`                                                                   |
| `models/stage/sandbox_config.rs:17`                   | `CommandConfinement`; `:100` `NetworkConfig`                                                                            |
| `orchestrator/terminal/native/wrapper.rs:181` | a SECOND, diverging copy of the env allowlist                                                                           |

Start from `architecture/execution-containment.md` — it states precisely what these do and
do not guarantee, which is narrower than the word "containment" suggests.

## New CLI Surface (2026-08-17)

→ [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md)

## Source Graph as a Retrieval Channel, and Its Lifecycle (2026-08-18)

→ [Context Retrieval and Source Graph](entry-points/context-and-source-graph.md)

## Owned Waits and Completion Recovery Surfaces (2026-09-14)

- `commands/subagents/wait/` — `lease.rs`, `lease_fs.rs`, `engine.rs`, `identity.rs`, `codex_binding.rs`, `model.rs`, `output.rs`, `mod.rs`: the owned `loom subagents watch/wait --worker ...` implementation. See [Owned Waits](architecture/owned-waits.md).
- `subagent_lifecycle/` (top-level crate module, not under `orchestrator/monitor/`) — `store.rs` (`LifecycleIndex::outcome`, `append_locked`, `codex_event_id`), `lock.rs` (`JournalLock`/`claim_lock`). Per-worker lifecycle journal (`subagents/<stage>/lifecycle.jsonl`), consumed by `commands/subagents/classify/lifecycle.rs`.
- `codex_lifecycle/` — `authorization.rs`, `jobs.rs`, `ledger.rs`, `reconcile.rs` (`reconcile_codex_jobs`, `companion_outcome`). Codex companion job identity/authorization and daemon reconciliation into `subagent_lifecycle`.
- `handoff/completion/` — `attest.rs` (HMAC attestation), `checkpoint.rs`, `identity.rs` (`expected_stage_commit`, `stage_head_commit`), `mod.rs`. See [Completion Recovery](architecture/completion-recovery.md).
- `models/session/methods.rs` — `Session.exit_reason: Option<SessionExitReason>`; `record_heartbeat` now takes `progress_at` first (monotonic `last_active`).
- `loom-hooks/_lifecycle.sh` — shared lifecycle-journal/heartbeat helpers (`loom_lifecycle_refresh_heartbeat`) used by `subagent-stop.sh` and `teammate-idle.sh`; `loom_heartbeat_prior_progress_at` (in `_common.sh`) carries forward the last validated `progress_at` on an observation-only tool call.

## Verification v2 Entry Points

Contract, integrity, review and impact gates, the 23 test-runner adapters, `loom project detect`, review harvest and the dispute-kind handlers. Table of paths: [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md). Gate order and owners: [Verification v2 gates](architecture/verification-v2-gates.md).

## Token Accounting and Receipt Surfaces

- `loom/src/commands/usage/mod.rs` — `loom usage` dispatch; args in `args.rs` (`UsageArgs`, `has_explicit_selection`); providers in `provider_types.rs`; the `reads` section (`sections/reads.rs`) measures rereads from transcripts
- `loom/src/commands/usage/completeness.rs`, `codex_scope.rs`, `spawn_join.rs` — `measured`/`partial` reasons and session-record inventory, receipt-scoped Codex rows, hook spawn requests joined to Claude starts
- `loom/src/commands/usage/comparison.rs`, `comparison_net.rs`, `comparison_latency.rs`, `run_evidence.rs` (+ `run_evidence_stages.rs`) — `--compare`, the declared net-total rule, declared latency tolerance, and `--run-evidence` run-evidence producer
- `loom/tests/signal_replay.rs` — signal replay harness ([Signal Generation](architecture/signal-generation.md))
- `loom/src/verify/criteria/cache_contract.rs` — certified criterion-cache contract
- `loom/src/models/forward_receipt.rs` (+ `payload.rs`, `transcript/argv.rs`), `loom/src/commands/hook/forward_receipt.rs` (reads the PostToolUse payload on stdin), `loom/src/commands/subagents/forward_jobs_wait.rs` — forward receipts and `loom subagents wait`
- `loom/src/commands/hook/worker_brief.rs` (worker briefs filed at issue), `loom/src/commands/hook/pre_compact.rs` (prompt-record reset on compaction), `loom/src/quota/history.rs` — worker briefs, compaction reset, quota history
- `loom-hooks/_read_ledger.sh`, `loom-hooks/_read_discipline.sh` — sibling-read ledger and read-guard rules (no Rust read-receipt path exists)

→ [Token Accounting and Receipts](architecture/token-accounting-and-receipts.md)
