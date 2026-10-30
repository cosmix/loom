<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then only what it points to: the section for your area in a tier-1 summary (`rg -n '^## ' <file>` lists them) and the tier-2 topics your task touches. A specific question is cheaper to pull than to read — `loom knowledge context --query "..."` returns the matching sections quoted.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 250 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 76 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 186 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 250 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 192 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 115 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 248 |

## Tier 2 — Topics

### architecture

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-lifecycle](architecture/adjudication-lifecycle.md) | Verdicts | 166 |
| [codex-concurrency](architecture/codex-concurrency.md) | Limits | 110 |
| [codex-plugin](architecture/codex-plugin.md) |  | 391 |
| [completion-recovery](architecture/completion-recovery.md) | Attest | 36 |
| [config-value-types](architecture/config-value-types.md) | web API, TypeScript client, React — instead of collapsing to a string at the… | 85 |
| [context-ceiling](architecture/context-ceiling.md) |  | 111 |
| [context-retrieval](architecture/context-retrieval.md) |  | 397 |
| [context-retrieval-corpus](architecture/context-retrieval-corpus.md) | BM25 | 178 |
| [context-retrieval-routing](architecture/context-retrieval-routing.md) |  | 75 |
| [context-retrieval-state](architecture/context-retrieval-state.md) | Layers | 193 |
| [contract-collection](architecture/contract-collection.md) | Scaffold | 70 |
| [contract-phase](architecture/contract-phase.md) | Freeze | 253 |
| [core-abstractions](architecture/core-abstractions.md) | Graph | 135 |
| [daemon-launch](architecture/daemon-launch.md) |  | 65 |
| [daemon-owned-commits](architecture/daemon-owned-commits.md) |  | 151 |
| [directory-structure](architecture/directory-structure.md) | Tree | 50 |
| [execution-containment](architecture/execution-containment.md) |  | 394 |
| [hook-system](architecture/hook-system.md) |  | 311 |
| [knowledge-bootstrap](architecture/knowledge-bootstrap.md) | Digest | 83 |
| [knowledge-hierarchy](architecture/knowledge-hierarchy.md) | KB fs | 264 |
| [memory-spool](architecture/memory-spool.md) |  | 197 |
| [merge-checkout-state](architecture/merge-checkout-state.md) | FF | 28 |
| [merge-flow](architecture/merge-flow.md) |  | 138 |
| [orchestrator-loop](architecture/orchestrator-loop.md) | Ticks | 79 |
| [owned-waits](architecture/owned-waits.md) | One bound wait per parent session, keyed to an explicit worker set, replacing a… | 70 |
| [plan-lifecycle-and-fields](architecture/plan-lifecycle-and-fields.md) |  | 257 |
| [plan-server](architecture/plan-server.md) | Chat | 243 |
| [planner-web-shell](architecture/planner-web-shell.md) | Shell and screens | 45 |
| [planning-document-model](architecture/planning-document-model.md) | Docs | 180 |
| [planning-guidance](architecture/planning-guidance.md) |  | 35 |
| [planning-session-bridge](architecture/planning-session-bridge.md) |  | 230 |
| [planning-session-sandbox](architecture/planning-session-sandbox.md) |  | 135 |
| [pressure](architecture/pressure.md) | Pipeline | 42 |
| [quota-poller](architecture/quota-poller.md) |  | 37 |
| [remote-control](architecture/remote-control.md) |  | 81 |
| [run-handoff](architecture/run-handoff.md) | Run lane | 52 |
| [security-and-isolation](architecture/security-and-isolation.md) | config.worktree deny and check | 188 |
| [signal-generation](architecture/signal-generation.md) |  | 241 |
| [skill-catalog](architecture/skill-catalog.md) |  | 155 |
| [source-graph](architecture/source-graph.md) | Contract | 369 |
| [source-graph-build](architecture/source-graph-build.md) | Builder | 128 |
| [source-graph-evaluation](architecture/source-graph-evaluation.md) | Evaluator | 68 |
| [source-graph-resolution](architecture/source-graph-resolution.md) | Bindings | 123 |
| [source-graph-view](architecture/source-graph-view.md) |  | 76 |
| [status-data-model](architecture/status-data-model.md) | Fields | 239 |
| [summary-store](architecture/summary-store.md) |  | 273 |
| [target-guard](architecture/target-guard.md) | Tip | 127 |
| [terminal-backends](architecture/terminal-backends.md) |  | 280 |
| [test-runner-adapters](architecture/test-runner-adapters.md) |  | 57 |
| [token-accounting-and-receipts](architecture/token-accounting-and-receipts.md) | Ledger | 376 |
| [token-flow-sources](architecture/token-flow-sources.md) |  | 75 |
| [usage-evidence](architecture/usage-evidence.md) |  | 127 |
| [verification-v2-gates](architecture/verification-v2-gates.md) | Daemon-run host jobs for completion, checks and freezes: state, child, apply | 244 |
| [web-dashboard](architecture/web-dashboard.md) | Web | 118 |
| [web-terminal](architecture/web-terminal.md) |  | 181 |

