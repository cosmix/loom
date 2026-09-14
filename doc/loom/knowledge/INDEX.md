<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then only what it points to: the section for your area in a tier-1 summary (`rg -n '^## ' <file>` lists them) and the tier-2 topics your task touches. A specific question is cheaper to pull than to read — `loom knowledge context --query "..."` returns the matching sections quoted.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 248 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 241 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 157 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 235 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 250 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 119 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 231 |

## Tier 2 — Topics

### architecture

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-lifecycle](architecture/adjudication-lifecycle.md) | Dispute to durable verdict, and each verdict's effect | 60 |
| [codex-concurrency](architecture/codex-concurrency.md) | Codex fan-out concurrency limits, what is measured, and what degrades under… | 123 |
| [codex-plugin](architecture/codex-plugin.md) | Codex plugin install, identity, and forwarding | 402 |
| [completion-recovery](architecture/completion-recovery.md) | Completion HMAC attestation, exit_reason, handoff folds | 34 |
| [context-ceiling](architecture/context-ceiling.md) | Resident-token ceiling: resolution tiers and three thresholds | 109 |
| [context-retrieval](architecture/context-retrieval.md) | Retrieval: graphs, lanes, gating, tiered packs | 674 |
| [core-abstractions](architecture/core-abstractions.md) | ExecutionGraph, Stage, Session, Orchestrator, data flow | 136 |
| [directory-structure](architecture/directory-structure.md) | loom/src module tree, state layout, root assets | 49 |
| [execution-containment](architecture/execution-containment.md) | Sandboxed command containment and its limits | 291 |
| [hook-system](architecture/hook-system.md) | Hook embedding/install, SessionStart contract, enforcement layers | 257 |
| [knowledge-hierarchy](architecture/knowledge-hierarchy.md) | Read before touching fs/knowledge: targets, INDEX.md, checks, size limits | 250 |
| [memory-spool](architecture/memory-spool.md) | Read before touching loom memory: spool/drain, ids, receipts, pending, archive | 159 |
| [merge-flow](architecture/merge-flow.md) | How a completed stage reaches its target branch | 79 |
| [orchestrator-loop](architecture/orchestrator-loop.md) | Daemon main-loop tick order, Monitor subsystem, heartbeat liveness. | 56 |
| [owned-waits](architecture/owned-waits.md) | Worker-set waits: lease/engine, exit codes, unit-survival limit | 23 |
| [plan-lifecycle-and-fields](architecture/plan-lifecycle-and-fields.md) | Plan field checklist, goal-backward layers, schema fields, amendment. | 107 |
| [quota-poller](architecture/quota-poller.md) | How loom learns the operator's Claude and Codex subscription budget, where it… | 29 |
| [remote-control](architecture/remote-control.md) | Capability detection, preflight, resolution, and per-kind session naming for… | 82 |
| [security-and-isolation](architecture/security-and-isolation.md) | 4-layer worktree defense, security model, settings.local.json sites. | 178 |
| [signal-generation](architecture/signal-generation.md) | Signal assembly: cache, append_* helpers, per-stage prefixes, hung escalation. | 196 |
| [skill-catalog](architecture/skill-catalog.md) | The two skill roots, why 53 skills live outside `~/.claude/skills`, and the… | 111 |
| [source-graph](architecture/source-graph.md) | What the source graph is and is not, its honesty contract, extractor trait… | 329 |
| [status-data-model](architecture/status-data-model.md) | Where each loom status field comes from | 196 |
| [terminal-backends](architecture/terminal-backends.md) | Native and tmux session backends, lane resolution | 285 |
| [token-accounting-and-receipts](architecture/token-accounting-and-receipts.md) | Usage ledger, --compare, criterion cache, receipts, exact waits | 254 |
| [web-dashboard](architecture/web-dashboard.md) | loom status --web: server, SPA, streaming | 51 |
| [web-terminal](architecture/web-terminal.md) | loom status --web --terminals: a browser terminal attached to a session | 163 |

