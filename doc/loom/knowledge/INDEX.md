<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then only what it points to: the section for your area in a tier-1 summary (`rg -n '^## ' <file>` lists them) and the tier-2 topics your task touches. A specific question is cheaper to pull than to read — `loom knowledge context --query "..."` returns the matching sections quoted.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 235 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 241 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 165 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 239 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 249 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 119 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 246 |

## Tier 2 — Topics

### architecture

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-lifecycle](architecture/adjudication-lifecycle.md) | Dispute to durable verdict, and each verdict's effect | 60 |
| [codex-concurrency](architecture/codex-concurrency.md) | Codex fan-out limits, what is measured, what degrades | 128 |
| [codex-plugin](architecture/codex-plugin.md) | Codex plugin install, identity, and forwarding | 359 |
| [completion-recovery](architecture/completion-recovery.md) | Completion HMAC attestation, exit_reason, handoff folds | 34 |
| [config-value-types](architecture/config-value-types.md) | ConfigValue typed read-path across CLI/TUI/web/TS/React | 85 |
| [context-ceiling](architecture/context-ceiling.md) | Resident-token ceiling: tiers and thresholds | 111 |
| [context-retrieval](architecture/context-retrieval.md) | Retrieval: graphs, lanes, gating, tiered packs | 366 |
| [context-retrieval-corpus](architecture/context-retrieval-corpus.md) | Stopwording, rescue floor, BM25 index, indexed prose | 177 |
| [context-retrieval-state](architecture/context-retrieval-state.md) | Base/overlay graph layers, delivery records | 192 |
| [core-abstractions](architecture/core-abstractions.md) | ExecutionGraph, Stage, Session, Orchestrator, data flow | 136 |
| [directory-structure](architecture/directory-structure.md) | loom/src module tree, state layout, root assets | 49 |
| [execution-containment](architecture/execution-containment.md) | Sandboxed command containment and its limits | 343 |
| [hook-system](architecture/hook-system.md) | Hook embedding, SessionStart contract, enforcement | 259 |
| [knowledge-bootstrap](architecture/knowledge-bootstrap.md) | Deterministic phase, cluster digest, receipt semantics, session contract | 86 |
| [knowledge-hierarchy](architecture/knowledge-hierarchy.md) | fs/knowledge targets, INDEX.md, checks, baselines | 287 |
| [memory-spool](architecture/memory-spool.md) | Read before touching loom memory: spool/drain, ids, receipts | 159 |
| [merge-flow](architecture/merge-flow.md) | How a completed stage reaches its target branch | 79 |
| [orchestrator-loop](architecture/orchestrator-loop.md) | Daemon tick order, Monitor subsystem, heartbeat liveness | 56 |
| [owned-waits](architecture/owned-waits.md) | Worker-set waits: lease/engine, exit codes | 47 |
| [plan-lifecycle-and-fields](architecture/plan-lifecycle-and-fields.md) | Plan fields, goal-backward layers, verify checks | 150 |
| [quota-poller](architecture/quota-poller.md) | Claude/Codex usage-quota polling, caching, and rendering | 29 |
| [remote-control](architecture/remote-control.md) | Capability detection, preflight, and per-kind session naming | 82 |
| [security-and-isolation](architecture/security-and-isolation.md) | 4-layer worktree defense, security model | 193 |
| [signal-generation](architecture/signal-generation.md) | Signal assembly: cache, append helpers, prefixes | 198 |
| [skill-catalog](architecture/skill-catalog.md) | The two skill roots; 53 catalogued skills | 126 |
| [source-graph](architecture/source-graph.md) | Source graph honesty contract, extractor, limits | 330 |
| [status-data-model](architecture/status-data-model.md) | Where each loom status field comes from | 196 |
| [terminal-backends](architecture/terminal-backends.md) | Native and tmux session backends, lane resolution | 285 |
| [token-accounting-and-receipts](architecture/token-accounting-and-receipts.md) | Usage ledger, --compare, criterion cache | 254 |
| [web-dashboard](architecture/web-dashboard.md) | loom status --web: server, SPA, streaming | 79 |
| [web-terminal](architecture/web-terminal.md) | loom status --web --terminals: browser terminal | 169 |

