# Concerns & Technical Debt

> Debt and open issues
> Every section here must be an OPEN concern. When one is resolved, DELETE it — do not strike the
> heading and leave the body, which is how eleven dead entries accumulated before 2026-08-26. Git
> history keeps the record and [mistakes.md](mistakes.md) keeps the lesson. If a resolved concern
> leaves a genuine residual, keep only the residual, under a plain heading.
>
> **Related files:** [mistakes.md](mistakes.md) for lessons learned, [architecture.md](architecture.md) for context.

## Architecture Concerns

### Layering Violations

Lower layers import from higher ones:

- daemon calls commands: `loom/src/daemon/server/client.rs::authorize_preface` (`commands::stage::admin_proof::verify_and_consume_admin_proof`), `loom/src/daemon/server/status.rs::collect_status` and `loom/src/daemon/protocol.rs::Response` (`commands::status::data`)
- orchestrator calls commands: `loom/src/orchestrator/core/coherence.rs::begin_scheduling_pass` (`commands::stage::complete::strip_privileged_env_for_runtime`), `loom/src/orchestrator/core/inbox_drain/merge_resolved.rs::resolve_merge_from_inbox` and `loom/src/orchestrator/core/merge_handler/resolver_exit.rs::land_resolved_worktree` (`commands::stage::complete::verify_resolved_completion`)
- git imports orchestrator: `loom/src/git/cleanup/orphans.rs` (`orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle}`)
- models imports plan: `loom/src/models/dispute.rs`, `loom/src/models/stage/types.rs`, `loom/src/models/stage/checks.rs`, `loom/src/models/stage/methods.rs`

## Code Quality Concerns

### Oversized Rust Units Remain Controlled Debt

`loom/maintainability-baseline.txt` and `loom/tests/maintainability.rs` track exact Rust size exceptions and reject growth or stale entries; they do not establish universal compliance. Several shell hook owners also exceed the repository's 400-line limit.

Extension-to-language lookup is shared; project manifest detection still has competing registries. The subagent verification hook uses a private command-position scanner, while its stage-type exemption scans whole Markdown. Remote hooks ship through the embedded binary's `install-assets` command.

→ [Code Quality and Hook Debt](concerns/code-quality-and-hook-debt.md)

### Hook False-Positive Plan Follow-Ups

`commit-filter.sh` (463 lines) needs a split; `subagent-verify-guard.sh` sources its runner matchers from `_verify_matchers.sh`. Seven guards and the codex lane record no blocks. The scratch-repo exemption reads repository state at hook time, and several tokenizer and resolver gaps remain (backslash-newline, `function`/`coproc`, logical `..`, `HOME`). `spawn-guard.sh`'s header still describes the old `updatedInput` behaviour.

