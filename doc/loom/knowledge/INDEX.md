<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then only what it points to: the section for your area in a tier-1 summary (`rg -n '^## ' <file>` lists them) and the tier-2 topics your task touches. A specific question is cheaper to pull than to read — `loom knowledge context --query "..."` returns the matching sections quoted.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 249 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 68 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 186 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 250 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 169 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 125 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 237 |

## Tier 2 — Topics

### architecture

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-lifecycle](architecture/adjudication-lifecycle.md) | Verdicts | 166 |
| [codex-concurrency](architecture/codex-concurrency.md) | Codex limits | 128 |
| [codex-plugin](architecture/codex-plugin.md) | Codex plugin | 391 |
| [completion-recovery](architecture/completion-recovery.md) | Attestation | 36 |
| [config-value-types](architecture/config-value-types.md) | Typed | 85 |
| [context-ceiling](architecture/context-ceiling.md) | Ceiling tiers | 111 |
| [context-retrieval](architecture/context-retrieval.md) | Retrieval | 397 |
| [context-retrieval-corpus](architecture/context-retrieval-corpus.md) | BM25 | 178 |
| [context-retrieval-routing](architecture/context-retrieval-routing.md) | Routing | 75 |
| [context-retrieval-state](architecture/context-retrieval-state.md) | Layers | 193 |
| [contract-collection](architecture/contract-collection.md) | Scaffold | 70 |
| [contract-phase](architecture/contract-phase.md) | Freeze | 226 |
| [core-abstractions](architecture/core-abstractions.md) | Graph, Stage | 135 |
| [daemon-launch](architecture/daemon-launch.md) | Launch | 65 |
| [daemon-owned-commits](architecture/daemon-owned-commits.md) | Commits | 151 |
| [directory-structure](architecture/directory-structure.md) | Module tree | 50 |
| [execution-containment](architecture/execution-containment.md) | Containment | 389 |
| [hook-system](architecture/hook-system.md) | Hooks | 307 |
| [knowledge-bootstrap](architecture/knowledge-bootstrap.md) | Digest phase | 83 |
| [knowledge-hierarchy](architecture/knowledge-hierarchy.md) | Knowledge fs | 264 |
| [memory-spool](architecture/memory-spool.md) | Memory relay | 197 |
| [merge-checkout-state](architecture/merge-checkout-state.md) | Fast-forward | 28 |
| [merge-flow](architecture/merge-flow.md) | Stage merge | 134 |
| [orchestrator-loop](architecture/orchestrator-loop.md) | Tick order | 79 |
| [owned-waits](architecture/owned-waits.md) | Worker waits | 70 |
| [plan-lifecycle-and-fields](architecture/plan-lifecycle-and-fields.md) | Plan fields | 257 |
| [plan-server](architecture/plan-server.md) | Plan server | 193 |
| [planning-document-model](architecture/planning-document-model.md) | Doc model | 172 |
| [quota-poller](architecture/quota-poller.md) | Polling | 35 |
| [remote-control](architecture/remote-control.md) | Capability | 81 |
| [security-and-isolation](architecture/security-and-isolation.md) | Isolation | 186 |
| [signal-generation](architecture/signal-generation.md) | Signals | 241 |
| [skill-catalog](architecture/skill-catalog.md) | Catalog | 155 |
| [source-graph](architecture/source-graph.md) | Contract | 369 |
| [source-graph-build](architecture/source-graph-build.md) | Builder | 128 |
| [source-graph-evaluation](architecture/source-graph-evaluation.md) | Evaluator | 68 |
| [source-graph-resolution](architecture/source-graph-resolution.md) | Bindings | 123 |
| [source-graph-view](architecture/source-graph-view.md) | Resolved view | 76 |
| [status-data-model](architecture/status-data-model.md) | Fields | 239 |
| [summary-store](architecture/summary-store.md) | Summary store | 146 |
| [target-guard](architecture/target-guard.md) | Accepted tip | 127 |
| [terminal-backends](architecture/terminal-backends.md) | Backends | 280 |
| [test-runner-adapters](architecture/test-runner-adapters.md) | Adapters | 57 |
| [token-accounting-and-receipts](architecture/token-accounting-and-receipts.md) | Usage ledger | 369 |
| [token-flow-sources](architecture/token-flow-sources.md) | Token flow | 75 |
| [usage-evidence](architecture/usage-evidence.md) | Evidence | 127 |
| [verification-v2-gates](architecture/verification-v2-gates.md) | v2 gates | 238 |
| [web-dashboard](architecture/web-dashboard.md) | status --web | 110 |
| [web-terminal](architecture/web-terminal.md) | Terminals | 181 |

