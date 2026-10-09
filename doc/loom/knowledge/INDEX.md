<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then only what it points to: the section for your area in a tier-1 summary (`rg -n '^## ' <file>` lists them) and the tier-2 topics your task touches. A specific question is cheaper to pull than to read — `loom knowledge context --query "..."` returns the matching sections quoted.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 249 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 245 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 186 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 247 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 250 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 125 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 247 |

## Tier 2 — Topics

### architecture

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-lifecycle](architecture/adjudication-lifecycle.md) | Dispute to verdict, per-kind rulings | 143 |
| [codex-concurrency](architecture/codex-concurrency.md) | Codex fan-out limits and degradation | 128 |
| [codex-plugin](architecture/codex-plugin.md) | Codex plugin install, forwarding | 364 |
| [completion-recovery](architecture/completion-recovery.md) | HMAC attestation, exit_reason | 34 |
| [config-value-types](architecture/config-value-types.md) | ConfigValue typed read-path | 85 |
| [context-ceiling](architecture/context-ceiling.md) | Resident-token ceiling tiers | 111 |
| [context-retrieval](architecture/context-retrieval.md) | Retrieval: graphs, lanes, gating, packs | 393 |
| [context-retrieval-corpus](architecture/context-retrieval-corpus.md) | Stopwording, rescue floor, BM25 | 177 |
| [context-retrieval-routing](architecture/context-retrieval-routing.md) | Intent routing, neighbours, caveats | 75 |
| [context-retrieval-state](architecture/context-retrieval-state.md) | Base/overlay layers, delivery records | 192 |
| [contract-phase](architecture/contract-phase.md) | Contract session, freeze, handover | 144 |
| [core-abstractions](architecture/core-abstractions.md) | ExecutionGraph, Stage, Session | 136 |
| [daemon-launch](architecture/daemon-launch.md) | Re-exec launch, readiness, socket path | 65 |
| [daemon-owned-commits](architecture/daemon-owned-commits.md) | Relayed signed commits | 143 |
| [directory-structure](architecture/directory-structure.md) | Module tree, state layout, root assets | 49 |
| [execution-containment](architecture/execution-containment.md) | Sandboxed command containment, limits | 397 |
| [hook-system](architecture/hook-system.md) | Hook embedding, SessionStart | 277 |
| [knowledge-bootstrap](architecture/knowledge-bootstrap.md) | Deterministic phase, digest, receipts | 86 |
| [knowledge-hierarchy](architecture/knowledge-hierarchy.md) | fs/knowledge, INDEX.md, checks | 261 |
| [memory-spool](architecture/memory-spool.md) | Read before touching loom memory | 188 |
| [merge-checkout-state](architecture/merge-checkout-state.md) | Guarded fast-forward; blocked retry | 28 |
| [merge-flow](architecture/merge-flow.md) | Finished stage to target branch | 183 |
| [orchestrator-loop](architecture/orchestrator-loop.md) | Tick order, Monitor, liveness | 56 |
| [owned-waits](architecture/owned-waits.md) | Worker-set waits, lease, boot ID | 57 |
| [plan-lifecycle-and-fields](architecture/plan-lifecycle-and-fields.md) | Plan fields v1/v2, checks, lints | 252 |
| [quota-poller](architecture/quota-poller.md) | Usage-quota polling and rendering | 31 |
| [remote-control](architecture/remote-control.md) | Capability detection, preflight, naming | 81 |
| [security-and-isolation](architecture/security-and-isolation.md) | 4-layer worktree defense | 212 |
| [signal-generation](architecture/signal-generation.md) | Signal assembly, cache, prefixes | 236 |
| [skill-catalog](architecture/skill-catalog.md) | Two skill roots; 65 catalogued skills | 153 |
| [source-graph](architecture/source-graph.md) | Source graph contract, limits | 368 |
| [source-graph-build](architecture/source-graph-build.md) | Layer builder, freshness, lease | 124 |
| [source-graph-evaluation](architecture/source-graph-evaluation.md) | Census, source windows, edge evaluator | 68 |
| [source-graph-resolution](architecture/source-graph-resolution.md) | Binding rules 1-7, path conventions | 123 |
| [source-graph-view](architecture/source-graph-view.md) | Resolved view, relink equals cold | 76 |
| [status-data-model](architecture/status-data-model.md) | Source of each status field | 226 |
| [target-guard](architecture/target-guard.md) | Accepted-tip record, hook, holds | 127 |
| [terminal-backends](architecture/terminal-backends.md) | Native and tmux session backends | 285 |
| [test-runner-adapters](architecture/test-runner-adapters.md) | 23 adapters, profiles, quoting, detect | 49 |
| [token-accounting-and-receipts](architecture/token-accounting-and-receipts.md) | Usage ledger, --compare, cache | 260 |
| [verification-v2-gates](architecture/verification-v2-gates.md) | v2 completion gates, order, owners | 135 |
| [web-dashboard](architecture/web-dashboard.md) | status --web server, SPA, streams | 95 |
| [web-terminal](architecture/web-terminal.md) | loom status --web terminals | 175 |

