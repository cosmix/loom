# Entry Points

> Key files to read first
>
> **Related files:** [architecture.md](architecture.md) for system overview, [patterns.md](patterns.md) for design patterns.

## Hooks

The full hook roster — every script in `loom-hooks/`, the event it binds to, what it blocks — plus
`loom-hooks/_common.sh`'s shared helpers and the registration sites a new hook must be added to.

→ [Hook Entry Points](entry-points/hooks.md)

## Token Accounting and Receipt Surfaces

- `loom/src/commands/usage/mod.rs` — `loom usage` dispatch; args in `args.rs` (`UsageArgs`, `has_explicit_selection`); providers in `provider_types.rs`; the `reads` section (`sections/reads.rs`) measures rereads from transcripts
- `loom/src/commands/usage/completeness.rs`, `codex_scope.rs`, `spawn_join.rs` — `measured`/`partial` reasons and session-record inventory, receipt-scoped Codex rows, hook spawn requests joined to Claude starts
- `loom/src/commands/usage/comparison.rs`, `comparison_net.rs`, `comparison_latency.rs`, `run_evidence.rs` (+ `run_evidence_stages.rs`) — `--compare`, the declared net-total rule, declared latency tolerance, and `--run-evidence` run-evidence producer
- `loom/tests/signal_replay.rs` — signal replay harness ([Signal Generation](architecture/signal-generation.md))
- `loom/src/verify/criteria/cache_contract.rs` — certified criterion-cache contract
- `loom/src/models/forward_receipt.rs` (+ `payload.rs`, `transcript/argv.rs`), `loom/src/commands/hook/forward_receipt.rs` (reads the PostToolUse payload on stdin), `loom/src/commands/subagents/forward_jobs_wait.rs` — forward receipts and `loom subagents wait`
- `loom/src/commands/hook/worker_brief.rs` (worker briefs filed at issue), `loom/src/commands/hook/pre_compact.rs` (prompt-record reset on compaction), `loom/src/quota/history.rs` — worker briefs, compaction reset, quota history
- `loom-hooks/_read_ledger.sh`, `loom-hooks/_read_discipline.sh` — deny-once read gate, sibling-read ledger and read-guard rules ([Read Gate](architecture/read-gate.md); no Rust read-receipt path exists)
- `loom/src/commands/ask/` (`mod.rs`, `backend.rs`, `run.rs`, `payload.rs`, `eval.rs`) — `loom ask` one-shot worker and `--eval` ([loom ask](architecture/loom-ask.md))
- `loom/src/commands/usage/sections/attribution/` — `loom usage` Attribution section ([Read Gate](architecture/read-gate.md#the-attribution-section-of-loom-usage))
- `loom/src/map/views/markdown_outline.rs` — direct heading outline for `loom map --outline` of a Markdown file
- `agents/loom-software-developer.md` — haiku implementer ([Subagent Hierarchy](patterns/subagent-hierarchy.md))

→ [Token Accounting and Receipts](architecture/token-accounting-and-receipts.md)

## Terminal Backends and `loom attach`

The dispatcher/lanes, `[terminal]` config, `--backend` flag, and `loom attach` entry
points all live in one topic file now.

Full detail: [terminal-backends.md](architecture/terminal-backends.md).

## Execution Containment

| Path                                                                 | What it owns                                                                                                            |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `verify/criteria/confine.rs`                                         | `spawn_confined` (the single leaf primitive for every plan-authored command), `resolve_confinement`, `plan_confinement` |
| `process/environment.rs`                                             | `AGENT_SESSION_ENV_NAMES`, `STAGE_HOST_ENV_ALLOWLIST`, `apply_stage_environment`                                        |
| `models/stage/sandbox_config.rs::CommandConfinement`                 | `CommandConfinement`; `NetworkConfig`                                                                            |
| `orchestrator/terminal/native/wrapper/script_text.rs::env_allowlist` | renders the shared `process::AGENT_SESSION_ENV_NAMES` allowlist                                                         |

Start from `architecture/execution-containment.md` — it states precisely what these do and
do not guarantee, which is narrower than the word "containment" suggests.

## Owned Waits and Completion Recovery Surfaces

- `commands/subagents/wait/` — `lease.rs`, `lease_fs.rs`, `engine.rs`, `identity.rs`, `codex_binding.rs`, `model.rs`, `output.rs`, `mod.rs`: the owned `loom subagents watch --worker ...` implementation. `commands/subagents/forward_jobs_wait.rs` owns `loom subagents wait --receipt ...`. See [Owned Waits](architecture/owned-waits.md).
- `subagent_lifecycle/` (top-level crate module, not under `orchestrator/monitor/`) — `store.rs` (`LifecycleIndex::outcome`, `append_locked`, `codex_event_id`), `lock.rs` (`JournalLock`/`claim_lock`). Per-worker lifecycle journal (`subagents/<stage>/lifecycle.jsonl`), consumed by `commands/subagents/classify/lifecycle.rs`.
- `codex_lifecycle/` — `authorization.rs`, `jobs.rs`, `ledger.rs`, `reconcile.rs` (`reconcile_codex_jobs`, `companion_outcome`). Codex companion job identity/authorization and daemon reconciliation into `subagent_lifecycle`.
- `handoff/completion/` — `attest.rs` (HMAC attestation), `checkpoint.rs`, `identity.rs` (`expected_stage_commit`, `stage_head_commit`), `mod.rs`. See [Completion Recovery](architecture/completion-recovery.md).
- `models/session/methods.rs` — `Session.exit_reason: Option<SessionExitReason>`; `record_heartbeat` now takes `progress_at` first (monotonic `last_active`).
- `loom-hooks/_lifecycle.sh` — shared lifecycle-journal/heartbeat helpers (`loom_lifecycle_refresh_heartbeat`) used by `subagent-stop.sh` and `teammate-idle.sh`; `loom_heartbeat_prior_progress_at` (in `_common.sh`) carries forward the last validated `progress_at` on an observation-only tool call.

## Host Verification Job Surfaces

`commands/stage/complete_relay.rs` (session side), `commands/stage/verify_host.rs` (host child), `host_verify/` (job store, runner, apply), `orchestrator/core/host_jobs.rs` (tick). See [Host Verification Jobs](architecture/verification-v2-gates.md) and [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md).

## Topic Files

Each topic file holds the full section for its area; this file only points to them.

- [CLI, Commands and Plan Pipeline](entry-points/cli-and-plan-pipeline.md) — `main.rs`/`lib.rs` entry, command dispatch (`cli/types.rs`), subcommand registration, plan parsing/validation/graph, schema-to-runtime conversion, the verification system and Verification v2 gates, config file paths, `loom pressure`, the new CLI surface.
- [Orchestrator, Daemon and Sessions](entry-points/orchestrator-daemon-and-sessions.md) — orchestrator main loop and recovery functions, data models, daemon and credentials, monitor subsystem, signal generation, terminal backend, status command, heartbeat, dispute criteria, fix-attempts counter, `TruthCheck`/before-after gates, the subagent verification guard.
- [Filesystem and Integration Modules](entry-points/filesystem-and-integration-modules.md) — git operations, `.loom/work/` state, handoff system, sandbox config and `ANTHROPIC_API_KEY` hygiene, remote-control gate, WorkDir helpers, self-update HTTP client, the tiered knowledge-base layout.
- [Context Retrieval and Source Graph](entry-points/context-and-source-graph.md) — retrieval pipeline, source-graph channel and its overlay lifecycle.
- [Hook Entry Points](entry-points/hooks.md) — hook roster and shared helpers.
- [Remote Control & Permission Mode](entry-points/remote-control.md) — capability detection and permission-mode resolution call sites.

## Planner Web Entry Points

- `scripts/plan-screens.sh` (+ `web/scripts/plan-screens.mjs`) — real-browser screenshots of planner routes against a live `loom plan` server ([plan-web](conventions/plan-web.md#visual-stages-and-the-screenshot-script)); `scripts/plan-journeys.sh` (+ `web/scripts/plan-journeys*.mjs`) — scripted user journeys J1-J8 against the same server
- `web/src/plan/feature/feature-layout.tsx`, `spine.tsx`, `page-offer.ts`, `landing.ts` — the feature layout route, its spine, offer ownership and landing; `web/src/plan/ui/DESIGN.md` — the design record; `loom/src/plan_session/owner_watch.rs` — reclaims a session's tmux server after its owner stays away
- `web/src/plan/header/header.tsx`, `dock.tsx`, `chat/session-home.ts`, `concept/column-keys.ts`, `help/pin-placement.ts` — app bar, chat mounting, a chat's home screen, reading-column keys, help pins
- `loom/src/plan_server/wire/checklist_card.rs` — per-feature card facts; `loom/src/plan_server/summaries/backend.rs` and `codex_spawner.rs` — summary backend dispatch and the Codex job; `loom/src/plan_session/bridge/opening.rs` — Enter re-press and unsubmitted-opening failure

- `loom/src/plan_server/uploads/mod.rs` (`POST /api/attachments`), `loom/src/plan_session/attachments/stored.rs` (id and sidecar grammar, media types), `loom/src/plan_server/chat/drafts.rs` and `draft_cmd.rs` (kickoff drafts, rename, set aside, continue), `web/src/plan/attachments/intake.ts`, `new/draft-view.tsx`, `home/drafts-shelf.tsx`, `review/changes-page.tsx`, `shell-footer.tsx` — uploads, drafts, the changes page and the limits footer

- `loom/src/plan_session/bridge/background.rs` (+ `background/entry.rs`, `inputs.rs`) — the background-task table; `loom/src/plan_server/chat/background/` and `loom/src/tokens/planner.rs` — its wire frame and the planner token pass; `loom/src/plan_session/commands/` (`list.rs`, `builtins.rs`, `side.rs`), `plan_server/commands/` and `plan_session/bridge/local.rs` — the slash command table, its side process, route and local settle; `loom/tests/fixtures/plan_session/probe-notes.md` — the Claude Code 2.1.321 facts they rest on; `web/src/plan/chat/background-line.tsx`, `background-list.tsx`, `command-menu.tsx`, `web/src/plan/state/commands-atom.ts` and `web/scripts/plan-chat-screens.mjs` — the idle line, task list, command menu and their browser screens.

→ [Planner Web Shell and Screens](architecture/planner-web-shell.md), [Planner Feature Screens](architecture/planner-feature-screens.md), [Planner Uploads and Kickoff Drafts](architecture/planner-uploads-and-drafts.md), [Summary Store](architecture/summary-store.md#summary-backends-claude-and-codex)