### entry-points

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md) | CLI dispatch, plan parsing/validation/graph, verification, configs | 173 |
| [context-and-source-graph](entry-points/context-and-source-graph.md) | Context retrieval pipeline and the source-graph channel/lifecycle | 50 |
| [filesystem-and-integration-modules](entry-points/filesystem-and-integration-modules.md) | Git, fs/work_dir, handoff, sandbox, remote control, knowledge base | 111 |
| [hooks](entry-points/hooks.md) | Hook scripts, their events, command matching | 117 |
| [orchestrator-daemon-and-sessions](entry-points/orchestrator-daemon-and-sessions.md) | Orchestrator loop, daemon, monitor, signals, merges, dispute/verify | 224 |
| [remote-control](entry-points/remote-control.md) | Files and call sites for remote-control capability detection and… | 97 |

### patterns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-process-and-conventions](patterns/cli-process-and-conventions.md) | CLI registration, TUI, error handling, process mgmt, config, HTTP client. | 246 |
| [doctrine-cross-surface](patterns/doctrine-cross-surface.md) | Pinning multi-surface guidance with equality tests | 134 |
| [hook-content-stripping](patterns/hook-content-stripping.md) | How a hook decides what a Bash command actually invokes: strip embedded… | 151 |
| [merge-and-recovery](patterns/merge-and-recovery.md) | Progressive merge, conflict recovery, attribution, dispute files. | 96 |
| [orchestrator-daemon-loop](patterns/orchestrator-daemon-loop.md) | Signal gen, daemon IPC, poll loop, heartbeat, session backend, spool drain. | 107 |
| [remote-control](patterns/remote-control.md) | The detect-capability, preflight, resolve-invocation shape for external agent… | 51 |
| [security-sandbox-and-hooks](patterns/security-sandbox-and-hooks.md) | Hooks, input validation, permission sync, sandbox config, untrusted values. | 105 |
| [stage-daemon-channels](patterns/stage-daemon-channels.md) | How a stage agent reaches the daemon to change its own state | 105 |
| [stage-lifecycle-and-verification](patterns/stage-lifecycle-and-verification.md) | Stage/session states, locked writes, acceptance & verification layers. | 170 |
| [subagent-hierarchy](patterns/subagent-hierarchy.md) | Flat fan-out vs 2-level coordinators vs agent teams; model mix | 86 |

### conventions

| Topic | Blurb | Lines |
| --- | --- | --- |
| [code-style-and-structure](conventions/code-style-and-structure.md) | Rust naming, error handling, size limits, splitting, and docstring conventions | 248 |
| [commits](conventions/commits.md) | Logically grouped commits, Conventional Commit messages, and no AI attribution. | 14 |
| [dispute-and-adjudication](conventions/dispute-and-adjudication.md) | Dispute file authority split, adjudicator scope, budgets, and transport | 114 |
| [git-and-build-workflow](conventions/git-and-build-workflow.md) | Git/worktree ops, cargo fmt/test discipline, the shared maintainability ledger | 188 |
| [guidance-channels-and-plugin-scope](conventions/guidance-channels-and-plugin-scope.md) | Guidance-channel selection, verification-is-main-agent rule, plugin scope | 104 |
| [model-and-effort-config](conventions/model-and-effort-config.md) | `[pressure]` and `[models]` config sections, the four-tier precedence chain… | 62 |
| [plan-yaml-and-hooks](conventions/plan-yaml-and-hooks.md) | Plan YAML schema, hook stdin/stdout contract, skill format, additive fields | 134 |
| [web-dashboard-typography](conventions/web-dashboard-typography.md) | Dashboard chrome type conventions and CSS gotchas for settings/graph views | 30 |