### entry-points

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md) | CLI dispatch | 191 |
| [context-and-source-graph](entry-points/context-and-source-graph.md) | Retrieval | 80 |
| [filesystem-and-integration-modules](entry-points/filesystem-and-integration-modules.md) | Fs, git | 133 |
| [hooks](entry-points/hooks.md) | Hook scripts | 150 |
| [orchestrator-daemon-and-sessions](entry-points/orchestrator-daemon-and-sessions.md) | Daemon | 248 |
| [remote-control](entry-points/remote-control.md) | Call sites | 96 |

### patterns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-process-and-conventions](patterns/cli-process-and-conventions.md) | CLI, TUI | 239 |
| [doctrine-cross-surface](patterns/doctrine-cross-surface.md) | Pinned text | 140 |
| [hook-content-stripping](patterns/hook-content-stripping.md) | Hook text | 163 |
| [merge-and-recovery](patterns/merge-and-recovery.md) | Merge flow | 111 |
| [orchestrator-daemon-loop](patterns/orchestrator-daemon-loop.md) | Poll loop | 117 |
| [remote-control](patterns/remote-control.md) | RC detection | 51 |
| [security-sandbox-and-hooks](patterns/security-sandbox-and-hooks.md) | Sandbox | 140 |
| [stage-daemon-channels](patterns/stage-daemon-channels.md) | Channels | 191 |
| [stage-lifecycle-and-verification](patterns/stage-lifecycle-and-verification.md) | Stage states | 202 |
| [subagent-hierarchy](patterns/subagent-hierarchy.md) | Fan-out | 100 |

### conventions

| Topic | Blurb | Lines |
| --- | --- | --- |
| [code-style-and-structure](conventions/code-style-and-structure.md) | Rust style | 283 |
| [commits](conventions/commits.md) | Format | 24 |
| [config-plan-and-gate-rules](conventions/config-plan-and-gate-rules.md) | Config rules | 23 |
| [dispute-and-adjudication](conventions/dispute-and-adjudication.md) | Dispute rules | 119 |
| [git-and-build-workflow](conventions/git-and-build-workflow.md) | Git, build | 235 |
| [guidance-channels-and-plugin-scope](conventions/guidance-channels-and-plugin-scope.md) | Guidance | 139 |
| [model-and-effort-config](conventions/model-and-effort-config.md) | [models] | 123 |
| [plan-web](conventions/plan-web.md) | Plan web | 34 |
| [plan-yaml-and-hooks](conventions/plan-yaml-and-hooks.md) | Plan YAML | 182 |
| [web-dashboard-typography](conventions/web-dashboard-typography.md) | Dashboard CSS | 51 |