### entry-points

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md) | CLI dispatch, plan parse/validate/graph, verification | 176 |
| [context-and-source-graph](entry-points/context-and-source-graph.md) | Context retrieval pipeline, source-graph channel | 53 |
| [filesystem-and-integration-modules](entry-points/filesystem-and-integration-modules.md) | Git, fs/work_dir, handoff, sandbox, remote control | 121 |
| [hooks](entry-points/hooks.md) | Hook scripts, their events, command matching | 146 |
| [orchestrator-daemon-and-sessions](entry-points/orchestrator-daemon-and-sessions.md) | Orchestrator loop, daemon, monitor, signals, merges | 224 |
| [remote-control](entry-points/remote-control.md) | Remote-control capability detection call sites | 99 |

### patterns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-process-and-conventions](patterns/cli-process-and-conventions.md) | CLI registration, TUI, errors, process mgmt, config, HTTP | 246 |
| [doctrine-cross-surface](patterns/doctrine-cross-surface.md) | Pinning multi-surface guidance with equality tests | 135 |
| [hook-content-stripping](patterns/hook-content-stripping.md) | How a hook decides what a Bash command invokes | 159 |
| [merge-and-recovery](patterns/merge-and-recovery.md) | Progressive merge, conflict recovery, attribution | 96 |
| [orchestrator-daemon-loop](patterns/orchestrator-daemon-loop.md) | Signal gen, daemon IPC, poll loop, heartbeat, spool drain | 107 |
| [remote-control](patterns/remote-control.md) | Detect-capability/preflight/resolve shape for external agents | 51 |
| [security-sandbox-and-hooks](patterns/security-sandbox-and-hooks.md) | Hooks, input validation, sandbox config | 105 |
| [stage-daemon-channels](patterns/stage-daemon-channels.md) | How a stage agent reaches the daemon to change its own state | 105 |
| [stage-lifecycle-and-verification](patterns/stage-lifecycle-and-verification.md) | Stage/session states, locked writes, verification | 170 |
| [subagent-hierarchy](patterns/subagent-hierarchy.md) | Fan-out vs coordinators vs teams; model mix | 91 |

### conventions

| Topic | Blurb | Lines |
| --- | --- | --- |
| [code-style-and-structure](conventions/code-style-and-structure.md) | Rust naming, errors, size limits, docstrings | 273 |
| [commits](conventions/commits.md) | Grouped Conventional Commits, no attribution, no trailers | 25 |
| [dispute-and-adjudication](conventions/dispute-and-adjudication.md) | Dispute file authority, adjudicator scope, budgets | 114 |
| [git-and-build-workflow](conventions/git-and-build-workflow.md) | Git/worktree ops, cargo discipline, the maintainability ledger | 206 |
| [guidance-channels-and-plugin-scope](conventions/guidance-channels-and-plugin-scope.md) | Guidance-channel choice, verification rule, plugin scope | 121 |
| [model-and-effort-config](conventions/model-and-effort-config.md) | [pressure]/[models] sections, precedence chain, value types | 69 |
| [plan-yaml-and-hooks](conventions/plan-yaml-and-hooks.md) | Plan YAML schema, hook stdin/stdout contract, skill format | 145 |
| [web-dashboard-typography](conventions/web-dashboard-typography.md) | Dashboard type conventions and CSS gotchas | 30 |

