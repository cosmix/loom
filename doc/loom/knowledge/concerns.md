# Concerns & Technical Debt

> Tech debt and open issues
> Every section here must be an OPEN concern. When one is resolved, DELETE it — do not strike the
> heading and leave the body, which is how eleven dead entries accumulated before 2026-08-26. Git
> history keeps the record and [mistakes.md](mistakes.md) keeps the lesson. If a resolved concern
> leaves a genuine residual, keep only the residual, under a plain heading.
>
> **Related files:** [mistakes.md](mistakes.md) for lessons learned, [architecture.md](architecture.md) for context.

## Architecture Concerns

### Layering Violations

Lower layers import from higher ones:

- daemon calls commands: `loom/src/daemon/server/client.rs:72` (`commands::stage::admin_proof::verify_and_consume_admin_proof`), `loom/src/daemon/server/status.rs:16` and `loom/src/daemon/protocol.rs:241` (`commands::status::data`)
- orchestrator calls commands: `loom/src/orchestrator/core/coherence.rs:41` (`commands::stage::complete::strip_privileged_env_for_runtime`), `loom/src/orchestrator/core/inbox_drain/merge_resolved.rs:53` and `loom/src/orchestrator/core/merge_handler/resolver_exit.rs:128` (`commands::stage::complete::verify_resolved_completion`)
- git imports orchestrator: `loom/src/git/cleanup/orphans.rs:10` (`orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle}`)
- models imports plan: `loom/src/models/dispute.rs:14`, `loom/src/models/stage/types.rs:6`, `loom/src/models/stage/checks.rs:8`, `loom/src/models/stage/methods.rs:5`

## Code Quality Concerns

### Oversized Rust Units Remain Controlled Debt

The maintainability gate does not yet prove every production Rust file is at most 400 lines or
every function at most 50 lines; exceptions are tracked by exact identity and size in
`loom/maintainability-baseline.txt` and pinned by `loom/tests/maintainability.rs`, which rejects
new entries, growth of an existing entry, or a stale one, so the exception set can only shrink.
This entry now also covers the wider code-quality/hook-debt cluster: duplicated
extension-to-language tables, hook files over the 400-line cap, the
`subagent-verify-guard.sh` raw-regex gap, and undelivered remote hooks.

→ [Code Quality and Hook Debt](concerns/code-quality-and-hook-debt.md)

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

Open items from the macOS, daemon-launch, daemon-owned-commit and stall-parking work: macOS CI gaps (hook
stdin reads unbounded on stock macOS, no `ulimit -u` cap on hook steps), no automated test of the production daemon re-exec (only the operator's
host smoke test covers it), sessions get no proxy, CA or `CLAUDE_CONFIG_DIR` variables, the commit relay's
tick cost and `MERGE_HEAD` ancestor gap, the park-time login probe on the tick thread, and mistakes that
recurred and want a check (the signal dropping `exit_code`, the watch exiting 6 after a hand-back).

→ [Platform and Commit Gaps](concerns/platform-and-commit-gaps.md)

## Token Accounting Follow-Ups

Open follow-ups from the token-accounting plans: git and bounded-runner hygiene, whether a failed forward fires PostToolUse, two fail-open guard choices, and the unimplemented reviewer suggestions of the delivery-truth and usage-evidence plans.