### mistakes

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-autonomy-deadlock](mistakes/adjudication-autonomy-deadlock.md) | Deadlock | 230 |
| [ambient-filesystem-trust](mistakes/ambient-filesystem-trust.md) | Ambient trust | 156 |
| [briefs-and-bug-reports](mistakes/briefs-and-bug-reports.md) | Briefs, bugs | 325 |
| [ci-toolchain-and-cargo](mistakes/ci-toolchain-and-cargo.md) | CI drift | 353 |
| [codex-direct-mode-invisible](mistakes/codex-direct-mode-invisible.md) | Direct mode | 23 |
| [codex-lane-rogue-wrapper](mistakes/codex-lane-rogue-wrapper.md) | Wrapper | 157 |
| [codex-navigation](mistakes/codex-navigation.md) | Codex reader | 102 |
| [codex-worker-briefing](mistakes/codex-worker-briefing.md) | Codex briefs | 130 |
| [completion-broker-credential](mistakes/completion-broker-credential.md) | Broker | 225 |
| [completion-runner-blocks](mistakes/completion-runner-blocks.md) | Blocks | 196 |
| [completion-runtime](mistakes/completion-runtime.md) | Cargo churn | 72 |
| [computed-values-and-hidden-couplings](mistakes/computed-values-and-hidden-couplings.md) | Couplings | 284 |
| [concurrency-and-locking](mistakes/concurrency-and-locking.md) | Locking races | 47 |
| [contract-autonomy-delivery](mistakes/contract-autonomy-delivery.md) | Chain bypass | 65 |
| [contract-phase-human-blocking](mistakes/contract-phase-human-blocking.md) | Waits | 11 |
| [daemon-fork-after-threads](mistakes/daemon-fork-after-threads.md) | Fork | 38 |
| [daemon-singleton](mistakes/daemon-singleton.md) | Two daemons | 141 |
| [detached-spawn-in-tests](mistakes/detached-spawn-in-tests.md) | Test orphans | 55 |
| [doctrine-and-acceptance](mistakes/doctrine-and-acceptance.md) | Doctrine | 392 |
| [hooks-shell-portability](mistakes/hooks-shell-portability.md) | Hook shells | 345 |
| [knowledge-base-drift](mistakes/knowledge-base-drift.md) | Stale docs | 259 |
| [knowledge-cli-invariants](mistakes/knowledge-cli-invariants.md) | CLI rules | 206 |
| [knowledge-write-channel](mistakes/knowledge-write-channel.md) | Writes | 100 |
| [ledger-tui-rendering](mistakes/ledger-tui-rendering.md) | Ledger TUI | 84 |
| [live-state-pollution](mistakes/live-state-pollution.md) | Pollution | 123 |
| [memory-relay-drain-gap](mistakes/memory-relay-drain-gap.md) | Relay leaks | 37 |
| [merge-cleanup-boundary](mistakes/merge-cleanup-boundary.md) | Cleanup bug | 171 |
| [merge-in-operator-checkout](mistakes/merge-in-operator-checkout.md) | Operator git | 70 |
| [merge-resolver-sandbox](mistakes/merge-resolver-sandbox.md) | Resolver | 15 |
| [parallel-worktree-shared-state](mistakes/parallel-worktree-shared-state.md) | Shared state | 341 |
| [phantom-merges](mistakes/phantom-merges.md) | Merged flag | 347 |
| [pinned-literals-ledgers-and-wiring](mistakes/pinned-literals-ledgers-and-wiring.md) | Pins | 322 |
| [plan-server-delivery](mistakes/plan-server-delivery.md) | Server | 195 |
| [pre-commit-hardening](mistakes/pre-commit-hardening.md) | Staging | 93 |
| [refactor-stragglers](mistakes/refactor-stragglers.md) | Renames | 319 |
| [release-feature-review](mistakes/release-feature-review.md) | Release gaps | 13 |
| [sandbox-and-settings](mistakes/sandbox-and-settings.md) | Settings | 391 |
| [sandbox-protected-hooks-dir](mistakes/sandbox-protected-hooks-dir.md) | Hooks dir | 37 |
| [sandbox-state-channels](mistakes/sandbox-state-channels.md) | Work state | 299 |
| [sandbox-tooling-and-network](mistakes/sandbox-tooling-and-network.md) | Tooling | 389 |
| [sandbox-write-rules-inert](mistakes/sandbox-write-rules-inert.md) | Inert rules | 57 |
| [schema-reuse-and-silent-skips](mistakes/schema-reuse-and-silent-skips.md) | Silent skips | 130 |
| [session-identity-env](mistakes/session-identity-env.md) | LOOM_* env | 126 |
| [sessions-and-liveness](mistakes/sessions-and-liveness.md) | Liveness | 382 |
| [shell-command-matchers](mistakes/shell-command-matchers.md) | Matchers | 246 |
| [source-graph-delivery](mistakes/source-graph-delivery.md) | Graph misses | 172 |
| [spurious-waiting-for-input](mistakes/spurious-waiting-for-input.md) | Idle | 35 |
| [status-broadcast-hardening](mistakes/status-broadcast-hardening.md) | Status | 75 |
| [store-without-consumer](mistakes/store-without-consumer.md) | Unread | 104 |
| [subagent-briefing](mistakes/subagent-briefing.md) | Briefs | 386 |
| [subagent-liveness-and-watch](mistakes/subagent-liveness-and-watch.md) | Watch traps | 387 |
| [subagent-orchestration](mistakes/subagent-orchestration.md) | Delegation | 377 |
| [test-concurrency-and-fixtures](mistakes/test-concurrency-and-fixtures.md) | Racy tests | 338 |
| [test-sweep-and-fixture-discipline](mistakes/test-sweep-and-fixture-discipline.md) | Fixtures | 145 |
| [testing-and-lint](mistakes/testing-and-lint.md) | Testing | 398 |
| [tests-that-cannot-fail](mistakes/tests-that-cannot-fail.md) | Vacuous | 392 |
| [tmux-backend](mistakes/tmux-backend.md) | tmux | 146 |
| [typed-config-values-process](mistakes/typed-config-values-process.md) | Dev server | 76 |
| [untrusted-value-boundaries](mistakes/untrusted-value-boundaries.md) | Untrusted | 252 |
| [verification-harness](mistakes/verification-harness.md) | Harness | 399 |
| [verification-v2-delivery](mistakes/verification-v2-delivery.md) | v2 misses | 396 |
| [visibility-and-reachability](mistakes/visibility-and-reachability.md) | Visibility | 141 |
| [web-dashboard](mistakes/web-dashboard.md) | Dashboard UI | 10 |
| [web-dashboard-server](mistakes/web-dashboard-server.md) | Server races | 278 |
| [web-terminal](mistakes/web-terminal.md) | Bridge | 135 |
| [writer-reader-address](mistakes/writer-reader-address.md) | Ignored keys | 105 |

