<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then the tier-1 summary for the area you're working in, then only the tier-2 topics you actually need. Tier-1 files are short summaries with links out; tier-2 files hold the full detail.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 516 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 591 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 752 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 591 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 964 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 97 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 785 |

## Tier 2 — Topics

### architecture

| Topic | Title | Blurb | Lines |
| --- | --- | --- | --- |
| [architecture/codex-concurrency.md](architecture/codex-concurrency.md) | Codex Concurrency | Topic notes for the architecture knowledge area. | 102 |
| [architecture/codex-plugin.md](architecture/codex-plugin.md) | Codex Plugin | Topic notes for the architecture knowledge area. | 254 |
| [architecture/context-retrieval.md](architecture/context-retrieval.md) | Context Retrieval | Topic notes for the architecture knowledge area. | 469 |
| [architecture/core-abstractions.md](architecture/core-abstractions.md) | Core Abstractions | ExecutionGraph, Stage, Session, Orchestrator, TerminalBackend — plus data flow and .work/ file ownership. | 93 |
| [architecture/directory-structure.md](architecture/directory-structure.md) | Directory Structure | Full loom/src module tree, the .work/ state layout, and the repo-root asset directories. | 49 |
| [architecture/execution-containment.md](architecture/execution-containment.md) | Execution Containment | Topic notes for the architecture knowledge area. | 193 |
| [architecture/hook-system.md](architecture/hook-system.md) | Hook System | Hook embedding and install, the SessionStart hookSpecificOutput contract, and the two subagent enforcement hooks. | 110 |
| [architecture/knowledge-hierarchy.md](architecture/knowledge-hierarchy.md) | Knowledge Hierarchy | Tier-1/tier-2 knowledge mechanics: layout predicate, target parsing, INDEX.md generation, audit link rules, coverage blast radius, opt-in migration, lock ordering. | 130 |
| [architecture/memory-spool.md](architecture/memory-spool.md) | Memory Spool and Drain | Topic notes for the architecture knowledge area. | 105 |
| [architecture/remote-control.md](architecture/remote-control.md) | Remote Control | Capability detection, preflight, resolution, and per-kind session naming for driving external agent binaries. | 64 |
| [architecture/signal-generation.md](architecture/signal-generation.md) | Signal Generation | How a stage signal is assembled: stable-prefix cache, shared append_* helpers, per-stage-type prefixes, soft signals. | 172 |
| [architecture/source-graph.md](architecture/source-graph.md) | Source Graph | Topic notes for the architecture knowledge area. | 240 |
| [architecture/terminal-backends.md](architecture/terminal-backends.md) | Terminal Backends | Topic notes for the architecture knowledge area. | 196 |

### entry-points

| Topic | Title | Blurb | Lines |
| --- | --- | --- | --- |
| [entry-points/hooks.md](entry-points/hooks.md) | Hooks | Every hook script and the event it binds to, _common.sh's seven helpers, and the registration sites a new hook needs. | 96 |
| [entry-points/remote-control.md](entry-points/remote-control.md) | Remote Control | Files and call sites for remote-control capability detection and permission-mode resolution. | 97 |

### patterns

| Topic | Title | Blurb | Lines |
| --- | --- | --- | --- |
| [patterns/doctrine-cross-surface.md](patterns/doctrine-cross-surface.md) | Doctrine Cross Surface | Pinning multi-surface guidance with equality tests, ambiguity-equals-fail-safe privilege lookups, and token-based shell classification. | 67 |
| [patterns/hook-content-stripping.md](patterns/hook-content-stripping.md) | Hook Content Stripping | Stripping heredoc bodies and -m text before matching, the full hook inventory, and the limits of that stripping. | 132 |
| [patterns/remote-control.md](patterns/remote-control.md) | Remote Control | The detect-capability, preflight, resolve-invocation shape for external agent binaries. | 50 |
| [patterns/subagent-hierarchy.md](patterns/subagent-hierarchy.md) | Subagent Hierarchy | Flat fan-out vs 2-level coordinator hierarchy vs agent teams: when to use each, model mix, file exclusivity. | 62 |

### mistakes