### entry-points

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md) | CLI | 192 |
| [context-and-source-graph](entry-points/context-and-source-graph.md) |  | 80 |
| [filesystem-and-integration-modules](entry-points/filesystem-and-integration-modules.md) | Fs, git | 133 |
| [hooks](entry-points/hooks.md) |  | 150 |
| [orchestrator-daemon-and-sessions](entry-points/orchestrator-daemon-and-sessions.md) |  | 248 |
| [remote-control](entry-points/remote-control.md) | **Note:** Any references to a "container backend" or `BackendType` elsewhere in… | 96 |

### patterns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-process-and-conventions](patterns/cli-process-and-conventions.md) | CLI, TUI | 239 |
| [doctrine-cross-surface](patterns/doctrine-cross-surface.md) | Pins | 144 |
| [hook-content-stripping](patterns/hook-content-stripping.md) | Text | 163 |
| [merge-and-recovery](patterns/merge-and-recovery.md) |  | 111 |
| [orchestrator-daemon-loop](patterns/orchestrator-daemon-loop.md) | Loop | 117 |
| [remote-control](patterns/remote-control.md) | RC | 51 |
| [security-sandbox-and-hooks](patterns/security-sandbox-and-hooks.md) |  | 140 |
| [stage-daemon-channels](patterns/stage-daemon-channels.md) | Channels | 191 |
| [stage-lifecycle-and-verification](patterns/stage-lifecycle-and-verification.md) |  | 202 |
| [subagent-hierarchy](patterns/subagent-hierarchy.md) | Fan-out | 100 |

### conventions

| Topic | Blurb | Lines |
| --- | --- | --- |
| [code-style-and-structure](conventions/code-style-and-structure.md) |  | 283 |
| [commits](conventions/commits.md) | Format | 24 |
| [config-plan-and-gate-rules](conventions/config-plan-and-gate-rules.md) |  | 23 |
| [dispute-and-adjudication](conventions/dispute-and-adjudication.md) |  | 119 |
| [git-and-build-workflow](conventions/git-and-build-workflow.md) |  | 235 |
| [guidance-channels-and-plugin-scope](conventions/guidance-channels-and-plugin-scope.md) |  | 139 |
| [model-and-effort-config](conventions/model-and-effort-config.md) | [models] | 143 |
| [plan-web](conventions/plan-web.md) |  | 69 |
| [plan-yaml-and-hooks](conventions/plan-yaml-and-hooks.md) |  | 182 |
| [web-dashboard-typography](conventions/web-dashboard-typography.md) |  | 62 |