### entry-points

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md) | CLI dispatch, plan pipeline | 192 |
| [context-and-source-graph](entry-points/context-and-source-graph.md) | Context retrieval, source graph | 79 |
| [filesystem-and-integration-modules](entry-points/filesystem-and-integration-modules.md) | Git, fs, handoff, sandbox, remote | 126 |
| [hooks](entry-points/hooks.md) | Hook scripts, events, matching | 147 |
| [orchestrator-daemon-and-sessions](entry-points/orchestrator-daemon-and-sessions.md) | Orchestrator, daemon, signals, merges | 234 |
| [remote-control](entry-points/remote-control.md) | Remote-control detection call sites | 99 |

### patterns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-process-and-conventions](patterns/cli-process-and-conventions.md) | CLI registration, TUI, errors, config | 249 |
| [doctrine-cross-surface](patterns/doctrine-cross-surface.md) | Pinning multi-surface guidance | 135 |
| [hook-content-stripping](patterns/hook-content-stripping.md) | How hooks read a Bash command | 161 |
| [merge-and-recovery](patterns/merge-and-recovery.md) | Progressive merge, conflict recovery | 120 |
| [orchestrator-daemon-loop](patterns/orchestrator-daemon-loop.md) | Signal gen, IPC, poll loop, spool | 110 |
| [remote-control](patterns/remote-control.md) | Remote Control detect and resolve | 51 |
| [security-sandbox-and-hooks](patterns/security-sandbox-and-hooks.md) | Hooks, validation, sandbox config | 137 |
| [stage-daemon-channels](patterns/stage-daemon-channels.md) | How a stage agent reaches the daemon | 117 |
| [stage-lifecycle-and-verification](patterns/stage-lifecycle-and-verification.md) | Stage states, locked writes, verify | 198 |
| [subagent-hierarchy](patterns/subagent-hierarchy.md) | Fan-out, coordinators, teams | 98 |

### conventions

| Topic | Blurb | Lines |
| --- | --- | --- |
| [code-style-and-structure](conventions/code-style-and-structure.md) | Rust naming, errors, size limits | 273 |
| [commits](conventions/commits.md) | Grouped Conventional Commits | 25 |
| [dispute-and-adjudication](conventions/dispute-and-adjudication.md) | Dispute authority, budgets | 119 |
| [git-and-build-workflow](conventions/git-and-build-workflow.md) | Git, cargo, CI paths, size ledger | 243 |
| [guidance-channels-and-plugin-scope](conventions/guidance-channels-and-plugin-scope.md) | Guidance channels, plugin scope | 126 |
| [model-and-effort-config](conventions/model-and-effort-config.md) | [models] precedence, value types | 103 |
| [plan-yaml-and-hooks](conventions/plan-yaml-and-hooks.md) | Plan YAML, hook I/O, skills | 168 |
| [web-dashboard-typography](conventions/web-dashboard-typography.md) | Dashboard type and CSS gotchas | 30 |

