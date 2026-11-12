<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then only what it points to: the section for your area in a tier-1 summary (`rg -n '^## ' <file>` lists them) and the tier-2 topics your task touches. A specific question is cheaper to pull than to read — `loom knowledge context --query "..."` returns the matching sections quoted.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 250 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 85 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 186 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 250 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 250 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 116 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 250 |

## Tier 2 — Topics

### architecture

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-lifecycle](architecture/adjudication-lifecycle.md) |  | 166 |
| [codex-concurrency](architecture/codex-concurrency.md) | Limits | 110 |
| [codex-plugin](architecture/codex-plugin.md) |  | 378 |
| [completion-recovery](architecture/completion-recovery.md) | Proof | 38 |
| [config-value-types](architecture/config-value-types.md) |  | 85 |
| [context-ceiling](architecture/context-ceiling.md) |  | 111 |
| [context-retrieval](architecture/context-retrieval.md) |  | 396 |
| [context-retrieval-corpus](architecture/context-retrieval-corpus.md) | BM25 | 178 |
| [context-retrieval-routing](architecture/context-retrieval-routing.md) |  | 75 |
| [context-retrieval-state](architecture/context-retrieval-state.md) | Layers | 179 |
| [contract-collection](architecture/contract-collection.md) |  | 70 |
| [contract-phase](architecture/contract-phase.md) | Freeze | 275 |
| [core-abstractions](architecture/core-abstractions.md) | Graph | 135 |
| [daemon-launch](architecture/daemon-launch.md) |  | 71 |
| [daemon-owned-commits](architecture/daemon-owned-commits.md) |  | 151 |
| [directory-structure](architecture/directory-structure.md) | Tree | 50 |
| [execution-containment](architecture/execution-containment.md) |  | 392 |
| [hook-system](architecture/hook-system.md) |  | 329 |
| [knowledge-bootstrap](architecture/knowledge-bootstrap.md) | KB | 83 |
| [knowledge-hierarchy](architecture/knowledge-hierarchy.md) | KB fs | 264 |
| [live-preview-editor](architecture/live-preview-editor.md) | Editor | 32 |
| [loom-ask](architecture/loom-ask.md) |  | 75 |
| [memory-spool](architecture/memory-spool.md) |  | 205 |
| [merge-checkout-state](architecture/merge-checkout-state.md) | FF | 32 |
| [merge-flow](architecture/merge-flow.md) |  | 133 |
| [orchestrator-loop](architecture/orchestrator-loop.md) | Ticks | 80 |
| [owned-waits](architecture/owned-waits.md) | Waits | 70 |
| [plan-lifecycle-and-fields](architecture/plan-lifecycle-and-fields.md) |  | 284 |
| [plan-server](architecture/plan-server.md) | Chat | 294 |
| [planner-feature-screens](architecture/planner-feature-screens.md) | Spine | 48 |
| [planner-uploads-and-drafts](architecture/planner-uploads-and-drafts.md) |  | 35 |
| [planner-web-shell](architecture/planner-web-shell.md) | Shell | 143 |
| [planning-document-model](architecture/planning-document-model.md) | Docs | 183 |
| [planning-guidance](architecture/planning-guidance.md) |  | 43 |
| [planning-session-bridge](architecture/planning-session-bridge.md) |  | 351 |
| [planning-session-sandbox](architecture/planning-session-sandbox.md) |  | 148 |
| [planning-trace](architecture/planning-trace.md) |  | 132 |
| [pressure](architecture/pressure.md) |  | 42 |
| [quota-poller](architecture/quota-poller.md) |  | 37 |
| [read-gate](architecture/read-gate.md) |  | 153 |
| [remote-control](architecture/remote-control.md) |  | 81 |
| [run-handoff](architecture/run-handoff.md) |  | 52 |
| [security-and-isolation](architecture/security-and-isolation.md) | Deny | 196 |
| [signal-generation](architecture/signal-generation.md) |  | 247 |
| [skill-catalog](architecture/skill-catalog.md) |  | 158 |
| [source-graph](architecture/source-graph.md) |  | 372 |
| [source-graph-build](architecture/source-graph-build.md) |  | 129 |
| [source-graph-evaluation](architecture/source-graph-evaluation.md) |  | 68 |
| [source-graph-resolution](architecture/source-graph-resolution.md) |  | 125 |
| [source-graph-view](architecture/source-graph-view.md) |  | 78 |
| [status-data-model](architecture/status-data-model.md) | Fields | 267 |
| [summary-store](architecture/summary-store.md) |  | 342 |
| [target-guard](architecture/target-guard.md) | Tip | 127 |
| [terminal-backends](architecture/terminal-backends.md) |  | 285 |
| [test-runner-adapters](architecture/test-runner-adapters.md) |  | 56 |
| [token-accounting-and-receipts](architecture/token-accounting-and-receipts.md) | Ledger | 358 |
| [token-flow-sources](architecture/token-flow-sources.md) |  | 84 |
| [usage-evidence](architecture/usage-evidence.md) |  | 127 |
| [verification-v2-gates](architecture/verification-v2-gates.md) | Gates | 257 |
| [web-dashboard](architecture/web-dashboard.md) | Web | 133 |
| [web-terminal](architecture/web-terminal.md) |  | 181 |

