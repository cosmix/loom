<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then only what it points to: the section for your area in a tier-1 summary (`rg -n '^## ' <file>` lists them) and the tier-2 topics your task touches. A specific question is cheaper to pull than to read — `loom knowledge context --query "..."` returns the matching sections quoted.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 250 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 86 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 200 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 250 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 262 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 116 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 248 |

## Tier 2 — Topics

### architecture

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-lifecycle](architecture/adjudication-lifecycle.md) |  | 167 |
| [codex-concurrency](architecture/codex-concurrency.md) | Limits | 112 |
| [codex-plugin](architecture/codex-plugin.md) |  | 353 |
| [completion-recovery](architecture/completion-recovery.md) | Proof | 38 |
| [config-value-types](architecture/config-value-types.md) |  | 85 |
| [context-ceiling](architecture/context-ceiling.md) |  | 111 |
| [context-retrieval](architecture/context-retrieval.md) |  | 397 |
| [context-retrieval-corpus](architecture/context-retrieval-corpus.md) | BM25 | 178 |
| [context-retrieval-routing](architecture/context-retrieval-routing.md) |  | 75 |
| [context-retrieval-state](architecture/context-retrieval-state.md) | Layers | 179 |
| [contract-collection](architecture/contract-collection.md) |  | 70 |
| [contract-phase](architecture/contract-phase.md) | Freeze | 282 |
| [contract-trial](architecture/contract-trial.md) | loom check --contracts, replacement trial receipt, overlay undo, env-red refusal | 22 |
| [core-abstractions](architecture/core-abstractions.md) | Graph | 135 |
| [criterion-narrowing-evidence](architecture/criterion-narrowing-evidence.md) | base-run evidence, leaves_out narrowing rules, hand-offs | 36 |
| [daemon-launch](architecture/daemon-launch.md) |  | 71 |
| [daemon-owned-commits](architecture/daemon-owned-commits.md) |  | 151 |
| [directory-structure](architecture/directory-structure.md) | Tree | 50 |
| [execution-containment](architecture/execution-containment.md) |  | 398 |
| [hook-system](architecture/hook-system.md) |  | 339 |
| [knowledge-bootstrap](architecture/knowledge-bootstrap.md) | KB | 83 |
| [knowledge-hierarchy](architecture/knowledge-hierarchy.md) | KB fs | 264 |
| [live-preview-editor](architecture/live-preview-editor.md) | Editor | 32 |
| [loom-ask](architecture/loom-ask.md) |  | 75 |
| [memory-spool](architecture/memory-spool.md) |  | 211 |
| [merge-checkout-state](architecture/merge-checkout-state.md) | FF | 32 |
| [merge-flow](architecture/merge-flow.md) |  | 133 |
| [native-codex-workers](architecture/native-codex-workers.md) |  | 74 |
| [orchestrator-loop](architecture/orchestrator-loop.md) | Ticks | 80 |
| [owned-waits](architecture/owned-waits.md) | Waits | 72 |
| [plan-lifecycle-and-fields](architecture/plan-lifecycle-and-fields.md) |  | 284 |
| [plan-server](architecture/plan-server.md) | Chat | 293 |
| [planner-feature-screens](architecture/planner-feature-screens.md) | Spine | 48 |
| [planner-uploads-and-drafts](architecture/planner-uploads-and-drafts.md) |  | 37 |
| [planner-web-shell](architecture/planner-web-shell.md) | Shell | 147 |
| [planning-document-model](architecture/planning-document-model.md) | Docs | 183 |
| [planning-guidance](architecture/planning-guidance.md) |  | 43 |
| [planning-session-bridge](architecture/planning-session-bridge.md) |  | 351 |
| [planning-session-sandbox](architecture/planning-session-sandbox.md) |  | 148 |
| [planning-trace](architecture/planning-trace.md) |  | 145 |
| [pressure](architecture/pressure.md) |  | 42 |
| [quota-poller](architecture/quota-poller.md) |  | 41 |
| [read-gate](architecture/read-gate.md) |  | 153 |
| [remote-control](architecture/remote-control.md) |  | 81 |
| [run-handoff](architecture/run-handoff.md) |  | 52 |
| [security-and-isolation](architecture/security-and-isolation.md) | Deny | 202 |
| [signal-generation](architecture/signal-generation.md) |  | 261 |
| [skill-catalog](architecture/skill-catalog.md) |  | 158 |
| [source-graph](architecture/source-graph.md) |  | 372 |
| [source-graph-build](architecture/source-graph-build.md) |  | 130 |
| [source-graph-evaluation](architecture/source-graph-evaluation.md) |  | 68 |
| [source-graph-resolution](architecture/source-graph-resolution.md) |  | 125 |
| [source-graph-view](architecture/source-graph-view.md) |  | 78 |
| [stage-build-cache](architecture/stage-build-cache.md) | Per-stage cache root: env, seeds, sockets, read roots, xcrun bypass | 50 |
| [status-data-model](architecture/status-data-model.md) | Fields | 268 |
| [summary-store](architecture/summary-store.md) |  | 352 |
| [target-guard](architecture/target-guard.md) | Tip | 127 |
| [terminal-backends](architecture/terminal-backends.md) |  | 285 |
| [test-runner-adapters](architecture/test-runner-adapters.md) |  | 56 |
| [token-accounting-and-receipts](architecture/token-accounting-and-receipts.md) | Ledger | 355 |
| [token-flow-sources](architecture/token-flow-sources.md) |  | 84 |
| [usage-evidence](architecture/usage-evidence.md) |  | 136 |
| [verification-v2-gates](architecture/verification-v2-gates.md) | Gates | 261 |
| [web-dashboard](architecture/web-dashboard.md) | Web | 133 |
| [web-terminal](architecture/web-terminal.md) |  | 181 |

