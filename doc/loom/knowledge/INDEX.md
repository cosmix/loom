<!-- generated automatically on knowledge writes — do not edit by hand -->

# Knowledge Index

> Read this index first, then only what it points to: the section for your area in a tier-1 summary (`rg -n '^## ' <file>` lists them) and the tier-2 topics your task touches. A specific question is cheaper to pull than to read — `loom knowledge context --query "..."` returns the matching sections quoted.

## Tier 1 — Summaries

| File | Description | Lines |
| --- | --- | --- |
| [architecture.md](architecture.md) | High-level component relationships, data flow, module dependencies | 246 |
| [entry-points.md](entry-points.md) | Key files agents should read first | 86 |
| [patterns.md](patterns.md) | Architectural patterns discovered in the codebase | 200 |
| [conventions.md](conventions.md) | Coding conventions discovered in the codebase | 230 |
| [mistakes.md](mistakes.md) | Mistakes made and lessons learned - what to avoid | 245 |
| [stack.md](stack.md) | Dependencies, frameworks, and tooling used in the project | 116 |
| [concerns.md](concerns.md) | Technical debt, warnings, and issues to address | 248 |

## Tier 2 — Topics

### architecture

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-lifecycle](architecture/adjudication-lifecycle.md) |  | 167 |
| [codex-concurrency](architecture/codex-concurrency.md) | Limits | 194 |
| [codex-plugin](architecture/codex-plugin.md) |  | 353 |
| [context-ceiling](architecture/context-ceiling.md) |  | 111 |
| [context-retrieval](architecture/context-retrieval.md) |  | 400 |
| [context-retrieval-corpus](architecture/context-retrieval-corpus.md) | BM25 | 178 |
| [context-retrieval-state](architecture/context-retrieval-state.md) | Layers | 253 |
| [contract-collection](architecture/contract-collection.md) |  | 124 |
| [contract-phase](architecture/contract-phase.md) | Freeze | 281 |
| [core-abstractions](architecture/core-abstractions.md) | Graph | 207 |
| [daemon-owned-commits](architecture/daemon-owned-commits.md) | How a completed/parked/stalled session's disposition is proven trustworthy, and… | 186 |
| [directory-structure](architecture/directory-structure.md) | Tree | 50 |
| [execution-containment](architecture/execution-containment.md) |  | 399 |
| [hook-system](architecture/hook-system.md) |  | 343 |
| [knowledge-hierarchy](architecture/knowledge-hierarchy.md) | KB fs | 346 |
| [memory-spool](architecture/memory-spool.md) |  | 211 |
| [merge-flow](architecture/merge-flow.md) |  | 161 |
| [orchestrator-loop](architecture/orchestrator-loop.md) | Ticks | 214 |
| [plan-lifecycle-and-fields](architecture/plan-lifecycle-and-fields.md) |  | 302 |
| [plan-server](architecture/plan-server.md) | Chat | 293 |
| [planner-feature-screens](architecture/planner-feature-screens.md) | Spine | 113 |
| [planner-web-shell](architecture/planner-web-shell.md) | Shell | 147 |
| [planning-document-model](architecture/planning-document-model.md) | Docs | 266 |
| [planning-session-bridge](architecture/planning-session-bridge.md) |  | 355 |
| [planning-session-sandbox](architecture/planning-session-sandbox.md) |  | 148 |
| [planning-trace](architecture/planning-trace.md) |  | 145 |
| [read-gate](architecture/read-gate.md) |  | 221 |
| [remote-control](architecture/remote-control.md) |  | 81 |
| [run-handoff](architecture/run-handoff.md) |  | 52 |
| [security-and-isolation](architecture/security-and-isolation.md) | Deny | 203 |
| [signal-generation](architecture/signal-generation.md) |  | 261 |
| [skill-catalog](architecture/skill-catalog.md) |  | 158 |
| [source-graph](architecture/source-graph.md) |  | 372 |
| [source-graph-build](architecture/source-graph-build.md) |  | 142 |
| [source-graph-resolution](architecture/source-graph-resolution.md) |  | 269 |
| [stage-build-cache](architecture/stage-build-cache.md) | Per-stage cache root: env, seeds, sockets, read roots, xcrun bypass | 60 |
| [stage-exit-recording](architecture/stage-exit-recording.md) | Stage exits: refused commit, idle stop, verdicts, artifact disputes | 41 |
| [status-data-model](architecture/status-data-model.md) | Fields | 269 |
| [summary-store](architecture/summary-store.md) |  | 352 |
| [target-guard](architecture/target-guard.md) | Tip | 180 |
| [terminal-backends](architecture/terminal-backends.md) |  | 198 |
| [token-accounting-and-receipts](architecture/token-accounting-and-receipts.md) | Ledger | 355 |
| [usage-evidence](architecture/usage-evidence.md) |  | 256 |
| [verification-v2-gates](architecture/verification-v2-gates.md) | Gates | 278 |
| [web-dashboard](architecture/web-dashboard.md) | Web | 137 |
| [web-terminal](architecture/web-terminal.md) |  | 199 |