### mistakes

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-autonomy-deadlock](mistakes/adjudication-autonomy-deadlock.md) |  | 230 |
| [ambient-filesystem-trust](mistakes/ambient-filesystem-trust.md) |  | 156 |
| [briefs-and-bug-reports](mistakes/briefs-and-bug-reports.md) |  | 393 |
| [ci-toolchain-and-cargo](mistakes/ci-toolchain-and-cargo.md) | CI drift | 353 |
| [codex-direct-mode-invisible](mistakes/codex-direct-mode-invisible.md) |  | 23 |
| [codex-lane-rogue-wrapper](mistakes/codex-lane-rogue-wrapper.md) |  | 177 |
| [codex-navigation](mistakes/codex-navigation.md) | Reader | 102 |
| [codex-worker-briefing](mistakes/codex-worker-briefing.md) |  | 162 |
| [completion-broker-credential](mistakes/completion-broker-credential.md) |  | 225 |
| [completion-runner-blocks](mistakes/completion-runner-blocks.md) |  | 206 |
| [completion-runtime](mistakes/completion-runtime.md) | Churn | 72 |
| [computed-values-and-hidden-couplings](mistakes/computed-values-and-hidden-couplings.md) | Couplings | 284 |
| [concurrency-and-locking](mistakes/concurrency-and-locking.md) |  | 57 |
| [contract-autonomy-delivery](mistakes/contract-autonomy-delivery.md) | Bypass | 75 |
| [contract-phase-human-blocking](mistakes/contract-phase-human-blocking.md) |  | 11 |
| [daemon-fork-after-threads](mistakes/daemon-fork-after-threads.md) | Fork | 38 |
| [daemon-singleton](mistakes/daemon-singleton.md) |  | 141 |
| [detached-spawn-in-tests](mistakes/detached-spawn-in-tests.md) | Orphans | 55 |
| [doctrine-and-acceptance](mistakes/doctrine-and-acceptance.md) |  | 392 |
| [hooks-shell-portability](mistakes/hooks-shell-portability.md) |  | 345 |
| [knowledge-base-drift](mistakes/knowledge-base-drift.md) |  | 266 |
| [knowledge-cli-invariants](mistakes/knowledge-cli-invariants.md) | Rules | 206 |
| [knowledge-write-channel](mistakes/knowledge-write-channel.md) | Writes | 110 |
| [ledger-tui-rendering](mistakes/ledger-tui-rendering.md) |  | 84 |
| [live-state-pollution](mistakes/live-state-pollution.md) |  | 133 |
| [maintainability-baseline](mistakes/maintainability-baseline.md) |  | 10 |
| [memory-relay-drain-gap](mistakes/memory-relay-drain-gap.md) |  | 37 |
| [merge-cleanup-boundary](mistakes/merge-cleanup-boundary.md) | Cleanup | 171 |
| [merge-in-operator-checkout](mistakes/merge-in-operator-checkout.md) | Git ops | 70 |
| [merge-resolver-sandbox](mistakes/merge-resolver-sandbox.md) |  | 15 |
| [parallel-worktree-shared-state](mistakes/parallel-worktree-shared-state.md) | Merge cleanup, daemon singleton, broker, waiting-state incidents | 343 |
| [phantom-merges](mistakes/phantom-merges.md) | Plan stages split by compile order instead of merge order | 347 |
| [pinned-literals-ledgers-and-wiring](mistakes/pinned-literals-ledgers-and-wiring.md) | Pins | 332 |
| [plan-authoring](mistakes/plan-authoring.md) | Plan gaps | 10 |
| [plan-server-delivery](mistakes/plan-server-delivery.md) | Delivery slips | 371 |
| [planner-ui-repair](mistakes/planner-ui-repair.md) | Repair slips | 94 |
| [planning-sandbox-delivery](mistakes/planning-sandbox-delivery.md) |  | 73 |
| [pre-commit-hardening](mistakes/pre-commit-hardening.md) | Staging | 93 |
| [refactor-stragglers](mistakes/refactor-stragglers.md) |  | 319 |
| [release-feature-review](mistakes/release-feature-review.md) | Gaps | 13 |
| [sandbox-and-settings](mistakes/sandbox-and-settings.md) |  | 391 |
| [sandbox-protected-hooks-dir](mistakes/sandbox-protected-hooks-dir.md) |  | 37 |
| [sandbox-state-channels](mistakes/sandbox-state-channels.md) |  | 299 |
| [sandbox-tooling-and-network](mistakes/sandbox-tooling-and-network.md) |  | 389 |
| [sandbox-write-rules-inert](mistakes/sandbox-write-rules-inert.md) |  | 57 |
| [schema-reuse-and-silent-skips](mistakes/schema-reuse-and-silent-skips.md) |  | 130 |
| [session-identity-env](mistakes/session-identity-env.md) | Env | 126 |
| [sessions-and-liveness](mistakes/sessions-and-liveness.md) |  | 382 |
| [shell-command-matchers](mistakes/shell-command-matchers.md) |  | 246 |
| [source-graph-delivery](mistakes/source-graph-delivery.md) | Misses | 172 |
| [spurious-waiting-for-input](mistakes/spurious-waiting-for-input.md) | Idle | 43 |
| [status-broadcast-hardening](mistakes/status-broadcast-hardening.md) |  | 75 |
| [store-without-consumer](mistakes/store-without-consumer.md) | Unread | 104 |
| [subagent-briefing](mistakes/subagent-briefing.md) |  | 386 |
| [subagent-liveness-and-watch](mistakes/subagent-liveness-and-watch.md) |  | 387 |
| [subagent-orchestration](mistakes/subagent-orchestration.md) | Delegate | 391 |
| [test-concurrency-and-fixtures](mistakes/test-concurrency-and-fixtures.md) | Races | 360 |
| [test-sweep-and-fixture-discipline](mistakes/test-sweep-and-fixture-discipline.md) |  | 145 |
| [testing-and-lint](mistakes/testing-and-lint.md) |  | 398 |
| [tests-that-cannot-fail](mistakes/tests-that-cannot-fail.md) | Vacuous | 392 |
| [tmux-backend](mistakes/tmux-backend.md) | tmux | 164 |
| [typed-config-values-process](mistakes/typed-config-values-process.md) | Dev | 76 |
| [untrusted-value-boundaries](mistakes/untrusted-value-boundaries.md) | Trust | 254 |
| [verification-harness](mistakes/verification-harness.md) |  | 399 |
| [verification-v2-delivery](mistakes/verification-v2-delivery.md) | v2 | 396 |
| [visibility-and-reachability](mistakes/visibility-and-reachability.md) |  | 141 |
| [web-dashboard](mistakes/web-dashboard.md) | UI | 17 |
| [web-dashboard-server](mistakes/web-dashboard-server.md) | Races | 278 |
| [web-terminal](mistakes/web-terminal.md) | Mistakes in the PTY/tmux terminal bridge under… | 135 |
| [writer-reader-address](mistakes/writer-reader-address.md) | Keys | 105 |

