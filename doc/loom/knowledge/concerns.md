# Concerns & Technical Debt

> Technical debt, warnings, issues, and improvements needed.
> Every section here must be an OPEN concern. When one is resolved, DELETE it — do not strike the
> heading and leave the body, which is how eleven dead entries accumulated before 2026-08-26. Git
> history keeps the record and [mistakes.md](mistakes.md) keeps the lesson. If a resolved concern
> leaves a genuine residual, keep only the residual, under a plain heading.
>
> **Related files:** [mistakes.md](mistakes.md) for lessons learned, [architecture.md](architecture.md) for context.

## Architecture Concerns

### Layering Violations (2026-01-29)

> **Full details:** See [architecture.md § Review Findings - Layering Violations](architecture.md#review-findings---layering-violations-2026-01-29)

Critical violations where lower layers import from higher layers:

- daemon imports commands (mark_plan_done_if_all_merged)
- orchestrator imports commands (check_merge_state)
- git/worktree imports orchestrator (hook config)
- models imports plan/schema (type definitions)

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

> **Full details:** See [conventions.md § Code Consolidation Opportunities](conventions.md#code-consolidation-opportunities-2026-01-29)

Key duplications needing consolidation:

- parse_stage_from_markdown: 4 copies
- branch_name_for_stage: 22+ inline format!() calls
- extract_yaml_frontmatter: 2 copies
- compute_level: 4 copies in status modules

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

The generated sandbox policy is covered by unit and flow tests, but nothing proves denial holds
against a live Claude runtime end to end — that verification is manual release validation. This
entry now also covers the wider sandbox/confinement cluster: two diverging stage-env allowlists,
confined commands still reaching a live credential bus, uncalled path-escape validators,
sandbox-widening fields needing no author acknowledgement, ReDoS-able plan regexes, the bootstrap
settings backup risk, and the `Read(...)` deny-rule ban.

→ [Sandbox and Confinement Gaps](concerns/sandbox-and-confinement-gaps.md)

## Long Codex Runs Starve the Loom Heartbeat (2026-08-07)

A foreground codex-lane run is ONE blocking Bash call, so neither `PostToolUse` nor
`SubagentStop` can refresh the heartbeat until it returns — a codex run longer than
the stage's hung-timeout still produces a spurious, advisory-only `appears hung`
warning. Mitigation is doctrine (bound the task, set `subagent_timeout_secs`),
not a monitor change — raising the global timeout was considered and rejected. The
same topic now also covers the independent `loom status` "Stale" badge mismatch
(two 300s constants, one stage-aware, one not).

Full detail: [codex-heartbeat-starvation.md](concerns/codex-heartbeat-starvation.md).

## Potential Concerns

- **18 TODO comments** found in source files
- **7 FIXME comments** found in source files

## Open After PLAN-automatic-knowledge-and-source-graph (2026-08-18)

Five smaller open items from this plan: an unbounded whole-file read ahead of the
extraction size cap, four production-dead `KnowledgeDir` methods kept alive only by
each other's tests, a writer/reader plan-key normalisation mismatch, a permission
deny that now reaches the `loom` binary's own child processes, and a fossilized
`LOOM_PERMISSIONS_WORKTREE` grant with no real consumers. The same topic now also
covers the retrieval-degradation ambiguity and natural-language stopwording.

Full detail: [automatic-knowledge-source-graph-followups.md](concerns/automatic-knowledge-source-graph-followups.md).

## iTerm2 Windows Survive Stage Completion — Spawn Never Names the Window (GitHub #7, 2026-08-29)

Teardown closes an iTerm2 window by title, but the iTerm2 spawn arm never names the
window (`git log -S 'set name of'` is empty), so the close query matches nothing and
falls through to killing just the `claude` process — the shell (and window) survive.
A second, independent defect in the same path: teardown addresses `tell application
"iTerm2"` while iTerm2's real scriptable name is `iTerm`. Naming the window alone is
necessary but not sufficient. Terminal.app is unverified (needs a macOS host).

Full detail: [iterm2-window-teardown.md](concerns/iterm2-window-teardown.md).

## Guard Hooks: Four Design Questions Deliberately Left Open (2026-08-30)

The adversarial review of the guard hooks left four behaviours deliberately unpatched pending a
spec decision (the deny threshold, ledger TSV atomicity, `loom_deny_enabled`'s line-oriented
match, an unreachable `poll-guard` branch). This entry now also covers the wider runtime/session
cluster: `evaluate_new_session`'s tmux-warning false failure, live overview-attach panes, the
unreaped viewer socket, path-swap races, `loom attach`'s stage-bound lifetime, daemon-startup-only
orphan adoption, `get_work_dir()`'s substring trust, `is_ancestor("1")`'s deterministic-false gap,
and process-global cwd mutation in memory tests.

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

## Merge Path Follow-Ups After the Silent-Unmerged Fix (2026-09-06)

Found while fixing the silent `Completed + !merged` outcome (`mistakes/phantom-merges.md`, last
entry): a probe-failure exemption from `MAX_MERGE_RESOLVER_ATTEMPTS`, `loom stage merge`'s
worktree-cwd requirement, a git-error-as-unverified revert path, and `merge_stage`'s
success-only branch restore. This entry now also covers the wider merge/recovery cluster: the
`BranchMissing` phantom-merge risk, the heuristic `BaseConflict` carve-out, `retry --force` racing
orphan-recovery, `started_at` not resetting on retry, and the completion-broker's
post-transition nonce-burn ordering.

→ [Merge and Recovery Edge Cases](concerns/merge-and-recovery-edge-cases.md)

## Token Accounting Follow-Ups (2026-09-13)

Open follow-ups from PLAN-token-optimization-2026-09-13: git and bounded-runner hygiene,
read-receipt runtime uncertainties, the IV fence wording, and two fail-open guard choices.

→ [Token Accounting Follow-Ups](concerns/token-accounting-and-proof-defects.md)

## State Confinement Gaps (2026-09-13) [DETAILED]

One accepted residual from the merged `.loom` state-confinement plan: shared package-manager
caches stay session-writable.

→ [State Confinement Gaps](concerns/state-confinement-gaps.md)

## `loom request status` Fails in Every Stage Worktree (2026-09-14)

`loom request status <id>` — the follow-up command every relay ticket's stderr tells the caller to
run (`loom/src/relay/emit/stderr_text.rs:17`) — fails inside every stage worktree with `Failed to
open dirfd at <worktree>/.loom/work: Not a directory`. `commands/request/status.rs:15` calls
`resolve_work_dir()` and anchors on it with `safe_fs::safe_open_dirfd`, which opens the root with
`O_NOFOLLOW` and by design refuses a symlinked root (`fs/safe_fs.rs:57-61`) — but a worktree's
`.loom/work` is ALWAYS a symlink to the main repo's state. This belongs to the session-relay feature
from earlier state-confinement work, not to any completion/recovery path; it was left unfixed rather
than folded into an unrelated plan's stage, since the fix touches a security-sensitive no-follow root
policy that deserves its own reviewed change. **Fix direction:** canonicalize the trusted work-dir
root once before anchoring (as the commands that already work correctly inside worktrees do), keeping
no-follow enforcement for everything beneath it; add a worktree-shaped regression test.

## Agent Rule-Bending Hardening (2026-09-16)

An env-var gate an agent can unset is class 1 of three enforcement classes; only the OS sandbox, the capsule deny layers and the daemon ancestry checks carry authority. Seven gaps follow, led by commits policed by text matching, an undenied worktree `.git` surface, and no CI proof that any denial holds.

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

## Markdown Lint Silently Skipped in a No-Network Stage Sandbox (2026-09-22)

The pre-commit hook runs `bunx markdownlint-cli2 --fix 2>/dev/null || true` (`loom/.githooks/pre-commit:63`); with no
network `bunx` cannot fetch transitive packages even for a cached tool, so `.md` files commit unlinted with exit 0
(seen in three stages, 2026-09-24). It should fail loudly; see `mistakes/verification-v2-delivery.md`.

## Verification v2 Follow-Ups (2026-09-25)

Nine of the 23 test-runner adapters (cargo-nextest, gradle, maven, sbt, rspec, phpunit, pest, swift-test, mix-test) have
fixtures written from documented output, not captured runs, so their parsers are unproven. Adapter gaps, contract-phase
gaps and duplicated helpers: [verification-v2-followups](concerns/verification-v2-followups.md).