### entry-points

| Topic | Blurb | Lines |
| --- | --- | --- |
| [cli-and-plan-pipeline](entry-points/cli-and-plan-pipeline.md) | CLI | 197 |
| [context-and-source-graph](entry-points/context-and-source-graph.md) |  | 80 |
| [filesystem-and-integration-modules](entry-points/filesystem-and-integration-modules.md) |  | 134 |
| [hooks](entry-points/hooks.md) |  | 152 |
| [orchestrator-daemon-and-sessions](entry-points/orchestrator-daemon-and-sessions.md) |  | 261 |
| [remote-control](entry-points/remote-control.md) | RC | 96 |

### patterns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [canonical-helpers](patterns/canonical-helpers.md) |  | 72 |
| [cli-process-and-conventions](patterns/cli-process-and-conventions.md) |  | 229 |
| [doctrine-cross-surface](patterns/doctrine-cross-surface.md) | Pins | 144 |
| [hook-content-stripping](patterns/hook-content-stripping.md) | Text | 162 |
| [merge-and-recovery](patterns/merge-and-recovery.md) |  | 111 |
| [orchestrator-daemon-loop](patterns/orchestrator-daemon-loop.md) | Loop | 112 |
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
| [defect-triage](conventions/defect-triage.md) | Blocking and security defects are fixed when found, not deferred | 9 |
| [dispute-and-adjudication](conventions/dispute-and-adjudication.md) |  | 156 |
| [git-and-build-workflow](conventions/git-and-build-workflow.md) |  | 222 |
| [guidance-channels-and-plugin-scope](conventions/guidance-channels-and-plugin-scope.md) |  | 134 |
| [host-job-test-guards](conventions/host-job-test-guards.md) | macOS host-job denials, sandbox_probe guards, strict-CI skips | 67 |
| [model-and-effort-config](conventions/model-and-effort-config.md) |  | 147 |
| [plan-web](conventions/plan-web.md) |  | 215 |
| [plan-yaml-and-hooks](conventions/plan-yaml-and-hooks.md) |  | 194 |
| [web-dashboard-typography](conventions/web-dashboard-typography.md) |  | 68 |

### mistakes