### concerns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [agent-rule-bending-hardening](concerns/agent-rule-bending-hardening.md) |  | 379 |
| [automatic-knowledge-source-graph-followups](concerns/automatic-knowledge-source-graph-followups.md) | Gaps | 79 |
| [code-quality-and-hook-debt](concerns/code-quality-and-hook-debt.md) | recovery sync resets a finished node; macOS forward-guard branches unrun | 265 |
| [codex-heartbeat-starvation](concerns/codex-heartbeat-starvation.md) |  | 49 |
| [contract-autonomy-followups](concerns/contract-autonomy-followups.md) |  | 63 |
| [handoff-followups](concerns/handoff-followups.md) |  | 57 |
| [iterm2-window-teardown](concerns/iterm2-window-teardown.md) |  | 49 |
| [knowledge-cli-gaps](concerns/knowledge-cli-gaps.md) |  | 111 |
| [merge-and-recovery-edge-cases](concerns/merge-and-recovery-edge-cases.md) | Edges | 117 |
| [plan-chat-followups](concerns/plan-chat-followups.md) | Chat | 40 |
| [planner-ui-repair-followups](concerns/planner-ui-repair-followups.md) | Repair | 67 |
| [planning-document-model-followups](concerns/planning-document-model-followups.md) |  | 143 |
| [planning-guidance-followups](concerns/planning-guidance-followups.md) |  | 42 |
| [planning-session-sandbox-limits](concerns/planning-session-sandbox-limits.md) |  | 185 |
| [platform-and-commit-gaps](concerns/platform-and-commit-gaps.md) |  | 212 |
| [runtime-and-session-safety](concerns/runtime-and-session-safety.md) |  | 156 |
| [sandbox-and-confinement-gaps](concerns/sandbox-and-confinement-gaps.md) |  | 384 |
| [source-graph-known-gaps](concerns/source-graph-known-gaps.md) | Limits | 85 |
| [source-graph-review-backlog](concerns/source-graph-review-backlog.md) | Host-verification review suggestions: open and fixed defects by file:line | 329 |
| [state-confinement-gaps](concerns/state-confinement-gaps.md) | Caches | 17 |
| [token-accounting-and-proof-defects](concerns/token-accounting-and-proof-defects.md) |  | 349 |
| [typed-config-values](concerns/typed-config-values.md) | Gaps | 27 |
| [verification-v2-followups](concerns/verification-v2-followups.md) |  | 173 |
| [web-dashboard-latent-issues](concerns/web-dashboard-latent-issues.md) | oversights. See… | 102 |
| [worktree-build-cold-start](concerns/worktree-build-cold-start.md) | Cold | 159 |