### entry-points

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md) | CLI | 197 |
| [context-and-source-graph](entry-points/context-and-source-graph.md) |  | 80 |
| [filesystem-and-integration-modules](entry-points/filesystem-and-integration-modules.md) |  | 135 |
| [hooks](entry-points/hooks.md) |  | 152 |
| [orchestrator-daemon-and-sessions](entry-points/orchestrator-daemon-and-sessions.md) |  | 262 |
| [remote-control](entry-points/remote-control.md) | RC | 96 |

### patterns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [canonical-helpers](patterns/canonical-helpers.md) |  | 72 |
| [cli-process-and-conventions](patterns/cli-process-and-conventions.md) |  | 239 |
| [doctrine-cross-surface](patterns/doctrine-cross-surface.md) | Pins | 144 |
| [hook-content-stripping](patterns/hook-content-stripping.md) | Text | 162 |
| [merge-and-recovery](patterns/merge-and-recovery.md) |  | 111 |
| [orchestrator-daemon-loop](patterns/orchestrator-daemon-loop.md) | Loop | 115 |
| [remote-control](patterns/remote-control.md) | RC | 51 |
| [security-sandbox-and-hooks](patterns/security-sandbox-and-hooks.md) |  | 144 |
| [stage-daemon-channels](patterns/stage-daemon-channels.md) |  | 211 |
| [stage-lifecycle-and-verification](patterns/stage-lifecycle-and-verification.md) |  | 214 |
| [subagent-hierarchy](patterns/subagent-hierarchy.md) |  | 143 |

### conventions

| Topic | Blurb | Lines |
| --- | --- | --- |
| [code-style-and-structure](conventions/code-style-and-structure.md) |  | 298 |
| [commits](conventions/commits.md) | Format | 24 |
| [config-plan-and-gate-rules](conventions/config-plan-and-gate-rules.md) |  | 39 |
| [dispute-and-adjudication](conventions/dispute-and-adjudication.md) |  | 155 |
| [git-and-build-workflow](conventions/git-and-build-workflow.md) |  | 237 |
| [guidance-channels-and-plugin-scope](conventions/guidance-channels-and-plugin-scope.md) |  | 134 |
| [host-job-test-guards](conventions/host-job-test-guards.md) | macOS host-job denials, sandbox_probe guards, strict-CI skips | 34 |
| [model-and-effort-config](conventions/model-and-effort-config.md) |  | 147 |
| [plan-web](conventions/plan-web.md) |  | 215 |
| [plan-yaml-and-hooks](conventions/plan-yaml-and-hooks.md) |  | 186 |
| [web-dashboard-typography](conventions/web-dashboard-typography.md) |  | 68 |