### concerns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [agent-rule-bending-hardening](concerns/agent-rule-bending-hardening.md) | Rule bending | 379 |
| [automatic-knowledge-source-graph-followups](concerns/automatic-knowledge-source-graph-followups.md) | Followups | 77 |
| [code-quality-and-hook-debt](concerns/code-quality-and-hook-debt.md) | Code debt | 263 |
| [codex-heartbeat-starvation](concerns/codex-heartbeat-starvation.md) | Heartbeat | 49 |
| [contract-autonomy-followups](concerns/contract-autonomy-followups.md) | Contract gaps | 63 |
| [iterm2-window-teardown](concerns/iterm2-window-teardown.md) | iTerm2 | 49 |
| [knowledge-cli-gaps](concerns/knowledge-cli-gaps.md) | CLI gaps | 103 |
| [merge-and-recovery-edge-cases](concerns/merge-and-recovery-edge-cases.md) | Merge edges | 125 |
| [planning-document-model-followups](concerns/planning-document-model-followups.md) | Planning TODO | 127 |
| [platform-and-commit-gaps](concerns/platform-and-commit-gaps.md) | Platform gaps | 210 |
| [runtime-and-session-safety](concerns/runtime-and-session-safety.md) | Orphans, tmux | 156 |
| [sandbox-and-confinement-gaps](concerns/sandbox-and-confinement-gaps.md) | Confinement | 384 |
| [source-graph-known-gaps](concerns/source-graph-known-gaps.md) | Graph limits | 79 |
| [source-graph-review-backlog](concerns/source-graph-review-backlog.md) | Backlog | 327 |
| [state-confinement-gaps](concerns/state-confinement-gaps.md) | Caches | 17 |
| [token-accounting-and-proof-defects](concerns/token-accounting-and-proof-defects.md) | Defects | 349 |
| [typed-config-values](concerns/typed-config-values.md) | Config gaps | 27 |
| [verification-v2-followups](concerns/verification-v2-followups.md) | v2 followups | 169 |
| [web-dashboard-latent-issues](concerns/web-dashboard-latent-issues.md) | Latent | 102 |
| [worktree-build-cold-start](concerns/worktree-build-cold-start.md) | Cold | 159 |