### mistakes

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-autonomy-deadlock](mistakes/adjudication-autonomy-deadlock.md) | An accepted verdict deadlocked the run: adoption by stage_id alone, requeue… | 188 |
| [ambient-filesystem-trust](mistakes/ambient-filesystem-trust.md) | Why a .git directory is not evidence of a real repository | 146 |
| [codex-lane-rogue-wrapper](mistakes/codex-lane-rogue-wrapper.md) | A forwarding wrapper that did the task itself instead of forwarding, and why… | 153 |
| [codex-navigation](mistakes/codex-navigation.md) | Forbidding reads instead of fixing a slow reader - a misdiagnosis and its… | 32 |
| [codex-worker-briefing](mistakes/codex-worker-briefing.md) | Codex brief pitfalls: braces, doc placeholders, path reuse, jq status | 46 |
| [completion-broker-credential](mistakes/completion-broker-credential.md) | The completion broker unreachable server-side fallback, duplicate file naming… | 179 |
| [computed-values-and-hidden-couplings](mistakes/computed-values-and-hidden-couplings.md) | Values computed right but unread downstream; hidden coupling bugs | 208 |
| [concurrency-and-locking](mistakes/concurrency-and-locking.md) | Locked-handle writes and read-mutate-save races that lose concurrent updates. | 36 |
| [detached-spawn-in-tests](mistakes/detached-spawn-in-tests.md) | Never spawn a process from a test that can outlive the test process. | 45 |
| [doctrine-and-acceptance](mistakes/doctrine-and-acceptance.md) | Why a one-phrase grep proves presence but never agreement, and how doctrine… | 290 |
| [hooks-shell-portability](mistakes/hooks-shell-portability.md) | gawk/bash portability traps and heredoc-scanning gotchas in the repo's hooks. | 85 |
| [knowledge-base-drift](mistakes/knowledge-base-drift.md) | How the knowledge base itself goes stale: plan-authoring notes frozen as… | 157 |
| [knowledge-cli-invariants](mistakes/knowledge-cli-invariants.md) | Invariants belong in the fs constructor, not the CLI handler | 130 |
| [knowledge-write-channel](mistakes/knowledge-write-channel.md) | Why a distillation stage cannot write knowledge directly, the… | 100 |
| [ledger-tui-rendering](mistakes/ledger-tui-rendering.md) | Wide-glyph padding, fan-out duplication, and latent panics in the ledger TUI. | 63 |
| [live-state-pollution](mistakes/live-state-pollution.md) | A stage test run rewrote live .loom/work state | 33 |
| [memory-relay-drain-gap](mistakes/memory-relay-drain-gap.md) | loom-relay.sh misses memory:resolve; ticket-cap deadlock and recovery | 17 |
| [merge-cleanup-boundary](mistakes/merge-cleanup-boundary.md) | A cleanup-boundary bug and its fix | 171 |
| [parallel-worktree-shared-state](mistakes/parallel-worktree-shared-state.md) | Cross-worktree state races: diagnostic question, cases, fix | 140 |
| [phantom-merges](mistakes/phantom-merges.md) | Eight lessons on merge machinery: merged=true without verifying | 141 |
| [pinned-literals-ledgers-and-wiring](mistakes/pinned-literals-ledgers-and-wiring.md) | The maintainability ledger exact-match trap and goal-backward wiring checks… | 235 |
| [pre-commit-hardening](mistakes/pre-commit-hardening.md) | Partial-staging guard decisions, edge cases, mutant-settled git defaults | 53 |
| [refactor-stragglers](mistakes/refactor-stragglers.md) | What a large removal or rename leaves behind: straggler initializers, stale… | 94 |
| [sandbox-and-settings](mistakes/sandbox-and-settings.md) | Sandbox path rules, permission sync, excludedCommands matching, and settings… | 737 |
| [schema-reuse-and-silent-skips](mistakes/schema-reuse-and-silent-skips.md) | deny_unknown_fields breaking a type with two deserialization sources… | 130 |
| [session-identity-env](mistakes/session-identity-env.md) | LOOM_* wrapper exports are a contract read by hooks, CLI and daemon | 104 |
| [sessions-and-liveness](mistakes/sessions-and-liveness.md) | Session identity, liveness routing, spawn-site coverage | 344 |
| [shell-command-matchers](mistakes/shell-command-matchers.md) | Separators that never become tokens, forgeable glob lookups, env leakage in… | 246 |
| [spurious-waiting-for-input](mistakes/spurious-waiting-for-input.md) | Stages flipped to waiting-for-input with no AskUserQuestion | 25 |
| [status-broadcast-hardening](mistakes/status-broadcast-hardening.md) | Frame-overflow eviction and read-timeout desync in status broadcast | 74 |
| [store-without-consumer](mistakes/store-without-consumer.md) | A store that was written but never read - what happened, why it stayed… | 94 |
| [subagent-orchestration](mistakes/subagent-orchestration.md) | Liveness signals: when a missing report is not a missing result | 534 |
| [testing-and-lint](mistakes/testing-and-lint.md) | Lint/test discipline: --all-targets, --no-fail-fast, headless CI | 664 |
| [tests-that-cannot-fail](mistakes/tests-that-cannot-fail.md) | Tests that pass regardless of whether the bug they exist to catch is present… | 264 |
| [tmux-backend](mistakes/tmux-backend.md) | tmux spawn-failure exit codes and cleanup-on-error discipline | 136 |
| [untrusted-value-boundaries](mistakes/untrusted-value-boundaries.md) | Enumerating every producer of a rendered field, not just the field, and why… | 188 |
| [verification-harness](mistakes/verification-harness.md) | When every check fails at once, suspect the harness; the PATH binary is not… | 369 |
| [visibility-and-reachability](mistakes/visibility-and-reachability.md) | pub(crate) is not nameable by itself - visibility is capped by path… | 115 |
| [web-dashboard-server](mistakes/web-dashboard-server.md) | Concurrency, security and testing lessons from building the hand-rolled… | 273 |
| [writer-reader-address](mistakes/writer-reader-address.md) | A layer written under a key its reader ignores looks identical to no-op. | 73 |