### mistakes

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-autonomy-deadlock](mistakes/adjudication-autonomy-deadlock.md) |  | 238 |
| [ambient-filesystem-trust](mistakes/ambient-filesystem-trust.md) |  | 186 |
| [briefs-and-bug-reports](mistakes/briefs-and-bug-reports.md) |  | 386 |
| [cargo-under-confinement](mistakes/cargo-under-confinement.md) | Cargo in sandboxes and confined runs: audit fetches, cold builds, Seatbelt | 83 |
| [ci-toolchain-and-cargo](mistakes/ci-toolchain-and-cargo.md) |  | 380 |
| [codex-direct-mode-invisible](mistakes/codex-direct-mode-invisible.md) |  | 23 |
| [codex-lane-rogue-wrapper](mistakes/codex-lane-rogue-wrapper.md) |  | 187 |
| [codex-navigation](mistakes/codex-navigation.md) | Reader | 202 |
| [codex-worker-briefing](mistakes/codex-worker-briefing.md) |  | 258 |
| [completion-broker-credential](mistakes/completion-broker-credential.md) |  | 225 |
| [completion-runner-blocks](mistakes/completion-runner-blocks.md) |  | 221 |
| [completion-runtime](mistakes/completion-runtime.md) | Churn | 124 |
| [computed-values-and-hidden-couplings](mistakes/computed-values-and-hidden-couplings.md) | Values | 284 |
| [concurrency-and-locking](mistakes/concurrency-and-locking.md) |  | 207 |
| [consolidation-drift](mistakes/consolidation-drift.md) |  | 139 |
| [contract-autonomy-delivery](mistakes/contract-autonomy-delivery.md) | Bypass | 109 |
| [daemon-fork-after-threads](mistakes/daemon-fork-after-threads.md) | Fork | 38 |
| [daemon-singleton](mistakes/daemon-singleton.md) |  | 141 |
| [detached-spawn-in-tests](mistakes/detached-spawn-in-tests.md) |  | 65 |
| [dev-install](mistakes/dev-install.md) | Daemon | 58 |
| [dispute-guards-delivery](mistakes/dispute-guards-delivery.md) | host-job hangs, parallel worker collisions, scaffold wording, hook quirks | 121 |
| [doctrine-and-acceptance](mistakes/doctrine-and-acceptance.md) |  | 392 |
| [hooks-shell-portability](mistakes/hooks-shell-portability.md) |  | 402 |
| [host-verify-group-containment](mistakes/host-verify-group-containment.md) |  | 47 |
| [knowledge-base-drift](mistakes/knowledge-base-drift.md) |  | 276 |
| [knowledge-cli-invariants](mistakes/knowledge-cli-invariants.md) | Rules | 241 |
| [knowledge-write-channel](mistakes/knowledge-write-channel.md) | Writes | 144 |
| [ledger-tui-rendering](mistakes/ledger-tui-rendering.md) |  | 84 |
| [live-state-pollution](mistakes/live-state-pollution.md) |  | 133 |
| [maintainability-baseline](mistakes/maintainability-baseline.md) |  | 103 |
| [memory-relay-drain-gap](mistakes/memory-relay-drain-gap.md) |  | 51 |
| [merge-cleanup-boundary](mistakes/merge-cleanup-boundary.md) |  | 237 |
| [merge-in-operator-checkout](mistakes/merge-in-operator-checkout.md) |  | 70 |
| [parallel-worktree-shared-state](mistakes/parallel-worktree-shared-state.md) | Shared | 351 |
| [phantom-merges](mistakes/phantom-merges.md) | Merges | 347 |
| [pinned-literals-ledgers-and-wiring](mistakes/pinned-literals-ledgers-and-wiring.md) | Pins | 332 |
| [plan-authoring](mistakes/plan-authoring.md) |  | 298 |
| [plan-server-delivery](mistakes/plan-server-delivery.md) | Slips | 376 |
| [planner-design-brainstorm](mistakes/planner-design-brainstorm.md) | Stubs | 134 |
| [planner-follow-ups-delivery](mistakes/planner-follow-ups-delivery.md) | Slips | 196 |
| [planner-polish](mistakes/planner-polish.md) | Polish | 127 |
| [planner-redesign](mistakes/planner-redesign.md) |  | 80 |
| [planner-review-fixes](mistakes/planner-review-fixes.md) |  | 149 |
| [planner-ui-repair](mistakes/planner-ui-repair.md) | Repair | 215 |
| [planner-visual-feedback-shortfall](mistakes/planner-visual-feedback-shortfall.md) | Visual | 24 |
| [planning-sandbox-delivery](mistakes/planning-sandbox-delivery.md) |  | 90 |
| [pre-commit-hardening](mistakes/pre-commit-hardening.md) |  | 93 |
| [refactor-stragglers](mistakes/refactor-stragglers.md) |  | 335 |
| [review-continuation-verification](mistakes/review-continuation-verification.md) | Repair | 331 |
| [review-final-wave](mistakes/review-final-wave.md) |  | 200 |
| [review-integration-verification](mistakes/review-integration-verification.md) |  | 283 |
| [review-tracker](mistakes/review-tracker.md) |  | 353 |
| [sandbox-and-settings](mistakes/sandbox-and-settings.md) |  | 391 |
| [sandbox-hook-preflight](mistakes/sandbox-hook-preflight.md) | Check 5 hook preflight: why extending its program denylist fails | 13 |
| [sandbox-protected-hooks-dir](mistakes/sandbox-protected-hooks-dir.md) |  | 37 |
| [sandbox-state-channels](mistakes/sandbox-state-channels.md) |  | 299 |
| [sandbox-tooling-and-network](mistakes/sandbox-tooling-and-network.md) |  | 397 |
| [sandbox-write-rules-inert](mistakes/sandbox-write-rules-inert.md) |  | 67 |
| [schema-reuse-and-silent-skips](mistakes/schema-reuse-and-silent-skips.md) |  | 130 |
| [session-identity-env](mistakes/session-identity-env.md) | Env | 126 |
| [sessions-and-liveness](mistakes/sessions-and-liveness.md) |  | 399 |
| [shell-command-matchers](mistakes/shell-command-matchers.md) |  | 295 |
| [source-graph-delivery](mistakes/source-graph-delivery.md) | Misses | 182 |
| [spurious-waiting-for-input](mistakes/spurious-waiting-for-input.md) | Idle | 53 |
| [stage-build-cache-delivery](mistakes/stage-build-cache-delivery.md) | Stage cache delivery: credential forwarding, sockets, seeds, gate timing | 44 |
| [status-broadcast-hardening](mistakes/status-broadcast-hardening.md) |  | 75 |
| [store-without-consumer](mistakes/store-without-consumer.md) | Unread | 104 |
| [subagent-briefing](mistakes/subagent-briefing.md) |  | 394 |
| [subagent-liveness-and-watch](mistakes/subagent-liveness-and-watch.md) |  | 387 |
| [subagent-orchestration](mistakes/subagent-orchestration.md) | Agents | 397 |
| [test-concurrency-and-fixtures](mistakes/test-concurrency-and-fixtures.md) | Races | 398 |
| [test-hermeticity](mistakes/test-hermeticity.md) | Tests that reached real network, keychain or home state | 13 |
| [test-sweep-and-fixture-discipline](mistakes/test-sweep-and-fixture-discipline.md) |  | 398 |
| [testing-and-lint](mistakes/testing-and-lint.md) |  | 408 |
| [tests-that-cannot-fail](mistakes/tests-that-cannot-fail.md) |  | 392 |
| [tmux-backend](mistakes/tmux-backend.md) | tmux | 164 |
| [typed-config-values-process](mistakes/typed-config-values-process.md) | Dev | 86 |
| [untrusted-value-boundaries](mistakes/untrusted-value-boundaries.md) | Trust | 252 |
| [verification-harness](mistakes/verification-harness.md) |  | 399 |
| [verification-v2-delivery](mistakes/verification-v2-delivery.md) | v2 | 396 |
| [visibility-and-reachability](mistakes/visibility-and-reachability.md) |  | 141 |
| [web-dashboard](mistakes/web-dashboard.md) | UI | 194 |
| [web-dashboard-server](mistakes/web-dashboard-server.md) | Races | 278 |
| [web-terminal](mistakes/web-terminal.md) | TTY | 142 |
| [writer-reader-address](mistakes/writer-reader-address.md) | Keys | 115 |