| Topic | Title | Blurb | Lines |
| --- | --- | --- | --- |
| [mistakes/codex-lane-rogue-wrapper.md](mistakes/codex-lane-rogue-wrapper.md) | Codex Lane Rogue Wrapper | Topic notes for the mistakes knowledge area. | 79 |
| [mistakes/completion-broker-credential.md](mistakes/completion-broker-credential.md) | Completion Broker Credential | Topic notes for the mistakes knowledge area. | 57 |
| [mistakes/computed-values-and-hidden-couplings.md](mistakes/computed-values-and-hidden-couplings.md) | Computed Values and Hidden Couplings | Topic notes for the mistakes knowledge area. Three lessons from the | 156 |
| [mistakes/detached-spawn-in-tests.md](mistakes/detached-spawn-in-tests.md) | Detached Spawn In Tests | Topic notes for the mistakes knowledge area. | 45 |
| [mistakes/doctrine-and-acceptance.md](mistakes/doctrine-and-acceptance.md) | Doctrine And Acceptance | Why a one-phrase grep proves presence but never agreement, and how doctrine drifts across surfaces unnoticed. | 110 |
| [mistakes/knowledge-base-drift.md](mistakes/knowledge-base-drift.md) | Knowledge Base Drift | How the knowledge base itself goes stale: plan-authoring notes frozen as architecture facts, `[UPDATED]` duplicates, and invented CLI surface. | 103 |
| [mistakes/knowledge-cli-invariants.md](mistakes/knowledge-cli-invariants.md) | Knowledge Cli Invariants | Invariants belong in the fs constructor, not the CLI handler; lock ordering for sibling refreshes; update appends. | 74 |
| [mistakes/knowledge-write-channel.md](mistakes/knowledge-write-channel.md) | Knowledge Write Channel | Topic notes for the mistakes knowledge area. | 100 |
| [mistakes/merge-cleanup-boundary.md](mistakes/merge-cleanup-boundary.md) | Merge Cleanup Boundary | Topic notes for the mistakes knowledge area. | 75 |
| [mistakes/parallel-worktree-shared-state.md](mistakes/parallel-worktree-shared-state.md) | Parallel Worktree Shared State | Topic notes for the mistakes knowledge area. | 104 |
| [mistakes/phantom-merges.md](mistakes/phantom-merges.md) | Phantom Merges | Seven lessons on loom's merge machinery — writing merged=true without verifying git ancestry (the costliest recurring failure class in loom), plus the preflight guards and session lifecycle around it. | 131 |
| [mistakes/pinned-literals-ledgers-and-wiring.md](mistakes/pinned-literals-ledgers-and-wiring.md) | Pinned Literals Ledgers And Wiring | Topic notes for the mistakes knowledge area. | 110 |
| [mistakes/refactor-stragglers.md](mistakes/refactor-stragglers.md) | Refactor Stragglers | What a large removal or rename leaves behind: straggler initializers, stale comments, stale docs, duplicate modules. | 62 |
| [mistakes/sandbox-and-settings.md](mistakes/sandbox-and-settings.md) | Sandbox And Settings | Sandbox path rules, permission sync, excludedCommands matching, and settings env leaking between main repo and worktrees. | 340 |
| [mistakes/schema-reuse-and-silent-skips.md](mistakes/schema-reuse-and-silent-skips.md) | Schema Reuse And Silent Skips | Topic notes for the mistakes knowledge area. | 56 |
| [mistakes/session-identity-env.md](mistakes/session-identity-env.md) | Session Identity Env | The wrapper script's `LOOM_*` exports are a contract read by hooks, the CLI and the daemon. Two long-standing defects in that contract made knowledge stages impossible to complete. | 83 |
| [mistakes/sessions-and-liveness.md](mistakes/sessions-and-liveness.md) | Sessions And Liveness | Session identity, liveness routing, spawn-site coverage, and the blast radius of adding a session field. | 136 |
| [mistakes/shell-command-matchers.md](mistakes/shell-command-matchers.md) | Shell Command Matchers | Separators that never become tokens, forgeable glob lookups, env leakage in hook tests, and three Bash traps. | 187 |
| [mistakes/store-without-consumer.md](mistakes/store-without-consumer.md) | Store Without Consumer | Topic notes for the mistakes knowledge area. | 94 |
| [mistakes/subagent-orchestration.md](mistakes/subagent-orchestration.md) | Subagent Orchestration | Topic notes for the mistakes knowledge area. | 145 |
| [mistakes/testing-and-lint.md](mistakes/testing-and-lint.md) | Testing And Lint | Lint and test discipline: --all-targets, --no-fail-fast, headless CI, ambient git config and inherited descriptors in tests, the stub checker, the maintainability ledger, and reviewer claims. | 190 |
| [mistakes/tests-that-cannot-fail.md](mistakes/tests-that-cannot-fail.md) | Tests That Cannot Fail | Topic notes for the mistakes knowledge area. | 124 |
| [mistakes/tmux-backend.md](mistakes/tmux-backend.md) | Tmux Backend | Topic notes for the mistakes knowledge area. | 120 |
| [mistakes/untrusted-value-boundaries.md](mistakes/untrusted-value-boundaries.md) | Untrusted Value Boundaries | Topic notes for the mistakes knowledge area. | 78 |
| [mistakes/verification-harness.md](mistakes/verification-harness.md) | Verification Harness | When every check fails at once, suspect the harness; the PATH binary is not your build; silent subagents are failed delegations. | 107 |
| [mistakes/visibility-and-reachability.md](mistakes/visibility-and-reachability.md) | Visibility And Reachability | Topic notes for the mistakes knowledge area. | 98 |
| [mistakes/writer-reader-address.md](mistakes/writer-reader-address.md) | Writer/Reader Address | Topic notes for the mistakes knowledge area. | 71 |

### concerns

| Topic | Title | Blurb | Lines |
| --- | --- | --- | --- |
| [concerns/daemon-singleton.md](concerns/daemon-singleton.md) | Daemon Singleton (Resolved 2026-08-08) | Historical incident: two daemons once attached to the same `.work/`. Startup now holds an | 98 |
| [concerns/sandbox-write-rules-inert.md](concerns/sandbox-write-rules-inert.md) | Sandbox Write Rules Inert | Topic notes for the concerns knowledge area. | 59 |