### entry-points

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md) | CLI | 197 |
| [context-and-source-graph](entry-points/context-and-source-graph.md) |  | 80 |
| [filesystem-and-integration-modules](entry-points/filesystem-and-integration-modules.md) |  | 135 |
| [hooks](entry-points/hooks.md) |  | 145 |
| [orchestrator-daemon-and-sessions](entry-points/orchestrator-daemon-and-sessions.md) |  | 250 |
| [remote-control](entry-points/remote-control.md) | RC | 96 |

### patterns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-process-and-conventions](patterns/cli-process-and-conventions.md) |  | 239 |
| [doctrine-cross-surface](patterns/doctrine-cross-surface.md) | Pins | 144 |
| [hook-content-stripping](patterns/hook-content-stripping.md) | Text | 162 |
| [merge-and-recovery](patterns/merge-and-recovery.md) |  | 111 |
| [orchestrator-daemon-loop](patterns/orchestrator-daemon-loop.md) | Loop | 115 |
| [remote-control](patterns/remote-control.md) | RC | 51 |
| [security-sandbox-and-hooks](patterns/security-sandbox-and-hooks.md) |  | 144 |
| [stage-daemon-channels](patterns/stage-daemon-channels.md) |  | 184 |
| [stage-lifecycle-and-verification](patterns/stage-lifecycle-and-verification.md) |  | 214 |
| [subagent-hierarchy](patterns/subagent-hierarchy.md) |  | 105 |

### conventions

| Topic | Blurb | Lines |
| --- | --- | --- |
| [code-style-and-structure](conventions/code-style-and-structure.md) |  | 287 |
| [commits](conventions/commits.md) | Format | 24 |
| [config-plan-and-gate-rules](conventions/config-plan-and-gate-rules.md) |  | 27 |
| [dispute-and-adjudication](conventions/dispute-and-adjudication.md) |  | 123 |
| [git-and-build-workflow](conventions/git-and-build-workflow.md) |  | 237 |
| [guidance-channels-and-plugin-scope](conventions/guidance-channels-and-plugin-scope.md) |  | 134 |
| [model-and-effort-config](conventions/model-and-effort-config.md) |  | 143 |
| [plan-web](conventions/plan-web.md) |  | 204 |
| [plan-yaml-and-hooks](conventions/plan-yaml-and-hooks.md) |  | 182 |
| [web-dashboard-typography](conventions/web-dashboard-typography.md) |  | 64 |