### concerns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [agent-rule-bending-hardening](concerns/agent-rule-bending-hardening.md) |  | 375 |
| [automatic-knowledge-source-graph-followups](concerns/automatic-knowledge-source-graph-followups.md) | Gap | 69 |
| [code-quality-and-hook-debt](concerns/code-quality-and-hook-debt.md) | Debt | 315 |
| [codex-heartbeat-starvation](concerns/codex-heartbeat-starvation.md) |  | 22 |
| [consolidation-followups](concerns/consolidation-followups.md) |  | 148 |
| [contract-autonomy-followups](concerns/contract-autonomy-followups.md) |  | 70 |
| [dispute-guards-followups](concerns/dispute-guards-followups.md) | open review suggestions: trial, overlay undo, narrowing, prompt text | 45 |
| [handoff-followups](concerns/handoff-followups.md) |  | 57 |
| [knowledge-cli-gaps](concerns/knowledge-cli-gaps.md) |  | 108 |
| [loom-ask-and-read-gate](concerns/loom-ask-and-read-gate.md) |  | 108 |
| [merge-and-recovery-edge-cases](concerns/merge-and-recovery-edge-cases.md) | Edges | 105 |
| [native-codex-qualification](concerns/native-codex-qualification.md) |  | 95 |
| [plan-chat-followups](concerns/plan-chat-followups.md) | Chat | 41 |
| [planner-follow-ups-followups](concerns/planner-follow-ups-followups.md) |  | 91 |
| [planner-kickoff-fixes-followups](concerns/planner-kickoff-fixes-followups.md) | Kickoff-fixes open items: web/dist hook, gate env, reviewer suggestions | 31 |
| [planner-ui-repair-followups](concerns/planner-ui-repair-followups.md) | UI | 214 |
| [planning-document-model-followups](concerns/planning-document-model-followups.md) |  | 170 |
| [planning-guidance-followups](concerns/planning-guidance-followups.md) |  | 43 |
| [planning-session-sandbox-limits](concerns/planning-session-sandbox-limits.md) |  | 238 |
| [platform-and-commit-gaps](concerns/platform-and-commit-gaps.md) |  | 247 |
| [runtime-and-session-safety](concerns/runtime-and-session-safety.md) |  | 137 |
| [sandbox-and-confinement-gaps](concerns/sandbox-and-confinement-gaps.md) |  | 400 |
| [source-graph-known-gaps](concerns/source-graph-known-gaps.md) | Limits | 94 |
| [source-graph-review-backlog](concerns/source-graph-review-backlog.md) | Todo | 295 |
| [stage-build-cache-gaps](concerns/stage-build-cache-gaps.md) | Open reviewer suggestions and residuals of the stage build cache | 25 |
| [state-confinement-gaps](concerns/state-confinement-gaps.md) | Caches | 21 |
| [token-accounting-and-proof-defects](concerns/token-accounting-and-proof-defects.md) |  | 333 |
| [typed-config-values](concerns/typed-config-values.md) | Gap | 22 |
| [verification-v2-followups](concerns/verification-v2-followups.md) |  | 234 |
| [web-dashboard-latent-issues](concerns/web-dashboard-latent-issues.md) | UI | 96 |
| [worktree-build-cold-start](concerns/worktree-build-cold-start.md) | Cold | 108 |