| Topic | Blurb | Lines |
| --- | --- | --- |
| [adjudication-autonomy-deadlock](mistakes/adjudication-autonomy-deadlock.md) |  | 238 |
| [ambient-filesystem-trust](mistakes/ambient-filesystem-trust.md) |  | 186 |
| [briefs-and-bug-reports](mistakes/briefs-and-bug-reports.md) |  | 396 |
| [cargo-under-confinement](mistakes/cargo-under-confinement.md) | Cargo in sandboxes and confined runs: audit fetches, cold builds, Seatbelt | 100 |
| [checkout-identity](mistakes/checkout-identity.md) | Persisted checkout identity: pin path and inode, never st_dev | 13 |
| [ci-gate-rollout](mistakes/ci-gate-rollout.md) | CI gate rollout slips: untested strict gates, shared index, known-red runs | 84 |
| [ci-toolchain-and-cargo](mistakes/ci-toolchain-and-cargo.md) |  | 399 |
| [closed-pipe-panics](mistakes/closed-pipe-panics.md) | println! on a closed stdout panics a server thread; drain piped stdout in tests | 13 |
| [codex-lane-rogue-wrapper](mistakes/codex-lane-rogue-wrapper.md) |  | 207 |
| [codex-navigation](mistakes/codex-navigation.md) | Reader | 202 |
| [codex-worker-briefing](mistakes/codex-worker-briefing.md) |  | 258 |
| [completion-broker-credential](mistakes/completion-broker-credential.md) |  | 225 |
| [completion-runner-blocks](mistakes/completion-runner-blocks.md) |  | 231 |
| [completion-runtime](mistakes/completion-runtime.md) | Churn | 134 |
| [computed-values-and-hidden-couplings](mistakes/computed-values-and-hidden-couplings.md) | Values | 284 |
| [concurrency-and-locking](mistakes/concurrency-and-locking.md) |  | 207 |
| [consolidation-drift](mistakes/consolidation-drift.md) |  | 139 |
| [contract-autonomy-delivery](mistakes/contract-autonomy-delivery.md) | Bypass | 127 |
| [daemon-singleton](mistakes/daemon-singleton.md) |  | 176 |
| [detached-spawn-in-tests](mistakes/detached-spawn-in-tests.md) |  | 95 |
| [dev-install](mistakes/dev-install.md) | Daemon | 68 |
| [dispute-guards-delivery](mistakes/dispute-guards-delivery.md) | host-job hangs, parallel worker collisions, scaffold wording, hook quirks | 141 |
| [doctrine-and-acceptance](mistakes/doctrine-and-acceptance.md) |  | 392 |
| [fixture-replay-waits](mistakes/fixture-replay-waits.md) | Waiting on a state a replayed session already holds races the bridge | 11 |
| [hooks-macos-and-bash32](mistakes/hooks-macos-and-bash32.md) | macOS Bash 3.2 and BSD tool faults that Linux hook runs hid | 96 |
| [hooks-shell-portability](mistakes/hooks-shell-portability.md) |  | 319 |
| [host-verify-group-containment](mistakes/host-verify-group-containment.md) |  | 47 |
| [knowledge-base-drift](mistakes/knowledge-base-drift.md) |  | 286 |
| [knowledge-cli-invariants](mistakes/knowledge-cli-invariants.md) | Rules | 241 |
| [knowledge-write-channel](mistakes/knowledge-write-channel.md) | Writes | 156 |
| [ledger-tui-rendering](mistakes/ledger-tui-rendering.md) |  | 84 |
| [live-state-pollution](mistakes/live-state-pollution.md) |  | 143 |
| [macos-socket-and-pty-tests](mistakes/macos-socket-and-pty-tests.md) | macOS setsockopt EINVAL after peer close; PTY 1 KiB reads in pacing tests | 17 |
| [maintainability-baseline](mistakes/maintainability-baseline.md) |  | 134 |
| [merge-cleanup-boundary](mistakes/merge-cleanup-boundary.md) |  | 244 |
| [merge-in-operator-checkout](mistakes/merge-in-operator-checkout.md) |  | 77 |
| [native-macos-delivery](mistakes/native-macos-delivery.md) | macOS boundary delivery slips: lint scope, pinned fields, shared TMPDIR | 67 |
| [parallel-worktree-shared-state](mistakes/parallel-worktree-shared-state.md) | Shared | 361 |
| [phantom-merges](mistakes/phantom-merges.md) | Merges | 347 |
| [pinned-literals-ledgers-and-wiring](mistakes/pinned-literals-ledgers-and-wiring.md) | Pins | 360 |
| [plan-authoring](mistakes/plan-authoring.md) |  | 357 |
| [plan-server-delivery](mistakes/plan-server-delivery.md) | Slips | 376 |
| [plan-stats-phase-timing](mistakes/plan-stats-phase-timing.md) | Phase spans and timeline bars that ended a stage at completed_at | 13 |
| [planner-design-brainstorm](mistakes/planner-design-brainstorm.md) | Stubs | 141 |
| [planner-follow-ups-delivery](mistakes/planner-follow-ups-delivery.md) | Slips | 196 |
| [planner-polish](mistakes/planner-polish.md) | Polish | 155 |
| [planner-redesign](mistakes/planner-redesign.md) |  | 101 |
| [planner-review-fixes](mistakes/planner-review-fixes.md) |  | 149 |
| [planner-ui-repair](mistakes/planner-ui-repair.md) | Repair | 215 |
| [planning-sandbox-delivery](mistakes/planning-sandbox-delivery.md) |  | 98 |
| [pre-commit-hardening](mistakes/pre-commit-hardening.md) |  | 93 |
| [refactor-stragglers](mistakes/refactor-stragglers.md) |  | 341 |
| [review-continuation-verification](mistakes/review-continuation-verification.md) | Repair | 331 |
| [review-final-wave](mistakes/review-final-wave.md) |  | 200 |
| [review-integration-verification](mistakes/review-integration-verification.md) |  | 311 |
| [review-remediation-stage-practice](mistakes/review-remediation-stage-practice.md) | Stage-practice mistakes: scratch rm, cwd, fmt, gate timing, briefs | 123 |
| [review-tracker](mistakes/review-tracker.md) |  | 363 |
| [sandbox-and-settings](mistakes/sandbox-and-settings.md) |  | 391 |
| [sandbox-protected-hooks-dir](mistakes/sandbox-protected-hooks-dir.md) | Sandbox-protected hooks/ dir, inert Write() rules, hook preflight denylist | 113 |
| [sandbox-state-channels](mistakes/sandbox-state-channels.md) |  | 299 |
| [sandbox-tooling-and-network](mistakes/sandbox-tooling-and-network.md) |  | 397 |
| [schema-reuse-and-silent-skips](mistakes/schema-reuse-and-silent-skips.md) |  | 130 |
| [session-identity-env](mistakes/session-identity-env.md) | Env | 126 |
| [sessions-and-liveness](mistakes/sessions-and-liveness.md) |  | 399 |
| [shell-command-matchers](mistakes/shell-command-matchers.md) |  | 295 |
| [source-graph-delivery](mistakes/source-graph-delivery.md) | Misses | 182 |
| [spurious-waiting-for-input](mistakes/spurious-waiting-for-input.md) | Idle | 53 |
| [status-broadcast-hardening](mistakes/status-broadcast-hardening.md) | Status frame overflow, relay ticket leaks and backgrounded relayed commands | 120 |
| [store-without-consumer](mistakes/store-without-consumer.md) | Unread | 104 |
| [subagent-briefing](mistakes/subagent-briefing.md) |  | 394 |
| [subagent-liveness-and-watch](mistakes/subagent-liveness-and-watch.md) |  | 387 |
| [subagent-orchestration](mistakes/subagent-orchestration.md) | Agents | 397 |
| [test-concurrency-and-fixtures](mistakes/test-concurrency-and-fixtures.md) | Races | 396 |
| [test-sweep-and-fixture-discipline](mistakes/test-sweep-and-fixture-discipline.md) |  | 398 |
| [testing-and-lint](mistakes/testing-and-lint.md) |  | 397 |
| [tests-that-cannot-fail](mistakes/tests-that-cannot-fail.md) |  | 392 |
| [tmux-backend](mistakes/tmux-backend.md) | tmux | 176 |
| [typed-config-values-process](mistakes/typed-config-values-process.md) | Dev | 86 |
| [untrusted-value-boundaries](mistakes/untrusted-value-boundaries.md) | Trust | 252 |
| [verification-harness](mistakes/verification-harness.md) |  | 399 |
| [verification-v2-delivery](mistakes/verification-v2-delivery.md) | v2 | 396 |
| [visibility-and-reachability](mistakes/visibility-and-reachability.md) |  | 141 |
| [web-dashboard](mistakes/web-dashboard.md) | UI | 211 |
| [web-dashboard-server](mistakes/web-dashboard-server.md) | Races | 288 |
| [web-terminal](mistakes/web-terminal.md) | TTY | 166 |
| [writer-reader-address](mistakes/writer-reader-address.md) | Keys | 115 |