### mistakes

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-autonomy-deadlock](mistakes/adjudication-autonomy-deadlock.md) | Accepted-verdict deadlock | 200 |
| [ambient-filesystem-trust](mistakes/ambient-filesystem-trust.md) | A .git dir is not evidence of a repo | 156 |
| [briefs-and-bug-reports](mistakes/briefs-and-bug-reports.md) | Stage bug reports; brief guard flags | 116 |
| [ci-toolchain-and-cargo](mistakes/ci-toolchain-and-cargo.md) | CI drift, cargo audit, install.sh | 222 |
| [codex-lane-rogue-wrapper](mistakes/codex-lane-rogue-wrapper.md) | Wrapper implemented, not forwarded | 157 |
| [codex-navigation](mistakes/codex-navigation.md) | Slow reader fixed by forbidding reads | 52 |
| [codex-worker-briefing](mistakes/codex-worker-briefing.md) | Codex brief pitfalls | 94 |
| [completion-broker-credential](mistakes/completion-broker-credential.md) | Broker fallback, dup naming, exit-0 | 195 |
| [computed-values-and-hidden-couplings](mistakes/computed-values-and-hidden-couplings.md) | Computed values unread downstream | 235 |
| [concurrency-and-locking](mistakes/concurrency-and-locking.md) | Locked-handle writes, save races | 46 |
| [daemon-fork-after-threads](mistakes/daemon-fork-after-threads.md) | Fork after threads, early ready byte | 38 |
| [daemon-singleton](mistakes/daemon-singleton.md) | Two daemons shared .loom/work | 141 |
| [detached-spawn-in-tests](mistakes/detached-spawn-in-tests.md) | No process may outlive its test | 55 |
| [doctrine-and-acceptance](mistakes/doctrine-and-acceptance.md) | Doctrine drift, acceptance | 367 |
| [hooks-shell-portability](mistakes/hooks-shell-portability.md) | gawk/bash portability, hook tests | 212 |
| [knowledge-base-drift](mistakes/knowledge-base-drift.md) | How knowledge goes stale | 229 |
| [knowledge-cli-invariants](mistakes/knowledge-cli-invariants.md) | Invariants live in constructor | 154 |
| [knowledge-write-channel](mistakes/knowledge-write-channel.md) | Why distillation cannot write directly | 100 |
| [ledger-tui-rendering](mistakes/ledger-tui-rendering.md) | Ledger TUI padding, fan-out, panics | 83 |
| [live-state-pollution](mistakes/live-state-pollution.md) | Tests wrote live state and HOME | 43 |
| [memory-relay-drain-gap](mistakes/memory-relay-drain-gap.md) | Relay tickets leaked past the hook | 34 |
| [merge-cleanup-boundary](mistakes/merge-cleanup-boundary.md) | A cleanup-boundary bug and its fix | 171 |
| [merge-in-operator-checkout](mistakes/merge-in-operator-checkout.md) | Git in operator checkout; plan errors | 70 |
| [parallel-worktree-shared-state](mistakes/parallel-worktree-shared-state.md) | Cross-worktree state races | 203 |
| [phantom-merges](mistakes/phantom-merges.md) | Merge lessons: merged=true unverified | 257 |
| [pinned-literals-ledgers-and-wiring](mistakes/pinned-literals-ledgers-and-wiring.md) | Ledger exact-match, wiring pins | 303 |
| [pre-commit-hardening](mistakes/pre-commit-hardening.md) | Partial-staging guard decisions, edges | 53 |
| [refactor-stragglers](mistakes/refactor-stragglers.md) | What a rename leaves behind | 146 |
| [sandbox-and-settings](mistakes/sandbox-and-settings.md) | Sandbox path rules, permission sync | 300 |
| [sandbox-protected-hooks-dir](mistakes/sandbox-protected-hooks-dir.md) | hooks/ dir is sandbox-protected | 37 |
| [sandbox-state-channels](mistakes/sandbox-state-channels.md) | Sandboxed callers vs work state | 298 |
| [sandbox-tooling-and-network](mistakes/sandbox-tooling-and-network.md) | Sandbox tool failures | 263 |
| [sandbox-write-rules-inert](mistakes/sandbox-write-rules-inert.md) | Only Edit(path) rules are enforced | 57 |
| [schema-reuse-and-silent-skips](mistakes/schema-reuse-and-silent-skips.md) | deny_unknown_fields, two sources | 130 |
| [session-identity-env](mistakes/session-identity-env.md) | LOOM_* wrapper exports contract | 126 |
| [sessions-and-liveness](mistakes/sessions-and-liveness.md) | Session identity, liveness, coverage | 366 |
| [shell-command-matchers](mistakes/shell-command-matchers.md) | Glued separators, forgeable lookups | 246 |
| [source-graph-delivery](mistakes/source-graph-delivery.md) | Resolver, cache, wiring, eval | 172 |
| [spurious-waiting-for-input](mistakes/spurious-waiting-for-input.md) | Stages flipped to waiting-for-input | 35 |
| [status-broadcast-hardening](mistakes/status-broadcast-hardening.md) | Frame overflow, read desync | 74 |
| [store-without-consumer](mistakes/store-without-consumer.md) | Store written, never read | 94 |
| [subagent-briefing](mistakes/subagent-briefing.md) | Briefs, wave sizing, ownership | 322 |
| [subagent-liveness-and-watch](mistakes/subagent-liveness-and-watch.md) | Subagent liveness; watch traps | 352 |
| [subagent-orchestration](mistakes/subagent-orchestration.md) | Delegation, defect reports | 130 |
| [test-concurrency-and-fixtures](mistakes/test-concurrency-and-fixtures.md) | Racy tests: fds, ETXTBSY, stdin | 217 |
| [testing-and-lint](mistakes/testing-and-lint.md) | Lint/test discipline | 336 |
| [tests-that-cannot-fail](mistakes/tests-that-cannot-fail.md) | Tests that pass with the bug | 302 |
| [tmux-backend](mistakes/tmux-backend.md) | tmux spawn failures, cleanup | 136 |
| [typed-config-values-process](mistakes/typed-config-values-process.md) | Brief, dev-server, plan-prose gotchas | 43 |
| [untrusted-value-boundaries](mistakes/untrusted-value-boundaries.md) | Producers of a rendered field | 188 |
| [verification-harness](mistakes/verification-harness.md) | Many checks failing: suspect harness | 371 |
| [verification-v2-delivery](mistakes/verification-v2-delivery.md) | Wave, gate, proof misses in v2 | 257 |
| [visibility-and-reachability](mistakes/visibility-and-reachability.md) | pub(crate) capped by path | 123 |
| [web-dashboard-server](mistakes/web-dashboard-server.md) | Dashboard server concurrency | 328 |
| [writer-reader-address](mistakes/writer-reader-address.md) | Layer written under an ignored key | 94 |