→ [Token Accounting Follow-Ups](concerns/token-accounting-and-proof-defects.md). Token flow limits, hardening and structure: [Token Flow Gaps](concerns/token-accounting-and-proof-defects.md#token-flow-gaps).

## Host-Verification Review Suggestions

Open and fixed reviewer findings on the host-verification code, one bullet per defect with its file:line. The open ones cover the apply and recovery path, criteria confinement, observer git discovery, and skill and reference text that still describes the session sandbox as the criteria environment.

→ [Host-Verification Review Suggestions](concerns/source-graph-review-backlog.md)

## Pressure and Bootstrap Gaps

`loom pressure` slash commands and the Codex skill ship only from a cloned-repo install (not `curl | bash`), and its
two real-invocation smokes are manual-only. The knowledge bootstrap command inlines a stage guard it could share and
prints a dry-run argv that cannot be pasted into a shell; repo-root resolution is duplicated in two places.

→ [Pressure and Bootstrap Gaps](concerns/automatic-knowledge-source-graph-followups.md)

## Execution Graph and Forward Guard Gaps

`ExecutionGraph::mark_queued` refuses a finished node, but recovery sync answers that refusal by forcing the node to
`WaitingForDeps`, so a stale in-memory graph can still reset a `Completed` node. The macOS branches of `loom_stage_evidence` (the Codex forward guard) were written on Linux and
never run; the topic lists the checks to do on a Mac.

→ [Execution Graph and Forward Guard Gaps](concerns/code-quality-and-hook-debt.md)

## Deferred Worktree Cleanup Has Two Residual Edge Cases

Two residuals in worktree cleanup: daemon cleanup can still race SessionEnd hooks during the short
SIGTERM teardown window, and manual `loom stage complete` with no daemon running defers cleanup
until the next recovery pass or an explicit `loom worktree remove`.

## iTerm2 Windows Survive Stage Completion — Spawn Never Names the Window (GitHub #7)

Teardown closes an iTerm2 window by title, but the spawn arm never names the window, so the close
query matches nothing and only the `claude` process dies; the shell and window survive. A second
defect in the same path: teardown addresses `tell application "iTerm2"` while the scriptable name is
`iTerm`. Naming the window alone is not enough. Terminal.app is unverified (needs a macOS host).

Full detail: [iterm2-window-teardown.md](concerns/iterm2-window-teardown.md).

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

Only the OS sandbox, the capsule denies and the daemon's ancestry checks carry authority. Eight
gaps, led by text-matched commits, the whole git common directory writable from a stage (Claude
Code's own grant; host git follows the agent's `.git` pointer; the merge gate fails open), no
live proof that any denial holds, and a knowledge-file guard that gates tool names over a directory
the sandbox grants to every stage (G8). No plan addresses G1-G3, G7's caches or G8.

→ [Agent Rule-Bending Hardening](concerns/agent-rule-bending-hardening.md)

## Typed Config Values: Known Gaps

Accepted limitations and test-coverage gaps in the typed config read-path (TUI control-char stripping, no non-interactive unset, an untagged-enum wire limitation, a TUI test gap). See [Typed Config Values: Known Gaps](concerns/typed-config-values.md).

## Verification v2 Follow-Ups

Nine of the 23 test-runner adapters (cargo-nextest, gradle, maven, sbt, rspec, phpunit, pest, swift-test, mix-test) have
fixtures written from documented output, not captured runs, so their parsers are unproven. Adapter gaps, contract-phase
gaps, duplicated helpers, provision and environment-lint gaps, gate and dispute backlog: [verification-v2-followups](concerns/verification-v2-followups.md).

## Merge Path and Recovery Gaps

The merge/recovery edge cases: `loom stage merge`'s worktree-cwd requirement, a git-error-as-unverified
revert path, the `BranchMissing` phantom-merge risk, the heuristic `BaseConflict` carve-out,
`retry --force` racing orphan-recovery, `started_at` not resetting on retry, the completion-broker's
post-transition nonce-burn ordering, the attribution code that has nothing left to attribute, the
editor residual between stash and pop, the `update-ref` window,
accumulating autostash backup refs, `human-review --force-complete` requiring no operator proof by design, a
newline inside a refused fast-forward path, and a loom gate error that parks a stage with no self-recovery (no
dispute covers loom's own gates, a deliberate block is never retried; proposed: a could-not-evaluate gate result).

→ [Merge and Recovery Edge Cases](concerns/merge-and-recovery-edge-cases.md)

## Sandbox Denial Is Unproven End to End

The capsule's denies are proven on the emitted capsule and, in CI, under bubblewrap (sibling
administrative directories, credential files); nothing in CI proves them against a live Claude Code
runtime, and the srt stand-in misses the git-dir grant. The manual release checklist
(`doc/sandbox-release-checklist.md`) is the only live check. Open: credential files outside the nine
listed (and a checkout-rooted session's file tools, which no hook checks against them), sibling
worktrees and the main checkout readable from Bash, refs and objects writable (ref denies
unmeasured), the residuals of the sibling-directory denies (unregistered or later-registered
worktrees, a rewritten `gitdir`), tools still allowed and unmeasured (`Monitor`, `EnterWorktree`,
`ExitWorktree` and others), unconfined `loom knowledge bootstrap`, the codex lane's whole `~/.codex`
grant, an inert credential-guard rule, three stage-env allowlists (sessions get no proxy, CA or
`CLAUDE_CONFIG_DIR` names), and the `Read(...)` deny-rule ban, among others.
"Tool Routes That Run Outside the Bash Sandbox" lists what the capsule denies (`LSP`,
`RemoteTrigger`, `CronCreate`, `PushNotification`, and `Workflow` except in an ultracode Stage
session) and what stays allowed (`WebFetch`, `WebSearch`, `SendMessage`, `ListAgents`, plugins).
The sibling-directory residuals, the other open route-closure items and the unimplemented reviewer
suggestions are in [Agent Rule-Bending Hardening](concerns/agent-rule-bending-hardening.md#sibling-directory-denies-residuals).

→ [Sandbox and Confinement Gaps](concerns/sandbox-and-confinement-gaps.md)

## Dependency Findings Follow-Ups

The plan 4 harness printed a stop-rule FAIL on all three recorded pairs (guard-core to guard-integration gains -444 and -2,221 tokens against a 12,190 cost); the operator overrode it and the verdict is printed, not asserted. Also open: spool id validation, render and path-normalization cost bounds, evidence-matching edges, test gaps, and the finalization-guard heredoc scan.

→ [Dependency Findings Follow-Ups](concerns/token-accounting-and-proof-defects.md#dependency-findings-follow-ups)