### mistakes

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-autonomy-deadlock](mistakes/adjudication-autonomy-deadlock.md) |  | 228 |
| [ambient-filesystem-trust](mistakes/ambient-filesystem-trust.md) |  | 176 |
| [briefs-and-bug-reports](mistakes/briefs-and-bug-reports.md) |  | 318 |
| [ci-toolchain-and-cargo](mistakes/ci-toolchain-and-cargo.md) |  | 383 |
| [codex-direct-mode-invisible](mistakes/codex-direct-mode-invisible.md) |  | 23 |
| [codex-lane-rogue-wrapper](mistakes/codex-lane-rogue-wrapper.md) |  | 177 |
| [codex-navigation](mistakes/codex-navigation.md) | Reader | 202 |
| [codex-worker-briefing](mistakes/codex-worker-briefing.md) |  | 162 |
| [completion-broker-credential](mistakes/completion-broker-credential.md) |  | 225 |
| [completion-runner-blocks](mistakes/completion-runner-blocks.md) |  | 216 |
| [completion-runtime](mistakes/completion-runtime.md) | Churn | 94 |
| [computed-values-and-hidden-couplings](mistakes/computed-values-and-hidden-couplings.md) | Values | 284 |
| [concurrency-and-locking](mistakes/concurrency-and-locking.md) |  | 207 |
| [contract-autonomy-delivery](mistakes/contract-autonomy-delivery.md) | Bypass | 109 |
| [daemon-fork-after-threads](mistakes/daemon-fork-after-threads.md) | Fork | 38 |
| [daemon-singleton](mistakes/daemon-singleton.md) |  | 141 |
| [detached-spawn-in-tests](mistakes/detached-spawn-in-tests.md) |  | 55 |
| [dev-install](mistakes/dev-install.md) | Daemon | 44 |
| [doctrine-and-acceptance](mistakes/doctrine-and-acceptance.md) |  | 392 |
| [frontmatter-boundary](mistakes/frontmatter-boundary.md) |  | 13 |
| [hooks-shell-portability](mistakes/hooks-shell-portability.md) |  | 372 |
| [host-verify-group-containment](mistakes/host-verify-group-containment.md) |  | 23 |
| [knowledge-base-drift](mistakes/knowledge-base-drift.md) |  | 266 |
| [knowledge-cli-invariants](mistakes/knowledge-cli-invariants.md) | Rules | 231 |
| [knowledge-write-channel](mistakes/knowledge-write-channel.md) | Writes | 134 |
| [ledger-tui-rendering](mistakes/ledger-tui-rendering.md) |  | 84 |
| [live-state-pollution](mistakes/live-state-pollution.md) |  | 133 |
| [maintainability-baseline](mistakes/maintainability-baseline.md) |  | 96 |
| [memory-relay-drain-gap](mistakes/memory-relay-drain-gap.md) |  | 39 |
| [merge-cleanup-boundary](mistakes/merge-cleanup-boundary.md) |  | 205 |
| [merge-in-operator-checkout](mistakes/merge-in-operator-checkout.md) |  | 70 |
| [merge-resolver-sandbox](mistakes/merge-resolver-sandbox.md) |  | 35 |
| [parallel-worktree-shared-state](mistakes/parallel-worktree-shared-state.md) | Shared | 351 |
| [phantom-merges](mistakes/phantom-merges.md) | Merges | 347 |
| [pinned-literals-ledgers-and-wiring](mistakes/pinned-literals-ledgers-and-wiring.md) | Pins | 332 |
| [plan-authoring](mistakes/plan-authoring.md) |  | 139 |
| [plan-server-delivery](mistakes/plan-server-delivery.md) | Slips | 369 |
| [planner-design-brainstorm](mistakes/planner-design-brainstorm.md) | Stubs | 94 |
| [planner-follow-ups-delivery](mistakes/planner-follow-ups-delivery.md) | Slips | 196 |
| [planner-polish](mistakes/planner-polish.md) | Polish | 127 |
| [planner-redesign](mistakes/planner-redesign.md) |  | 80 |
| [planner-review-fixes](mistakes/planner-review-fixes.md) |  | 139 |
| [planner-ui-repair](mistakes/planner-ui-repair.md) | Repair | 215 |
| [planner-visual-feedback-shortfall](mistakes/planner-visual-feedback-shortfall.md) | Visual | 17 |
| [planning-sandbox-delivery](mistakes/planning-sandbox-delivery.md) |  | 73 |
| [pre-commit-hardening](mistakes/pre-commit-hardening.md) |  | 93 |
| [refactor-stragglers](mistakes/refactor-stragglers.md) |  | 335 |
| [release-feature-review](mistakes/release-feature-review.md) | Gaps | 13 |
| [review-continuation-verification](mistakes/review-continuation-verification.md) | Repair | 331 |
| [review-final-wave](mistakes/review-final-wave.md) |  | 190 |
| [review-integration-verification](mistakes/review-integration-verification.md) |  | 182 |
| [review-tracker](mistakes/review-tracker.md) |  | 353 |
| [sandbox-and-settings](mistakes/sandbox-and-settings.md) |  | 391 |
| [sandbox-protected-hooks-dir](mistakes/sandbox-protected-hooks-dir.md) |  | 37 |
| [sandbox-state-channels](mistakes/sandbox-state-channels.md) |  | 299 |
| [sandbox-tooling-and-network](mistakes/sandbox-tooling-and-network.md) |  | 387 |
| [sandbox-write-rules-inert](mistakes/sandbox-write-rules-inert.md) |  | 67 |
| [schema-reuse-and-silent-skips](mistakes/schema-reuse-and-silent-skips.md) |  | 130 |
| [session-identity-env](mistakes/session-identity-env.md) | Env | 126 |
| [sessions-and-liveness](mistakes/sessions-and-liveness.md) |  | 392 |
| [shell-command-matchers](mistakes/shell-command-matchers.md) |  | 295 |
| [source-graph-delivery](mistakes/source-graph-delivery.md) | Misses | 172 |
| [spurious-waiting-for-input](mistakes/spurious-waiting-for-input.md) | Idle | 53 |
| [status-broadcast-hardening](mistakes/status-broadcast-hardening.md) |  | 75 |
| [store-without-consumer](mistakes/store-without-consumer.md) | Unread | 104 |
| [subagent-briefing](mistakes/subagent-briefing.md) |  | 394 |
| [subagent-liveness-and-watch](mistakes/subagent-liveness-and-watch.md) |  | 387 |
| [subagent-orchestration](mistakes/subagent-orchestration.md) | Agents | 397 |
| [test-concurrency-and-fixtures](mistakes/test-concurrency-and-fixtures.md) | Races | 398 |
| [test-sweep-and-fixture-discipline](mistakes/test-sweep-and-fixture-discipline.md) |  | 388 |
| [testing-and-lint](mistakes/testing-and-lint.md) |  | 398 |
| [tests-that-cannot-fail](mistakes/tests-that-cannot-fail.md) |  | 392 |
| [tmux-backend](mistakes/tmux-backend.md) | tmux | 164 |
| [typed-config-values-process](mistakes/typed-config-values-process.md) | Dev | 86 |
| [untrusted-value-boundaries](mistakes/untrusted-value-boundaries.md) | Trust | 252 |
| [verification-harness](mistakes/verification-harness.md) |  | 399 |
| [verification-v2-delivery](mistakes/verification-v2-delivery.md) | v2 | 396 |
| [visibility-and-reachability](mistakes/visibility-and-reachability.md) |  | 141 |
| [web-dashboard](mistakes/web-dashboard.md) | UI | 184 |
| [web-dashboard-server](mistakes/web-dashboard-server.md) | Races | 278 |
| [web-terminal](mistakes/web-terminal.md) | TTY | 142 |
| [writer-reader-address](mistakes/writer-reader-address.md) | Keys | 115 |