### concerns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [agent-rule-bending-hardening](concerns/agent-rule-bending-hardening.md) | Bendable checks, hardening backlog | 256 |
| [automatic-knowledge-source-graph-followups](concerns/automatic-knowledge-source-graph-followups.md) | Knowledge-plan followups | 51 |
| [code-quality-and-hook-debt](concerns/code-quality-and-hook-debt.md) | Oversized units, hook debt | 195 |
| [codex-heartbeat-starvation](concerns/codex-heartbeat-starvation.md) | Heartbeat starvation, stall limits | 66 |
| [iterm2-window-teardown](concerns/iterm2-window-teardown.md) | iTerm2 window never named | 49 |
| [knowledge-cli-gaps](concerns/knowledge-cli-gaps.md) | Knowledge CLI gaps and housekeeping | 105 |
| [merge-and-recovery-edge-cases](concerns/merge-and-recovery-edge-cases.md) | Merge/retry/completion edge cases | 147 |
| [platform-and-commit-gaps](concerns/platform-and-commit-gaps.md) | Open gaps: commits, launch, stalls | 183 |
| [runtime-and-session-safety](concerns/runtime-and-session-safety.md) | tmux, orphan adoption edge cases | 150 |
| [sandbox-and-confinement-gaps](concerns/sandbox-and-confinement-gaps.md) | Sandbox gaps: canary, creds, env | 304 |
| [source-graph-known-gaps](concerns/source-graph-known-gaps.md) | Language limits, open decisions | 79 |
| [source-graph-review-backlog](concerns/source-graph-review-backlog.md) | Unimplemented reviewer suggestions | 242 |
| [state-confinement-gaps](concerns/state-confinement-gaps.md) | Shared package caches session-writable | 17 |
| [token-accounting-and-proof-defects](concerns/token-accounting-and-proof-defects.md) | Token-optimization follow-ups | 92 |
| [typed-config-values](concerns/typed-config-values.md) | Accepted gaps in the config read-path | 27 |
| [verification-v2-followups](concerns/verification-v2-followups.md) | v2 adapters, parser gaps, known gaps | 118 |
| [web-dashboard-latent-issues](concerns/web-dashboard-latent-issues.md) | Latent issues in status/web | 94 |