### mistakes

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-autonomy-deadlock](mistakes/adjudication-autonomy-deadlock.md) | Accepted-verdict deadlock: adoption, requeue | 188 |
| [ambient-filesystem-trust](mistakes/ambient-filesystem-trust.md) | Why a .git directory is not evidence of a real repository | 146 |
| [briefs-and-bug-reports](mistakes/briefs-and-bug-reports.md) | Stage bug reports; guard flags in briefs | 85 |
| [ci-toolchain-and-cargo](mistakes/ci-toolchain-and-cargo.md) | CI clippy drift, offline cargo audit, install.sh | 182 |
| [codex-lane-rogue-wrapper](mistakes/codex-lane-rogue-wrapper.md) | A wrapper implemented the task instead of forwarding it | 156 |
| [codex-navigation](mistakes/codex-navigation.md) | Forbidding reads instead of fixing a slow reader | 32 |
| [codex-worker-briefing](mistakes/codex-worker-briefing.md) | Codex brief pitfalls: braces, placeholders, path reuse | 70 |
| [completion-broker-credential](mistakes/completion-broker-credential.md) | Completion broker fallback, dup naming, exit-0 | 179 |
| [computed-values-and-hidden-couplings](mistakes/computed-values-and-hidden-couplings.md) | Values computed right but unread downstream | 208 |
| [concurrency-and-locking](mistakes/concurrency-and-locking.md) | Locked-handle writes and read-mutate-save races | 36 |
| [daemon-singleton](mistakes/daemon-singleton.md) | Two daemons once shared one .loom/work/; startup now flocks | 131 |
| [detached-spawn-in-tests](mistakes/detached-spawn-in-tests.md) | No process from a test may outlive the test process | 45 |
| [doctrine-and-acceptance](mistakes/doctrine-and-acceptance.md) | Doctrine drift, setup-line grants, completion rules | 337 |
| [hooks-shell-portability](mistakes/hooks-shell-portability.md) | gawk/bash portability, redirects, hook test env | 188 |
| [knowledge-base-drift](mistakes/knowledge-base-drift.md) | How the knowledge base goes stale: frozen notes, drift | 192 |
| [knowledge-cli-invariants](mistakes/knowledge-cli-invariants.md) | Invariants belong in the fs constructor, not the CLI handler | 139 |
| [knowledge-write-channel](mistakes/knowledge-write-channel.md) | Why distillation cannot write knowledge directly | 100 |
| [ledger-tui-rendering](mistakes/ledger-tui-rendering.md) | Ledger TUI wide-glyph padding, fan-out duplication, panics | 83 |
| [live-state-pollution](mistakes/live-state-pollution.md) | A stage test run rewrote live .loom/work state | 33 |
| [memory-relay-drain-gap](mistakes/memory-relay-drain-gap.md) | Relay tickets leaked when their line missed the hook | 34 |
| [merge-cleanup-boundary](mistakes/merge-cleanup-boundary.md) | A cleanup-boundary bug and its fix | 171 |
| [parallel-worktree-shared-state](mistakes/parallel-worktree-shared-state.md) | Cross-worktree state races: diagnosis, cases, fix | 160 |
| [phantom-merges](mistakes/phantom-merges.md) | Merge machinery lessons: merged=true without verifying | 171 |
| [pinned-literals-ledgers-and-wiring](mistakes/pinned-literals-ledgers-and-wiring.md) | Ledger exact-match trap and wiring-check pinning | 291 |
| [pre-commit-hardening](mistakes/pre-commit-hardening.md) | Partial-staging guard decisions and edge cases | 53 |
| [refactor-stragglers](mistakes/refactor-stragglers.md) | What a large removal or rename leaves behind, uncleaned | 106 |
| [sandbox-and-settings](mistakes/sandbox-and-settings.md) | Sandbox path rules, permission sync, merge traps | 293 |
| [sandbox-protected-hooks-dir](mistakes/sandbox-protected-hooks-dir.md) | A directory named hooks/ is sandbox write-protected. | 37 |
| [sandbox-state-channels](mistakes/sandbox-state-channels.md) | Sandboxed callers vs .loom/work state: memory, handoff, socket | 252 |
| [sandbox-tooling-and-network](mistakes/sandbox-tooling-and-network.md) | Stage-sandbox tool failures: sccache, audit, loopback | 222 |
| [sandbox-write-rules-inert](mistakes/sandbox-write-rules-inert.md) | Only Edit(path) rules are enforced; Write(path) is ignored | 57 |
| [schema-reuse-and-silent-skips](mistakes/schema-reuse-and-silent-skips.md) | deny_unknown_fields with two deserialization sources | 130 |
| [session-identity-env](mistakes/session-identity-env.md) | LOOM_* wrapper exports: a contract for hooks, CLI, daemon | 104 |
| [sessions-and-liveness](mistakes/sessions-and-liveness.md) | Session identity, liveness routing, coverage | 347 |
| [shell-command-matchers](mistakes/shell-command-matchers.md) | Separators that never tokenize; forgeable glob lookups | 246 |
| [spurious-waiting-for-input](mistakes/spurious-waiting-for-input.md) | Stages flipped to waiting-for-input with no AskUserQuestion | 35 |
| [status-broadcast-hardening](mistakes/status-broadcast-hardening.md) | Frame-overflow eviction, read-timeout desync in status | 74 |
| [store-without-consumer](mistakes/store-without-consumer.md) | A store written but never read | 94 |
| [subagent-briefing](mistakes/subagent-briefing.md) | Writing briefs, sizing waves, delegation, file ownership | 265 |
| [subagent-liveness-and-watch](mistakes/subagent-liveness-and-watch.md) | Subagent alive/done/dead detection; watch traps | 321 |
| [subagent-orchestration](mistakes/subagent-orchestration.md) | Delegation model, defect reports, codex/pressure gotchas | 110 |
| [test-concurrency-and-fixtures](mistakes/test-concurrency-and-fixtures.md) | Racy tests: fds, ETXTBSY, serial env, stdin hangs | 208 |
| [testing-and-lint](mistakes/testing-and-lint.md) | Lint/test discipline: --all-targets, --no-fail-fast | 324 |
| [tests-that-cannot-fail](mistakes/tests-that-cannot-fail.md) | Tests that pass whether the bug is present | 274 |
| [tmux-backend](mistakes/tmux-backend.md) | tmux spawn-failure exit codes and cleanup-on-error discipline | 136 |
| [typed-config-values-process](mistakes/typed-config-values-process.md) | Verification-brief, dev-server, plan-prose gotchas | 43 |
| [untrusted-value-boundaries](mistakes/untrusted-value-boundaries.md) | Enumerate every producer of a rendered field | 188 |
| [verification-harness](mistakes/verification-harness.md) | When checks fail at once, suspect the harness | 369 |
| [visibility-and-reachability](mistakes/visibility-and-reachability.md) | pub(crate) visibility is capped by path | 123 |
| [web-dashboard-server](mistakes/web-dashboard-server.md) | Dashboard server: concurrency, security, tests | 294 |
| [writer-reader-address](mistakes/writer-reader-address.md) | A layer written under a key its reader ignores | 73 |

