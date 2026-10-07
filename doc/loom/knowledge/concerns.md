# Concerns & Technical Debt

> Technical debt and open issues
> Every section here must be an OPEN concern. When one is resolved, DELETE it — do not strike the
> heading and leave the body, which is how eleven dead entries accumulated before 2026-08-26. Git
> history keeps the record and [mistakes.md](mistakes.md) keeps the lesson. If a resolved concern
> leaves a genuine residual, keep only the residual, under a plain heading.
>
> **Related files:** [mistakes.md](mistakes.md) for lessons learned, [architecture.md](architecture.md) for context.

## Architecture Concerns

### Layering Violations (2026-01-29)

Lower layers import from higher ones: daemon imports commands (`mark_plan_done_if_all_merged`), orchestrator
imports commands (`check_merge_state`), git/worktree imports orchestrator (hook config), models imports
plan/schema. Full details: [architecture.md § Review Findings - Layering Violations](architecture.md#review-findings---layering-violations-2026-01-29)

## Code Quality Concerns

### Oversized Rust Units Remain Controlled Debt (2026-08-09)

The maintainability gate does not yet prove every production Rust file is at most 400 lines or
every function at most 50 lines; exceptions are tracked by exact identity and size in
`loom/maintainability-baseline.txt` and pinned by `loom/tests/maintainability.rs`, which rejects
new entries, growth of an existing entry, or a stale one, so the exception set can only shrink.
This entry now also covers the wider code-quality/hook-debt cluster: duplicated
extension-to-language tables, hook debug logging to `/tmp/`, hook files over the 400-line cap, the
`subagent-verify-guard.sh` raw-regex gap, and undelivered remote hooks.

→ [Code Quality and Hook Debt](concerns/code-quality-and-hook-debt.md)

### Code Consolidation Needed

Duplications needing consolidation: `parse_stage_from_markdown` (4 copies), `branch_name_for_stage` (22+ inline
`format!()` calls), `extract_yaml_frontmatter` (2), `compute_level` (4, in status modules). Full details:
[conventions.md § Code Consolidation Opportunities](conventions.md#code-consolidation-opportunities-2026-01-29)

## `loom pressure` Known Gaps

### Vendored commands / Codex skill install LOCAL-only

`install.sh` installs `commands/*.md` (→ `~/.claude/commands/`) and `codex/skills/pressure/SKILL.md` (→ `~/.codex/skills/pressure/`) ONLY in the local (cloned-repo) branch — `install_commands`/`install_codex_skill` run under the `else` of `is_curl_pipe` in `main()` (~install.sh:619). The remote `curl | bash` install path does NOT ship the `loom pressure` slash commands or the Codex skill. A user who installs via curl-pipe and then runs `loom pressure` will be missing `/pressure`, `/address`, and the `$pressure` skill.

### `loom pressure` real-invocation smokes are manual-only

The two end-to-end smokes — Claude `/pressure` actually editing the plan, and Codex `$pressure` writing the `codex-` sidecar — need network + agent auth and are NOT exercised by `loom stage complete`. They are manual release-validation. Automated coverage is dry-run + 10 unit tests (argv, step order, exit classification, path resolution).

### `git rev-parse --show-toplevel` Duplication (2026-08-18, updated 2026-09-10)

Repo-root resolution is inlined in two places: the merge-side resolution at
`commands/stage/merge/preflight.rs` and the pressure-side one at
`commands/pressure/paths.rs::resolve_repo_root`. Both are below the "extract at 3+" threshold in
conventions.md Import Deduplication — not an active consolidation candidate, but the two copies
could still drift.

## Deferred Worktree Cleanup Has Two Residual Edge Cases (2026-07-22)

Two residuals in worktree cleanup: daemon cleanup can still race SessionEnd hooks during the short
SIGTERM teardown window, and manual `loom stage complete` with no daemon running defers cleanup
until the next recovery pass or an explicit `loom worktree remove`.

## Sandbox Denial Has No End-to-End CI Canary

Nothing proves sandbox denial holds against a live Claude Code runtime, and the srt stand-in misses
the git-dir grant and every read deny. Also: credential reads confined to five home paths, sibling
worktrees readable from Bash, the codex lane's whole `~/.codex` grant, an inert credential-guard
rule, three stage-env allowlists (sessions get no proxy, CA or `CLAUDE_CONFIG_DIR` names), and the `Read(...)` deny-rule ban, among others.
"Tool Routes That Run Outside the Bash Sandbox" lists the tools (LSP plugins, cross-session
messaging, `WebFetch`, `RemoteTrigger`) the OS sandbox does not wrap.

→ [Sandbox and Confinement Gaps](concerns/sandbox-and-confinement-gaps.md)

## Long Codex Runs Starve the Loom Heartbeat

A foreground codex-lane run is ONE blocking Bash call, so neither `PostToolUse` nor `SubagentStop`
can refresh the heartbeat until it returns. A stale heartbeat is acted on: a stage that had worked
is re-queued at 3x its budget (twice at most, then parked in `NeedsHumanReview`), and a stage whose
first tool call is a long foreground run counts as never worked and is parked at one budget.
Mitigation is doctrine (bound the task, set `subagent_timeout_secs`), not a monitor change; raising
the global timeout was considered and rejected. The same topic covers the independent `loom status`
"Stale" badge mismatch (two 300 s constants, one stage-aware, one not).

Full detail: [codex-heartbeat-starvation.md](concerns/codex-heartbeat-starvation.md).

## Potential Concerns

- **18 TODO comments** found in source files
- **7 FIXME comments** found in source files

## Automatic-Knowledge Plan Followups

Four open items: an unbounded whole-file read ahead of the extraction size cap,
three production-dead `KnowledgeDir` methods kept alive only by each other's tests,
a writer/reader plan-key normalisation mismatch, and a `LOOM_PERMISSIONS_WORKTREE`
constant with no consumer. The topic also covers natural-language queries that
corpus stopwording empties.

Full detail: [automatic-knowledge-source-graph-followups.md](concerns/automatic-knowledge-source-graph-followups.md).

## iTerm2 Windows Survive Stage Completion — Spawn Never Names the Window (GitHub #7, 2026-08-29)

Teardown closes an iTerm2 window by title, but the spawn arm never names the window, so the close
query matches nothing and only the `claude` process dies; the shell and window survive. A second
defect in the same path: teardown addresses `tell application "iTerm2"` while the scriptable name is
`iTerm`. Naming the window alone is not enough. Terminal.app is unverified (needs a macOS host).

Full detail: [iterm2-window-teardown.md](concerns/iterm2-window-teardown.md).

## Guard Hooks: Four Design Questions Deliberately Left Open (2026-08-30)

The adversarial review of the guard hooks left four behaviours unpatched pending a spec decision (the
deny threshold, ledger TSV atomicity, `loom_deny_enabled`'s line-oriented match, an unreachable
`poll-guard` branch). The entry also covers the wider runtime/session cluster: tmux-warning false
failures, live overview-attach panes, the unreaped viewer socket, path-swap races, `loom attach`'s
stage-bound lifetime, startup-only orphan adoption, `get_work_dir()`'s substring trust, and
process-global cwd mutation in memory tests.

→ [Runtime and Session Safety](concerns/runtime-and-session-safety.md)

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

## Merge Path Follow-Ups After the Silent-Unmerged Fix

The merge/recovery edge cases: `loom stage merge`'s worktree-cwd requirement, a git-error-as-unverified
revert path, the `BranchMissing` phantom-merge risk, the heuristic `BaseConflict` carve-out,
`retry --force` racing orphan-recovery, `started_at` not resetting on retry, the completion-broker's
post-transition nonce-burn ordering, the attribution code that has nothing left to attribute, the
editor residual between stash and pop, the `update-ref` window,
accumulating autostash backup refs, `human-review --force-complete` requiring no operator proof by design, a
newline inside a refused fast-forward path, and a loom gate error that parks a stage with no self-recovery (no
dispute covers loom's own gates, a deliberate block is never retried; proposed: a could-not-evaluate gate result).

→ [Merge and Recovery Edge Cases](concerns/merge-and-recovery-edge-cases.md)

## Token Accounting Follow-Ups (2026-09-13)

Open follow-ups from PLAN-token-optimization-2026-09-13: git and bounded-runner hygiene,
read-receipt runtime uncertainties, the IV fence wording, and two fail-open guard choices.

→ [Token Accounting Follow-Ups](concerns/token-accounting-and-proof-defects.md)

## State Confinement Gaps (2026-09-13) [DETAILED]

Shared package-manager caches stay session-writable, so a stage can edit an extracted crate's
`build.rs` that the operator's next host build runs.

→ [State Confinement Gaps](concerns/state-confinement-gaps.md)

## Agent Rule-Bending Hardening (2026-09-16)

Only the OS sandbox, the capsule denies and the daemon's ancestry checks carry authority. Seven
gaps, led by text-matched commits, the whole git common directory writable from a stage (Claude
Code's own grant; host git follows the agent's `.git` pointer; the merge gate fails open), and no
live proof that any denial holds. No plan addresses G1-G3 or G7's caches.

→ [Agent Rule-Bending Hardening](concerns/agent-rule-bending-hardening.md)

## Typed Config Values: Known Gaps (2026-09-16)

Accepted limitations and test-coverage gaps in the typed config read-path (TUI control-char stripping, no non-interactive unset, an untagged-enum wire limitation, a TUI test gap). See [Typed Config Values: Known Gaps](concerns/typed-config-values.md).

## Execution Graph: `mark_queued` Skips Node's Own Status (2026-09-18)

`ExecutionGraph::mark_queued` (`loom/src/plan/graph/mod.rs:247-285`) checks only the
file-declared stage's dependency list (each dep must be `Completed` and `merged`) before
unconditionally setting `node.status = StageStatus::Queued`; it never checks the node's own
current status first. A stale in-memory graph -- e.g. a daemon still running plan P5 after
`.loom/work/` was replaced by `loom init` for plan P6 -- can force a `Completed` node straight
back to `Queued` for any P6 stage that shares an id with a no-deps P5 stage. Not yet fixed;
candidate remedies: validate `node.status` is a legitimate pre-queued state before overwriting
it, or have callers refuse to sync when the file's declared deps disagree with the graph node's
own dependency list.

## Codex Forward Guard: macOS Stage-Evidence Branches Were Never Executed (2026-09-18)

`loom_stage_evidence` (`loom-hooks/_codex_forward.sh`) decides whether `codex-forward-guard.sh` enforces anything. Its Linux branches are covered by `loom-hooks/tests/codex-forward-guard-stage-evidence.sh` with real ancestry and a real `bwrap`. Its macOS branches were written and reviewed on Linux and have NOT been run. Background: [The forward guard engages only inside a loom stage](architecture/codex-plugin.md).

**To do on a Mac, before relying on the guard there:**

- Run `bash loom-hooks/tests/codex-forward-guard-stage-evidence.sh` and `bash loom-hooks/tests/run-all.sh`. The Seatbelt case (guard under `/usr/bin/sandbox-exec -p '(version 1)(allow default)'` with a clean env, expects exit 2 naming `sandbox confinement (seatbelt)`) only runs on Darwin; on Linux it prints `SKIP`.
- E2 parent walk: `_loom_parent_pid` parses `/bin/ps -o ppid= -p <pid>`. Confirm the value is a bare integer after the `read -r` trim under bash 3.2.
- E2 environment read: `_loom_pid_env_has_stage_var` matches `LOOM_STAGE_ID=`, `LOOM_SESSION_ID=`, `LOOM_WORK_DIR=` in `/bin/ps eww -o command= -p <pid>`. Confirm `ps eww` prints the environment of the user's own `claude`/node process on the macOS versions in use (SIP and hardened-runtime processes can hide it), and that the nested-session test (an ancestor with `LOOM_STAGE_ID`, guard run under `env -i`) exits 2.
- `/bin/ps` is setuid root and may be refused inside a Seatbelt profile. If it is, E2 yields nothing there and E3 must carry the case: check that a guard run inside the stage sandbox with a scrubbed env still exits 2 through the `sandbox-exec` refusal probe.
- E3 probe: confirm `/usr/bin/sandbox-exec` and `/usr/bin/true` exist and that the probe SUCCEEDS (no evidence) in an ordinary unsandboxed session, so the stock Codex plugin is usable there: a `codex:codex-rescue` call in a plain session must not be blocked.
- The Rust test `forward_guard_allows_when_no_stage_evidence_exists` (`loom/src/fs/permissions/hooks/policy_tests_stage_gate.rs`) skips only on LOOM_*in its own env or `/proc/1/comm == bwrap`. Under a macOS Seatbelt with no LOOM_* it would FAIL instead of skipping; add a Seatbelt skip if that combination occurs in practice.

**Known limits, all platforms:** a nested `claude` whose launcher controls its environment can set `BASH_ENV` or a config directory with no hooks, which no hook can police (sandbox policy's job); a plain session inside the user's own bubblewrap or Seatbelt wrapper counts as confined and stays blocked.

## Knowledge Bootstrap Follow-Ups (2026-09-22)

- **Guard duplication:** `commands/knowledge/bootstrap/mod.rs::guard_not_in_stage` inlines its own
  `LOOM_STAGE_ID` check instead of calling `commands::hook::target::non_empty_env`, because
  `commands/hook/mod.rs:14` declares `mod target;` private (`mistakes/visibility-and-reachability.md`).
  Widen the module (`pub(crate) mod target` or a re-export) to de-duplicate.
- **Dry-run argv quoting:** `knowledge bootstrap --dry-run` prints the `claude` argv with
  `argv.join(" ")` (no shell quoting), copied from `pressure::render_dry_run_step` whose output is
  pinned by tests. The multi-line `--append-system-prompt` value and the positional prompt run
  together, so the printed line cannot be pasted into a shell. A shared quoting helper would fix
  both call sites.

## Markdown Lint Blocks Only at Push

`loom/.githooks/pre-commit:63` runs `markdownlint-cli2 --fix` and ignores its exit status (it warns only when `bunx`
cannot run), so an error `--fix` cannot repair, such as a table row missing a cell (MD055/MD056), commits silently;
`pre-push:48` blocks it. Lint new markdown with `bunx markdownlint-cli2 <files>` before committing.

## Verification v2 Follow-Ups (2026-09-25)

Nine of the 23 test-runner adapters (cargo-nextest, gradle, maven, sbt, rspec, phpunit, pest, swift-test, mix-test) have
fixtures written from documented output, not captured runs, so their parsers are unproven. Adapter gaps, contract-phase
gaps, duplicated helpers, provision and environment-lint gaps, gate and dispute backlog: [verification-v2-followups](concerns/verification-v2-followups.md).

## Source Graph Limits and Review Backlog

The graph states what it cannot see: C/C++ macros and templates, reflection and dynamic dispatch, JavaScript
`#private` members, Ruby bare calls, inherited members, and Kotlin, Swift and shell (not supported). Open decisions:
the storage engine waits on warm `--timings` data, the agent-task comparison and `scripts/retrieval-ab` have not been
run, and the labelled corpora enter loom's own graph because `EXCLUDED_ROOTS` matches only a first path segment. About
150 small reviewer suggestions (resolver, extraction, view store, lease hardening, routing, evaluator, fixtures) are
grouped by area in the backlog.

→ [Source Graph Known Gaps](concerns/source-graph-known-gaps.md), [Source Graph Review Backlog](concerns/source-graph-review-backlog.md)

## Platform, Commit-Relay and Stall Gaps

Open items from the macOS, daemon-launch, daemon-owned-commit and stall-parking work: no macOS CI runner (BSD
behaviour is emulated by shims), no automated test of the production daemon re-exec (only the operator's
host smoke test covers it), sessions get no proxy, CA or `CLAUDE_CONFIG_DIR` variables, the commit relay's
tick cost and `MERGE_HEAD` ancestor gap, the park-time login probe on the tick thread, and mistakes that
recurred and want a check (the signal dropping `exit_code`, the watch exiting 6 after a hand-back).

→ [Platform and Commit Gaps](concerns/platform-and-commit-gaps.md)