### concerns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [automatic-knowledge-source-graph-followups](concerns/automatic-knowledge-source-graph-followups.md) | Knowledge-plan followups, retrieval-degradation gap, stopwording, resolved items | 100 |
| [code-quality-and-hook-debt](concerns/code-quality-and-hook-debt.md) | Code-quality/hook debt: oversized units, debug logging, duplicated tables | 181 |
| [codex-heartbeat-starvation](concerns/codex-heartbeat-starvation.md) | Heartbeat starvation from long codex runs; stale-badge constant mismatch | 77 |
| [daemon-singleton](concerns/daemon-singleton.md) | Historical incident: two daemons once attached to the same `.work/`. Startup… | 98 |
| [iterm2-window-teardown](concerns/iterm2-window-teardown.md) | iTerm2 spawn never names its window, so teardown cannot find it to close | 48 |
| [knowledge-cli-gaps](concerns/knowledge-cli-gaps.md) | Knowledge/memory CLI gaps: no delete-section, no blurb flag, CRLF, backlog | 179 |
| [merge-and-recovery-edge-cases](concerns/merge-and-recovery-edge-cases.md) | Merge/retry/completion edge cases: phantom merges, stale started_at, nonces | 80 |
| [runtime-and-session-safety](concerns/runtime-and-session-safety.md) | Runtime edge cases: tmux warning, attach lifetime, orphan adoption, guards | 146 |
| [sandbox-and-confinement-gaps](concerns/sandbox-and-confinement-gaps.md) | Sandbox gaps: no E2E canary, diverging env allowlists, uncalled validators | 166 |
| [sandbox-protected-hooks-dir](concerns/sandbox-protected-hooks-dir.md) | Resolved 2026-09-13: repo hook sources moved to loom-hooks/ | 47 |
| [sandbox-write-rules-inert](concerns/sandbox-write-rules-inert.md) | Sandbox Write() rules inert in generated stage settings | 62 |
| [state-confinement-gaps](concerns/state-confinement-gaps.md) | Confinement gaps: all closed by the 2026-09-14 merge except shared caches | 19 |
| [token-accounting-and-proof-defects](concerns/token-accounting-and-proof-defects.md) | Resolved plan defects and open token-accounting follow-ups | 119 |
| [web-dashboard-latent-issues](concerns/web-dashboard-latent-issues.md) | Issues reviewed in commands/status/web/ during integration-verify | 63 |