### concerns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [agent-rule-bending-hardening](concerns/agent-rule-bending-hardening.md) | Checks an agent can bend, and the hardening backlog | 213 |
| [automatic-knowledge-source-graph-followups](concerns/automatic-knowledge-source-graph-followups.md) | Knowledge-plan followups: retrieval gap, stopwording | 84 |
| [code-quality-and-hook-debt](concerns/code-quality-and-hook-debt.md) | Oversized units, debug logging, duplicated tables, hook debt | 195 |
| [codex-heartbeat-starvation](concerns/codex-heartbeat-starvation.md) | Heartbeat starvation from long codex runs | 73 |
| [iterm2-window-teardown](concerns/iterm2-window-teardown.md) | iTerm2 window never named, so teardown cannot close it | 48 |
| [knowledge-cli-gaps](concerns/knowledge-cli-gaps.md) | Knowledge CLI gaps: heading rename, CRLF, fences, housekeeping | 101 |
| [merge-and-recovery-edge-cases](concerns/merge-and-recovery-edge-cases.md) | Merge/retry/completion edge cases | 82 |
| [runtime-and-session-safety](concerns/runtime-and-session-safety.md) | Runtime edge cases: tmux, attach lifetime, orphan adoption | 139 |
| [sandbox-and-confinement-gaps](concerns/sandbox-and-confinement-gaps.md) | Sandbox gaps: no E2E canary, diverging env allowlists | 168 |
| [state-confinement-gaps](concerns/state-confinement-gaps.md) | Shared package-manager caches stay session-writable. | 9 |
| [token-accounting-and-proof-defects](concerns/token-accounting-and-proof-defects.md) | Open follow-ups from the token-optimization and efficiency plans | 92 |
| [typed-config-values](concerns/typed-config-values.md) | Accepted gaps in the config read-path | 27 |
| [web-dashboard-latent-issues](concerns/web-dashboard-latent-issues.md) | Latent issues found in commands/status/web/ | 88 |