### concerns

| Topic | Blurb | Lines |
| --- | --- | --- |
| [agent-rule-bending-hardening](concerns/agent-rule-bending-hardening.md) |  | 351 |
| [automatic-knowledge-source-graph-followups](concerns/automatic-knowledge-source-graph-followups.md) | Gap | 68 |
| [code-quality-and-hook-debt](concerns/code-quality-and-hook-debt.md) | Debt | 325 |
| [consolidation-followups](concerns/consolidation-followups.md) |  | 147 |
| [contract-autonomy-followups](concerns/contract-autonomy-followups.md) |  | 122 |
| [knowledge-cli-gaps](concerns/knowledge-cli-gaps.md) |  | 82 |
| [loom-ask-and-read-gate](concerns/loom-ask-and-read-gate.md) |  | 107 |
| [merge-and-recovery-edge-cases](concerns/merge-and-recovery-edge-cases.md) | Edges | 105 |
| [native-codex-qualification](concerns/native-codex-qualification.md) |  | 139 |
| [planner-follow-ups-followups](concerns/planner-follow-ups-followups.md) | Planner, chat, guidance, kickoff and typed-config open follow-ups | 221 |
| [planner-ui-repair-followups](concerns/planner-ui-repair-followups.md) | UI | 212 |
| [planning-document-model-followups](concerns/planning-document-model-followups.md) |  | 173 |
| [planning-session-sandbox-limits](concerns/planning-session-sandbox-limits.md) |  | 234 |
| [platform-and-commit-gaps](concerns/platform-and-commit-gaps.md) |  | 237 |
| [review-remediation-followups](concerns/review-remediation-followups.md) | Open review suggestions and findings left out of the remediation plan | 112 |
| [runtime-and-session-safety](concerns/runtime-and-session-safety.md) |  | 201 |
| [sandbox-and-confinement-gaps](concerns/sandbox-and-confinement-gaps.md) |  | 394 |
| [source-graph-known-gaps](concerns/source-graph-known-gaps.md) | Limits | 94 |
| [source-graph-review-backlog](concerns/source-graph-review-backlog.md) | Todo | 287 |
| [stage-exits-and-guards-followups](concerns/stage-exits-and-guards-followups.md) | Out-of-scope rows, open reviewer suggestions, proposed checks after exits plan | 82 |
| [state-confinement-gaps](concerns/state-confinement-gaps.md) | Open confinement gaps: caches, task outputs, macOS networking, stage cache | 77 |
| [token-accounting-and-proof-defects](concerns/token-accounting-and-proof-defects.md) |  | 323 |
| [verification-v2-followups](concerns/verification-v2-followups.md) |  | 247 |
| [web-dashboard-latent-issues](concerns/web-dashboard-latent-issues.md) | UI | 100 |
| [worktree-build-cold-start](concerns/worktree-build-cold-start.md) | Cold | 121 |