→ [Hook False-Positive Plan Follow-Ups](concerns/code-quality-and-hook-debt.md#hook-false-positive-plan-follow-ups)

## Long Codex Runs Starve the Loom Heartbeat

A foreground codex-lane run is ONE blocking Bash call, so neither `PostToolUse` nor `SubagentStop`
can refresh the heartbeat until it returns. A stale heartbeat is acted on: a stage that had worked
is re-queued at 3x its budget (twice at most, then parked in `NeedsHumanReview`), and a stage whose
first tool call is a long foreground run counts as never worked and is parked at one budget.
Mitigation is doctrine (bound the task, set `subagent_timeout_secs`), not a monitor change; raising
the global timeout was considered and rejected. The `loom status` badge shows such a stage as `Busy`
(open tool call) and measures silence against the same per-stage budget as the monitor.

Full detail: [codex-heartbeat-starvation.md](concerns/codex-heartbeat-starvation.md).

## Automatic-Knowledge Plan Followups

Four open items: an unbounded whole-file read ahead of the extraction size cap,
three production-dead `KnowledgeDir` methods kept alive only by each other's tests,
a writer/reader plan-key normalisation mismatch, and a `LOOM_PERMISSIONS_WORKTREE`
constant with no consumer. The topic also covers natural-language queries that
corpus stopwording empties.

Full detail: [automatic-knowledge-source-graph-followups.md](concerns/automatic-knowledge-source-graph-followups.md).

## Tier-1 Knowledge Housekeeping Backlog

`loom knowledge check --strict` exits 0 on this tree under the tier-1 limits (250 lines per file, 40 per
section), the tier-2 limits (400 per file, 80 per section) and the 16 KB cap on `INDEX.md`, which has about 100
bytes of headroom, so every new topic must be paid for with shorter blurbs. What remains is the CLI's own rough
edges: `replace-section`'s CRLF/trailing-blank-line quirks, `update`'s stdin-vs-inline trim mismatch, no blurb flag on
`update`, no in-place heading rename (delete-section plus update is the workaround), and the disagreement between the
chunker's and the splicer's fenced-code models. `--baseline` and `--write-baseline` exist for adopting a limit on a
tree that cannot clear it yet; none is needed while `--strict` stays green.

→ [Knowledge CLI Gaps](concerns/knowledge-cli-gaps.md)

## Web Dashboard Latent Issues

Four issues reviewed and deliberately left unchanged in `loom/src/commands/status/web/`: a mutex-poisoning cascade risk, a cosmetic `GET /ws` status-code mismatch, an inherited partial-frame truncation risk shared with the TUI, and a left-in-place bundle-size warning. Detail: [concerns/web-dashboard-latent-issues.md](concerns/web-dashboard-latent-issues.md).

## Markdown Lint Blocks Only at Push

`loom/.githooks/pre-commit:72` runs `markdownlint-cli2 --fix` and ignores its exit status (it warns only when `bunx`
cannot run), so an error `--fix` cannot repair, such as a table row missing a cell (MD055/MD056), commits silently;
`pre-push:78` blocks it. Lint new markdown with `bunx markdownlint-cli2 <files>` before committing.

## Source Graph Limits and Review Backlog

The graph states what it cannot see: C/C++ macros and templates, reflection and dynamic dispatch, JavaScript
`#private` members, Ruby bare calls, inherited members, and Kotlin, Swift and shell (not supported). Open decisions:
the storage engine waits on warm `--timings` data, the agent-task comparison and `scripts/retrieval-ab` have not been
run, and the labelled corpora enter loom's own graph because `EXCLUDED_ROOTS` matches only a first path segment. About
150 small reviewer suggestions (resolver, extraction, view store, lease hardening, routing, evaluator, fixtures) are
grouped by area in the backlog.

→ [Source Graph Known Gaps](concerns/source-graph-known-gaps.md), [Source Graph Review Backlog](concerns/source-graph-review-backlog.md)

## Platform, Commit-Relay and Stall Gaps

Remaining platform/runtime questions include live macOS branch coverage, process limits on hook tests, full daemon re-exec coverage, intentionally filtered session environment, synchronous commit and login-probe costs, merge-side authorization, and status/watch evidence. Hook stdin reads use the portable bounded helper; separate direct-command timeout fallbacks remain. A dedicated Linux capsule step requires sandbox availability, while general test jobs can still skip capability-gated cases. Open items on the busy line and `busy::while_busy` are in [Busy Status Follow-Ups](concerns/platform-and-commit-gaps.md#busy-status-follow-ups).

→ [Platform and Commit Gaps](concerns/platform-and-commit-gaps.md)

## Token Accounting Follow-Ups

Open follow-ups from the token-accounting plans: git and bounded-runner hygiene, whether a failed forward fires PostToolUse, two fail-open guard choices, and the unimplemented reviewer suggestions of the delivery-truth and usage-evidence plans.

→ [Token Accounting Follow-Ups](concerns/token-accounting-and-proof-defects.md). Token flow limits, hardening and structure: [Token Flow Gaps](concerns/token-accounting-and-proof-defects.md#token-flow-gaps).

## `loom ask` and Read Gate Concerns

`loom ask` has no evaluation and no isolation probe yet, and its Codex lane is experimental, so no agent, skill, hook or doctrine text may tell an agent to use it until the evaluation and the replays pass. The read gate ships on by default with no replay behind it. Also the unimplemented reviewer suggestions for both, and proposed guards for two recurring mistakes.

→ [`loom ask` and Read Gate Concerns](concerns/loom-ask-and-read-gate.md)

## Host-Verification Review Suggestions

Open and fixed reviewer findings on the host-verification code, one bullet per defect with its file:line. The open ones cover the apply and recovery path, criteria confinement, observer git discovery, and skill and reference text that still describes the session sandbox as the criteria environment.

→ [Host-Verification Review Suggestions](concerns/source-graph-review-backlog.md)

## Pressure Gaps

`loom pressure`'s two real-invocation smokes are manual-only, its dry-run argv is not shell-quoted so it cannot be
pasted into a shell, and repo-root resolution is duplicated in two places. The app pressure and run-lane backlog is in
[Handoff Follow-ups](concerns/handoff-followups.md).

## Execution Graph and Forward Guard Gaps

`ExecutionGraph::mark_queued` refuses a finished node, but recovery sync answers that refusal by forcing the node to
`WaitingForDeps`, so a stale in-memory graph can still reset a `Completed` node. The macOS branches of `loom_stage_evidence` (the Codex forward guard) were written on Linux and
never run; the topic lists the checks to do on a Mac.

→ [Execution Graph and Forward Guard Gaps](concerns/code-quality-and-hook-debt.md)

## Deferred Worktree Cleanup Has Two Residual Edge Cases

Two residuals in worktree cleanup: daemon cleanup can still race SessionEnd hooks during the short
SIGTERM teardown window, and manual `loom stage complete` with no daemon running defers cleanup
until the next recovery pass or an explicit `loom worktree remove`.

## Guard Hooks: Four Design Questions Deliberately Left Open

The adversarial review of the guard hooks left four behaviours unpatched pending a spec decision (the
deny threshold, ledger TSV atomicity, `loom_deny_enabled`'s line-oriented match, an unreachable
`poll-guard` branch). The entry also covers the wider runtime/session cluster: tmux-warning false
failures, live overview-attach panes, the unreaped viewer socket, path-swap races, `loom attach`'s
stage-bound lifetime, startup-only orphan adoption, `get_work_dir()`'s substring trust, and
process-global cwd mutation in memory tests.

→ [Runtime and Session Safety](concerns/runtime-and-session-safety.md)

## State Confinement Gaps [DETAILED]

Shared package-manager caches stay session-writable, so a stage can edit an extracted crate's
`build.rs` that the operator's next host build runs.

→ [State Confinement Gaps](concerns/state-confinement-gaps.md)

## Agent Rule-Bending Hardening

The OS sandbox, attached session capsule and daemon ancestry checks carry authority; hook command matching is best effort. Remaining questions concern broad git/cache/Codex write grants, native file-tool credential coverage, live enforcement probes and knowledge-result validation. The authoritative merge-stage control-path gate fails closed on a diff error; the early orchestrator filter can defer to it.

→ [Agent Rule-Bending Hardening](concerns/agent-rule-bending-hardening.md)

## Typed Config Values: Known Gaps

Accepted limitations and test-coverage gaps in the typed config read-path (TUI control-char stripping, no non-interactive unset, an untagged-enum wire limitation, a TUI test gap). See [Typed Config Values: Known Gaps](concerns/typed-config-values.md).

## Verification v2 Follow-Ups

Nine of the 23 test-runner adapters (cargo-nextest, gradle, maven, sbt, rspec, phpunit, pest, swift-test, mix-test) have
fixtures written from documented output, not captured runs, so their parsers are unproven. Adapter gaps, contract-phase
gaps, duplicated helpers, provision and environment-lint gaps, gate and dispute backlog: [verification-v2-followups](concerns/verification-v2-followups.md). Contract chain-check blind spots, freeze-refusal filing, verdict and briefing gaps, test gaps and the relayed-command background hook proposal: [contract-autonomy-followups](concerns/contract-autonomy-followups.md).

## Merge Path and Recovery Gaps

The merge/recovery edge cases: `loom stage merge`'s worktree-cwd requirement, a git-error-as-unverified
revert path, the `BranchMissing` phantom-merge risk, the heuristic `BaseConflict` carve-out,
`retry --force` racing orphan-recovery, the completion-broker's
post-transition nonce-burn ordering, the attribution code that has nothing left to attribute, the
editor residual between stash and pop, the `update-ref` window,
accumulating autostash backup refs, `human-review --force-complete` requiring no operator proof by design, a
newline inside a refused fast-forward path, and a loom gate error that parks a stage with no self-recovery (no
dispute covers loom's own gates, a deliberate block is never retried; proposed: a could-not-evaluate gate result).

→ [Merge and Recovery Edge Cases](concerns/merge-and-recovery-edge-cases.md)

## Sandbox Denial Is Unproven End to End

CI proves selected emitted capsule denies under bubblewrap; it does not exercise a credentialed Claude Code session or every native tool route. The srt stand-in emulates the linked-worktree common-directory grant and built-in git denies. The release checklist (`doc/sandbox-release-checklist.md`) remains the live runtime check.

Open boundaries include native file-tool credential coverage in checkout-rooted sessions, broad git/common-cache/Codex grants, spawn-time sibling-directory enumeration, unmeasured tool routes, and bootstrap launches without the shared capsule. Codex AGENTS, prompts, catalog and Loom-installed skills are denied; remaining Codex state needs separate assessment. Preserve the ban on `Read(...)` permission denies and keep stage environment filtering explicit.

→ [Sandbox and Confinement Gaps](concerns/sandbox-and-confinement-gaps.md), [Agent Rule-Bending Hardening](concerns/agent-rule-bending-hardening.md)

## Dependency Findings Follow-Ups

The plan 4 harness printed a stop-rule FAIL on all three recorded pairs (guard-core to guard-integration gains -444 and -2,221 tokens against a 12,190 cost); the operator overrode it and the verdict is printed, not asserted. Also open: spool id validation, render and path-normalization cost bounds, evidence-matching edges, test gaps, and the finalization-guard heredoc scan.

→ [Dependency Findings Follow-Ups](concerns/token-accounting-and-proof-defects.md#dependency-findings-follow-ups)

## Planning Document Model Open Items

Unapplied reviewer suggestions for `loom::planning`: heading and cross-reference parsing edges, refresh and lock-scan cost, symlink and non-UTF-8 handling in `LockedSet::scan` and `collect_inputs`, Unicode folding in the locked-name rules, and untested branches; plus the summary store's open robustness items (store lock timeout, unreadable summary files, claim-removal failure after a seal) and test gaps.

The planning trace's open checks (journeys that need a real session or tmux, the settle stale window, the `Settled:` prose-line hash, signed-commit `git log` output) and its unapplied reviewer suggestions are in [Planning Trace Limits](concerns/planning-document-model-followups.md#planning-trace-limits).

→ [Planning Document Model Followups](concerns/planning-document-model-followups.md)

## Every Stage Worktree Builds From Cold [DETAILED]

A new stage worktree has no build state, so its first compile or install is cold in every ecosystem; the contract freeze pays it first (about 2 min 20 s of a 3.5-minute contract phase on loom). sccache is gated off in sandboxed sessions and stripped from confined host jobs, so nothing is shared. Options, constraints (parallel stages, all languages, macOS and Linux, no copies, no worktree pool) and next steps:

→ [Every Stage Worktree Builds From Cold](concerns/worktree-build-cold-start.md)

## Concept Mode Open Items

Reviewer suggestions left open on concept mode: engine-failure and claims-fallback paths that name no pages or call no listener, a summary hash read apart from its evaluation, accessibility and path-rendering gaps, passage provenance scope, and presence not wired to concept pages.

→ [Planning Document Model Followups](concerns/planning-document-model-followups.md#concept-mode-server-gaps)

## Planning Session Sandbox Limits

Accepted limits of the planning sandbox (Bash writes inside `doc/loom/`, new top-level paths, locks taken mid-session, the guard's check-to-write race), the untested Seatbelt and provenance paths, and the unimplemented reviewer suggestions. → [Planning Session Sandbox Limits](concerns/planning-session-sandbox-limits.md)

## Planning Session Bridge Follow-ups

Open teardown, race, transcript-mapper, chat-wire and trust-entry items from the bridge plan, the unverified operator e2e, and a recurring flock-after-drop proposal. The background-task and slash-command work added open items (native table pinned to Claude Code 2.1.321, an agent that never stalls, test gaps, web nits, the sandbox journeys). → [Planning Session Sandbox Limits](concerns/planning-session-sandbox-limits.md#planning-session-bridge-follow-ups)

## Plan Chat Follow-ups

The chat plan left the live-session operator test unrun, a SHA-256 ref gap and git edge cases in the review server, reconnect, draft and layout-state defects in the chat model, accessibility notes, and a list of duplicated helpers and missing tests, each with its file. → [Plan Chat Follow-ups](concerns/plan-chat-followups.md) The planner UI repair left the Codex summary backend unrun live, raw server strings outside the Untrusted rule, duplicated helpers and unshot layout details. → [Planner UI Repair Follow-ups](concerns/planner-ui-repair-followups.md) The planner redesign left the pressure mark split between the spine and the Rust step mark, owner-watch limits (one start, one kill attempt, children not terminated), chat context-attach edges, journey-script and bundle-rule gaps, and tracked-bundle and sealed-summary rebuilds, in the same file under "Planner redesign follow-ups". Claude Code's TMPDIR is read-only in sandboxed sessions, which breaks Bash exit codes and relayed loom commands. → [Session Deny on TMPDIR](concerns/session-tmpdir-deny.md)

## macOS Window Teardown Closes by Title and Is Unverified (GitHub #7)

Teardown closes an iTerm2 or Terminal.app window by its title. The iTerm2 spawn arm names the window and spawn and teardown address the same application name, but no macOS run of the close is on record, and Terminal.app's displayed title may not match.

Full detail: [iterm2-window-teardown.md](concerns/iterm2-window-teardown.md).

## Planner Review Fixes Follow-ups

Operator checks no stage sandbox can run (rebuilt summaries, a real image read, a kickoff round trip, journeys J2-J5 and J8), one open question about server-written sends returned on end, and about forty unimplemented reviewer suggestions on uploads, drafts, summaries and the web screens. → [Planner UI Repair Follow-ups](concerns/planner-ui-repair-followups.md#planner-review-fixes-unchecked-here-and-reviewer-suggestions)

## Planner Polish Follow-ups

Operator checks no stage sandbox can run (a rebuilt metaplan summary, a real Stop, journeys J3-J5 and J8) and about twenty unimplemented reviewer suggestions on chat tests, the Stop button look, roundel CSS, spine row accessibility and metaplan breadcrumbs. → [Planner UI Repair Follow-ups](concerns/planner-ui-repair-followups.md#planner-polish-unchecked-here-and-reviewer-suggestions) Planner feedback fixes: the scope gap of the spine heads (Home cards, the queue and the progress strip read only the server step mark), open reviewer suggestions on bulk settle, `/api/built`, the diff view and the chat ending, layout shift, the bundle budget. → [Planner UI Repair Follow-ups](concerns/planner-ui-repair-followups.md#home-cards-the-queue-and-the-progress-strip-read-only-the-server-step-mark)

## Planner Follow-ups Follow-ups

Open items from the plan-summary levels and fragment column, the three-theme palette, the Hearth chat (client `userWrote` mirror, closing block limits), the spine and run reviews, the requirement walk's "could not check here" lines and the operator checks no stage sandbox can run. → [Planner follow-ups open items](concerns/planner-follow-ups-followups.md)

## Trace Screens Follow-ups

Unapplied review suggestions on the trace screens (focus and status regions on the Coverage and Findings pages, popover overflow, comment and DESIGN.md drift, two missing tests), a server/browser mismatch on `(my call` inside code fences, four open questions (spine group counts, the chat greeting's face, mermaid chunk warnings) and two screens no stage sandbox can walk. → [Trace Screens Follow-ups](concerns/planner-follow-ups-followups.md)