### concerns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [agent-rule-bending-hardening](concerns/agent-rule-bending-hardening.md) |  | 375 |
| [automatic-knowledge-source-graph-followups](concerns/automatic-knowledge-source-graph-followups.md) | Gaps | 69 |
| [code-quality-and-hook-debt](concerns/code-quality-and-hook-debt.md) | Debt | 248 |
| [codex-heartbeat-starvation](concerns/codex-heartbeat-starvation.md) |  | 22 |
| [contract-autonomy-followups](concerns/contract-autonomy-followups.md) |  | 74 |
| [handoff-followups](concerns/handoff-followups.md) |  | 57 |
| [iterm2-window-teardown](concerns/iterm2-window-teardown.md) |  | 15 |
| [knowledge-cli-gaps](concerns/knowledge-cli-gaps.md) |  | 108 |
| [loom-ask-and-read-gate](concerns/loom-ask-and-read-gate.md) |  | 112 |
| [merge-and-recovery-edge-cases](concerns/merge-and-recovery-edge-cases.md) | Edges | 105 |
| [plan-chat-followups](concerns/plan-chat-followups.md) | Chat | 41 |
| [planner-follow-ups-followups](concerns/planner-follow-ups-followups.md) |  | 92 |
| [planner-ui-repair-followups](concerns/planner-ui-repair-followups.md) | UI | 214 |
| [planning-document-model-followups](concerns/planning-document-model-followups.md) |  | 172 |
| [planning-guidance-followups](concerns/planning-guidance-followups.md) |  | 43 |
| [planning-session-sandbox-limits](concerns/planning-session-sandbox-limits.md) |  | 238 |
| [platform-and-commit-gaps](concerns/platform-and-commit-gaps.md) |  | 234 |
| [runtime-and-session-safety](concerns/runtime-and-session-safety.md) |  | 137 |
| [sandbox-and-confinement-gaps](concerns/sandbox-and-confinement-gaps.md) |  | 363 |
| [session-tmpdir-deny](concerns/session-tmpdir-deny.md) |  | 7 |
| [source-graph-known-gaps](concerns/source-graph-known-gaps.md) | Limits | 90 |
| [source-graph-review-backlog](concerns/source-graph-review-backlog.md) | Todo | 298 |
| [state-confinement-gaps](concerns/state-confinement-gaps.md) | Caches | 21 |
| [token-accounting-and-proof-defects](concerns/token-accounting-and-proof-defects.md) |  | 334 |
| [typed-config-values](concerns/typed-config-values.md) | Gaps | 22 |
| [verification-v2-followups](concerns/verification-v2-followups.md) |  | 230 |
| [web-dashboard-latent-issues](concerns/web-dashboard-latent-issues.md) | UI | 96 |
| [worktree-build-cold-start](concerns/worktree-build-cold-start.md) | Cold | 136 |
