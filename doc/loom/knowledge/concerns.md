# Concerns & Technical Debt

> Technical debt, warnings, issues, and improvements needed.
> This file is append-only - agents add discoveries, never delete.
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

## Security Concerns

### Release Checksum Asset-Name Mismatch (corrected 2026-07-01)

> An earlier note here claimed `agents.zip`/`skills.zip`/`CLAUDE.md.template` "lack verification." That is STALE and WRONG — corrected below.

Self-update DOES SHA256-verify all three non-binary assets via `download_verify_and_extract_zip` (loom/src/commands/self_update/mod.rs:277-340) and `verify_checksum` (signature.rs:77), and it REFUSES to install any asset that has no checksum entry.

The real defect is an **asset-name mismatch**: self-update fetches the digests from a release asset literally named `checksums.txt` (mod.rs:224), but the release workflow publishes them as `SHA256SUMS.txt` (.github/workflows/release.yml:148,161,240). At runtime self-update therefore bails with "Release is missing checksums.txt" and cannot update these assets at all.

**Fix:** reconcile the names — rename the published asset to `checksums.txt`, or have self-update look for `SHA256SUMS.txt`.

## Code Quality Concerns

### Oversized Rust Units Remain Controlled Debt (2026-08-09)

The maintainability gate does not yet prove that every production Rust file is at most 400 lines or
every function at most 50 lines. Existing violations are recorded by exact identity and size in
`loom/maintainability-baseline.txt`; `loom/tests/maintainability.rs` rejects new entries, growth of an
existing entry, or a stale baseline entry. CI runs that gate, so the exception set can only stay flat
or shrink. Treat this as controlled decomposition debt, not as completed decomposition, and remove
an entry whenever its unit is brought under the limit.

### Code Consolidation Needed

> **Full details:** See [conventions.md § Code Consolidation Opportunities](conventions.md#code-consolidation-opportunities-2026-01-29)

Key duplications needing consolidation:

- parse_stage_from_markdown: 4 copies
- branch_name_for_stage: 22+ inline format!() calls
- extract_yaml_frontmatter: 2 copies
- compute_level: 4 copies in status modules

### Debug Output in Production

`eprintln!` statements with 'Debug:' prefix in production code (complete.rs, orchestrator.rs). Should use tracing crate with log levels.

## ReDoS Potential in Plan Pattern Regex

User-provided regex patterns in plan files (failure_patterns, wiring patterns) are compiled and executed without complexity checks. While mitigated by trust model (plan authors = trusted), consider adding regex timeout or complexity limits for defense in depth.

Files: src/verify/baseline/capture.rs:76-79, src/verify/baseline/compare.rs:155-158

## Bootstrap Settings Backup Risk

`bootstrap.rs:write_bootstrap_sandbox()` keeps the settings.local.json backup in memory only (`Option<String>`). If the process is killed between writing sandbox settings and restoring the original, user settings are permanently lost. Low probability since bootstrap is interactive, but a disk-based temp backup would be more robust.

## Bootstrap Tool Restriction Scope

`bootstrap.rs:57` uses `Bash(loom knowledge*)` which allows all knowledge subcommands (init, check, audit, show, list, gc) not just `update`. Most are read-only, but `gc` spawns Claude — so allowing it from inside another Claude session could cause recursion. The new `knowledge/gc.rs` and `knowledge/spawn.rs` already exclude `loom knowledge gc` from their own bash allowlist; `bootstrap`'s allowlist should be tightened to `Bash(loom knowledge update*)`, `Bash(loom knowledge replace-section*)`, `Bash(loom knowledge audit*)` for principle of least privilege.

## Hook Pattern Matching: False Positives on Embedded Content (2026-03-31)

All PreToolUse hooks (worktree-isolation.sh, commit-filter.sh, git-add-guard.sh,
prefer-modern-tools.sh) and Rust validators (bash.rs) matched patterns against
full bash command strings including heredoc bodies and -m/--message content.
Keywords in commit messages or string literals triggered false blocks.

Issue #13: git commit -m "Add .worktrees/ to .gitignore" blocked by
worktree-isolation.sh because .worktrees/ appeared in message text.

Fix: Introduced _common.sh with strip_embedded_content() that removes heredoc
bodies and message content before pattern matching. Rust parallel implementation
in validators/bash.rs. Also tightened commit-filter.sh attribution pattern to
require Co-Authored-By: header prefix instead of substring matching.

Hooks affected: worktree-isolation.sh, commit-filter.sh, git-add-guard.sh,
prefer-modern-tools.sh, validators/bash.rs.

## Hook Debug Logging to /tmp/ (2026-03-31)

Several hooks (worktree-isolation.sh, commit-filter.sh, prefer-modern-tools.sh) hardcode debug log paths to `/tmp/<name>-debug.log`. Under `set -euo pipefail`, if `/tmp/` is not writable (e.g., sandboxed environments), the hook script exits immediately with error. `git-add-guard.sh` already uses a gated `debug()` pattern that only writes when `GIT_ADD_GUARD_DEBUG=1` is set. Other hooks should adopt the same pattern.

## ~~Sandbox Test Failures in fs::permissions~~ (RESOLVED 2026-04-16)

Fixed by `install_loom_hooks_to(path)` in commit 8d2bf2e. Tests now use temp directories via `ensure_loom_permissions_to(repo_root, Some(&hooks_dir))` instead of writing to the real `~/.claude/` directory.

## Rust/Shell Heredoc Terminator Divergence

The Rust `strip_embedded_content` in `bash.rs:79` uses `line.trim() == marker` (tolerates indented terminators), while the shell version in `_common.sh:44` uses `$0 == marker` (exact line match). Both fail-safe but should be aligned for consistency.

## Codex Findings Fixed (2026-04-16)

The following Codex review findings from PLAN-fix-codex-findings are now resolved:

- **H-01**: worktree-file-guard.sh registered for Read, Glob, Grep (hooks.rs:87-112)
- **H-02**: Plan sandbox config threaded to OrchestratorConfig in both foreground and daemon paths
- **H-03**: Fail-closed error handling in load_stage — only reconstructs on file-not-found, not parse errors
- **H-04**: finalize_merge_resolution handles both MergeConflict and MergeBlocked
- **M-03**: Budget check decoupled from health bucket guard — runs every poll tick
- **M-04**: merge_resolved() and merge_retry() use resolve_target_branch() instead of default_branch()
- **M-07**: Daemon status categorizes NeedsHandoff/WaitingForInput as "executing" matching CLI

Additionally fixed during integration-verify:

- **is_manually_merged**: Updated to use resolve_target_branch() instead of default_branch(), added work_dir parameter to detect_worktree_status() and is_manually_merged() for config access

## BranchMissing Phantom-Merge Risk in merge_handler.rs (2026-04-16)

`handle_merge_session_completed` at line 97-103 treats `MergeState::BranchMissing` as a successful merge by calling `finalize_merge_resolution` which unconditionally sets `merged=true`. This violates the project invariant that daemon-side paths must never write `merged=true` without git ancestry verification.

Scenario: merge session dies, `check_merge_state` returns Conflict/Unknown, branch was deleted without being merged (e.g., manual `git branch -D`), code assumes "branch missing = cleaned up after merge."

Pre-existing issue, not introduced by the merge conflict session lifecycle fix. The `ProgressiveMergeResult::is_success()` method also still classifies `NoBranch` as success, inconsistent with `progressive_complete.rs` treating it as `Blocked`.

## Dead Code: is_knowledge_stage()

models/stage/methods.rs:443 defines is_knowledge_stage() but it is never called. All call sites use direct stage_type comparison. Contains fragile heuristic name matching that duplicates detect_stage_type() logic. Consider removing or consolidating with detect_stage_type and check_knowledge_recommendations.

## BaseConflict Carve-out is Heuristic (2026-04-27)

`attribute_main_repo_merge` carves out `loom/_base/*` merges with a heuristic on the current branch name and on `SessionType::BaseConflict` session metadata. If a base-merge ever runs from a non-`loom/_base/*` branch (manual flow, future refactor) and no `BaseConflict` session is alive, attribution would tie the active merge to the stage whose branch HEAD shows up in `MERGE_HEAD` — leading to a spurious revert.

**Hardening path:** Tag base merges explicitly via session metadata (e.g., a marker file or distinct `SessionType::BaseConflict` always present during the base-merge window) and key the carve-out off that signal alone, not the current branch name. Until then, the heuristic is documented here so future work knows where to look.

## Deferred: Context Velocity

The heartbeat JSON written by `post-tool-use.sh` always records `"context_percent": null`. Context velocity tracking (how fast the agent is consuming context budget) was listed as a planned metric but deferred because extracting context percentage requires parsing the stream-json JSONL output of the Claude process, which the `post-tool-use` hook does not currently do.

**Current state:** `context_percent` field exists in the heartbeat JSON schema but is always `null`. The monitor reads it but never observes a non-null value through the hook path.

**What's needed:** Stream-json events (specifically `"type":"system"` with a `usage` subkey, or similar) need to be parsed from the Claude process stdout to extract token counts. A separate sidecar process would be the cleanest approach without modifying the hook flow.

**Where to look when implementing:**

- `hooks/post-tool-use.sh` — heartbeat writer (add context_percent extraction here)
- `orchestrator/monitor/context.rs` — context health thresholds (Green/Yellow/Red)
- `orchestrator/monitor/detection.rs` — where heartbeat data is consumed
- Stream-json `"system"` event shape: `{"type":"system","subtype":"init","session_id":"...","usage":{"input_tokens":N,...}}`

## Recovery: `retry --force` races daemon orphan-recovery on existing worktree (2026-05-13)

**Observed:** `loom stage retry --force --context "..."` correctly set `integration-verify` to `Queued`, but on the next daemon poll the orphan-recovery routine in `orchestrator/core/recovery.rs:638-705` saw the (now-stale) session_id, found commits-ahead-of-base on the worktree branch, and immediately re-routed the stage to `NeedsHandoff` (commits_ahead path at `recovery.rs:668`). To the user, the stage looked stuck — they typed `retry`, it was ready for a second, then back to a handoff state with no agent activity.

This is a logically defensible design (commits exist, don't burn tokens redoing them), but the user-visible interaction is confusing. The "fix" — using `retry --force` a _second_ time after acknowledging the handoff — is undocumented in the recovery flow.

**What's needed (pick one or both):**

- `retry --force` should clear `stage.session` before saving, so subsequent orphan recovery doesn't treat the prior session as live and doesn't rerun its decision tree.
- Orphan-recovery should respect a recently-saved "retry intent" marker (e.g., a timestamp on the stage indicating user-driven retry within the last poll interval) and skip its commits-ahead reroute for those.

**Where to look:**

- `commands/stage/skip_retry.rs` (the `retry` command sets Queued at line 122 but leaves `stage.session` populated)
- `orchestrator/core/recovery.rs:633-707` (the orphan-recovery decision tree that re-routes to `NeedsHandoff`)

## Status Dashboard: `started_at` not refreshed on retry, stage appears "stale/orphaned" (2026-05-13)

**Observed:** After a successful `loom stage retry --force` that spawned a fresh session, the status dashboard rendered `integration-verify` as `19h4m · 🔄 · orphaned (stale)` for the duration of the new attempt. The number came from the original (long-dead) `started_at`; the new session was actually `Up About a minute` in podman and actively making tool calls.

**What's needed:** `stage_executor`'s spawn path (or `retry`) should reset `stage.started_at` to `Utc::now()` when a new session is created. The dashboard's "stale" heuristic should key off the new attempt, not the cumulative duration.

**Where to look:**

- `commands/stage/skip_retry.rs::retry` (where retry mutates stage fields)
- `orchestrator/core/stage_executor.rs:291-293` (`begin_attempt(Utc::now())` is already called here — confirm it's the only writer of `started_at` and that it's reached on retry).
- The "stale" indicator emitter — likely in `commands/graph/indicators.rs` or a dashboard renderer.

## ~~Daemon Singleton Not Enforced~~ (RESOLVED 2026-08-08)

The daemon now holds an authoritative stable-file `flock` for its full lifetime. Startup refuses a
second owner before socket/control-file mutation, and shutdown signals only a matching PID/start-time
identity. The original incident and its evidence remain preserved for regression context.

→ [Daemon Singleton Incident](concerns/daemon-singleton.md)

## loom plan verify: Missing bypass-permissions Sandbox Validation (2026-05-14)

`loom plan verify` does not call `sandbox::config::validate_config`, so a plan with `sandbox.permission_mode=bypass-permissions` reports 0 errors from `plan verify` but fails at `loom init`. The validation exists in `commands/init/plan_setup.rs` and at spawn time, but not in the verify path (`commands/plan/verify.rs`).

**Recommended fix:** Thread `validate_config` into `commands/plan/verify.rs` so the same validation that blocks `loom init` is surfaced early at plan-authoring time.

## ~~code_review Persistence Gap~~ (RESOLVED 2026-08-08)

`Stage` now persists `code_review`, and the single `Stage::from_definition` conversion copies it with all other execution and verification policy. Initialization delegates to that canonical conversion, preventing schema-to-runtime field drift.

## before_stage Already Wired — Plan PLAN-anti-slop-thoroughness Was Wrong

The plan described before_stage as "dormant / parsed-but-never-run". This was **INCORRECT** — `before_stage` was fully wired at `orchestrator/core/stage_executor.rs:219-256` BEFORE this plan ran.

**Impact:** Stage 3 Subagent 1's task to "wire before_stage" was a confirmed no-op. The wire-dormant-gates implementation agent verified this against the code before writing any code and skipped the task.

**Lesson (see mistakes.md):** Always verify "dead schema" or "dormant" claims against the actual execution paths (`rg "before_stage" loom/src/`) before accepting the plan description as authoritative. Plan descriptions can become stale relative to implementation.

## `loom pressure` Known Gaps

### Vendored commands / Codex skill install LOCAL-only

`install.sh` installs `commands/*.md` (→ `~/.claude/commands/`) and `codex/skills/pressure/SKILL.md` (→ `~/.codex/skills/pressure/`) ONLY in the local (cloned-repo) branch — `install_commands`/`install_codex_skill` run under the `else` of `is_curl_pipe` in `main()` (~install.sh:619). The remote `curl | bash` install path does NOT ship the `loom pressure` slash commands or the Codex skill. A user who installs via curl-pipe and then runs `loom pressure` will be missing `/pressure`, `/address`, and the `$pressure` skill.

### `loom pressure` real-invocation smokes are manual-only

The two end-to-end smokes — Claude `/pressure` actually editing the plan, and Codex `$pressure` writing the `codex-` sidecar — need network + agent auth and are NOT exercised by `loom stage complete`. They are manual release-validation. Automated coverage is dry-run + 10 unit tests (argv, step order, exit classification, path resolution).

### `git rev-parse --show-toplevel` duplicated 3×

Repo-root resolution is now inlined in three places: `commands/knowledge/spawn.rs` (`resolve_project_root`), `commands/stage/merge.rs` (inline), and `commands/pressure/mod.rs` (`resolve_repo_root`). conventions.md Import Deduplication says extract at 3+ — candidate for a shared `git::repo_root()` helper (deferred during the parallel plan to avoid cross-module merge conflicts).

## Stop hooks fail with `posix_spawn '/bin/sh'` ENOENT — worktree deleted under the live session (2026-07-22)

**Symptom:** Every successful worktree stage ends with two non-blocking Stop hook errors: `ENOENT: no such file or directory, posix_spawn '/bin/sh'` (once each for `commit-guard.sh` and `learning-validator.sh`).

**Root cause:** `loom stage complete` (run by the agent FROM INSIDE the worktree) calls `cleanup_after_merge` on the success path (`commands/stage/progressive_complete.rs:217`), which removes `.worktrees/<stage-id>/` while the Claude session standing in it is still alive. The session then finishes its turn; Claude Code (a Bun binary) spawns each Stop hook as `/bin/sh -c <cmd>` with an explicit `cwd` = the now-deleted worktree, and Bun's spawn with a nonexistent `cwd` fails with exactly this message (reproduced: `Bun.spawnSync({cmd:['/bin/sh','-c','…'], cwd:'/nonexistent'})` → `ENOENT: no such file or directory, posix_spawn '/bin/sh'`). `/bin/sh` itself exists — the ENOENT is for the working directory. The same spawn failure necessarily hits the SessionEnd hook (`session-end.sh`) and the PostToolUse hook for the final `loom stage complete` tool call, so those silently never run either.

**Impact:** Cosmetic noise on the success path (commit-guard has nothing to block once the stage is complete; learning-validator is advisory), but ALL post-completion hooks silently stop running — session-end.sh never writes its final handoff/cleanup, and the last heartbeat/tool-event updates are lost.

**Fix direction:** Worktree removal must not happen while the session whose cwd it is can still run hooks. Move `cleanup_after_merge` out of the agent-run CLI success path and into the daemon, after `kill_session` in `handle_stage_completed` (`orchestrator/core/completion_handler.rs:44`) — the daemon already owns session teardown there, and `stage_executor.rs:390` shows precedent for daemon-side `remove_worktree`. The daemon's `try_auto_merge` path needs the same audit.

**RESOLVED (2026-07-22):** Two-part fix. (1) `commands/stage/progressive_complete.rs::should_defer_cleanup(cwd, repo_root, stage_id)` — `complete_with_merge` now skips `cleanup_after_merge` when the process cwd is inside the worktree it would delete (cwd-based detection, NOT env vars, per the stale-`LOOM_STAGE_ID` lesson; unverifiable cwd fails toward defer). (2) `orchestrator/core/merge_handler.rs::cleanup_merged_stage_resources(stage_id, repo_root)` — `try_auto_merge`'s `stage.merged` short-circuit (reached from `handle_stage_completed` after `kill_session`, and from the recovery one-shot retry) now performs the deferred cleanup, gated on `needs_cleanup` and never mutating stage state. Residual (accepted): the daemon proceeds from `kill_session` to cleanup within the same tick, so SessionEnd hooks that run during SIGTERM teardown can still race the removal — a far smaller window than the old guaranteed Stop-hook failure. Manual `loom stage complete` with no daemon running and cwd inside the worktree defers cleanup that nothing will pick up until the next daemon start (`recovery.rs` retry path) or `loom worktree remove` — the CLI prints that hint.

## Knowledge Signals Never Teach Tier-2 (2026-07-28)

`orchestrator/signals/` generates `loom knowledge update <tier-1-file>` guidance, and the
knowledge-distill prefix mentions `loom knowledge index`, but **no prefix teaches the
`category/slug` tier-2 form**. Verified functionally: tier-2 works
(`loom knowledge update patterns/lock-ordering` creates the file and the index picks it up) — it
is simply never advertised to an orchestrated knowledge stage.

Consequence: the hierarchy grows only through the interactive `bootstrap`/`gc` paths, not during
`loom run`. Not a defect in what landed; follow-up stage material.

## `loom knowledge` Has No Delete-Section Verb (2026-07-28)

`update` appends and `replace-section` replaces, but nothing removes a section. Consolidating
several tier-1 sections into one tier-2 topic therefore cannot be completed with the CLI alone —
the migration in this plan replaced the lead section with a summary and had to strip the
remaining N-1 headings with an external script. A `loom knowledge drop-section` (or a
`replace-section --delete`) would close the gap.

## Tier-2 Topic Blurbs Cannot Be Set From the CLI (2026-07-28)

A new topic is seeded with a fixed scaffold — a title derived from the slug and the blurb
"Topic notes for the `<category>` knowledge area" — and user content is appended _after_ it.
`scan_topics` harvests the **first** `#` and `>` lines for the INDEX.md table, so the generic
seeded blurb always wins and every topic reads identically in the index unless the file is edited
afterwards. The index's Blurb column is its main routing signal, so this directly costs
navigability. Wanted: a `--blurb` flag, or have the scaffold defer to a leading `>` line in the
supplied content.

## Remote Install Silently Ships Zero Hooks (PRE-EXISTING, 2026-07-28)

`install.sh::install_hooks_remote` fetches each hook from `${GITHUB_RELEASES}/<name>`, but
`.github/workflows/release.yml` publishes **no hook assets**, and the fetch loop swallows
failures (`2>/dev/null`, no error check). The remote install path therefore installs no hooks
while reporting success — every hook shipped this way is dead on remote installs.

**Detection:** exit 0 from the remote installer is not evidence hooks landed; list
`~/.claude/hooks/loom/` after installing.

## `loom plan verify` False-Positive Cargo Warning for Subdirectory Crates (2026-07-28)

For this repo layout the verifier emits "Acceptance criterion uses cargo but Cargo.toml not found
at `<project root>`" — 15 times in one run — because it probes `<root>/Cargo.toml` while the crate
lives at `<root>/loom/Cargo.toml` and every criterion already passes
`--manifest-path loom/Cargo.toml`. 0 errors, 19 warnings, exit 0. Noise for any repo whose crate
is in a subdirectory. Fix: honour an explicit `--manifest-path` before warning.

## No-Verify Doctrine Block Carries Only a Rust Example (2026-07-28)

The doctrine block must stay byte-identical across the signal, the template, and the hook's
refusal message, so it carries a single scoped-command example — a `cargo` one. A blocked Python
or Go subagent is shown a Rust example. `hooks/subagent-verify-guard.sh` is at the 400-line cap
with no slack, so the fix is to append language-specific examples **after** the pinned block as
explicitly hook-local guidance, the same way the `BLOCKED:` framing line already sits outside it.

## Dead Configurability: `analyze_gc_metrics_with_promoted` (2026-07-28)

No caller passes a non-default `max_promoted_blocks`. Pre-existing and kept deliberately —
Engineering Discipline C says record pre-existing dead code rather than delete it as a drive-by.

## Generated INDEX.md Is Not markdownlint-Clean — Permanent Commit Churn (2026-07-28)

`fs/knowledge/index.rs::generate_index` emits each `### <category>` sub-heading of the Tier 2
section **immediately after the previous table row, with no blank line before it**, which
violates MD022 (headings surrounded by blank lines). The repo's pre-commit hook runs
`markdownlint-cli2 --fix`, which inserts the blank lines and re-stages the file.

The result is a loop: the generator writes non-compliant markdown → the pre-commit hook fixes it
→ the next `loom knowledge index` overwrites the fix → the following commit's hook fixes it
again. Every commit that touches the knowledge base shows a spurious `INDEX.md` diff.

Observed on this repository with 22 topics across 5 categories; harmless semantically, since
both forms render identically and the staleness check only looks for substrings.

**Fix:** emit `\n### {category}\n` — a blank line before each category sub-heading — in
`generate_index`, and add a regression test asserting the generated index is markdownlint-clean.
Until then, prefer the linted form: let the pre-commit hook be the last writer, and do not
re-run `loom knowledge index` after committing unless topics actually changed.

## Per-Stage Sandbox `Write(path)` Rules Are Inert (2026-07-31)

Claude Code's file permission check consults **only** `Edit(path)` rules. A `Write(path)` rule
parses, prints a startup warning, and is then ignored — so a `Write(**)` deny permits every write
it was written to block. The warning is easy to miss because it scrolls past during session
startup:

```text
Permission deny rule (.claude/settings.local.json): Write(**) is not matched by file
permission checks — only Edit(path) rules are. Use Edit(**) instead.
```

`sandbox/settings.rs` builds the sandbox for **every worktree stage session** and emits `Write(...)`
in both its allow and deny lists, so per-stage write restrictions likely do not enforce what they
claim. The repo's own committed `.claude/settings.json` carries `Write(.work/**)` for the same
reason and warns on every session start. The knowledge spawn path was fixed in
`commands/knowledge/spawn.rs` (2026-07-31); `sandbox/settings.rs` was left alone deliberately —
it means reworking the core stage sandbox model and updating many exact-string test assertions.

**Detection:** grep for `Write(` in generated settings. Any rule meant to gate file access must be
`Edit(...)`; `Edit` covers all file-editing tools, `Write` covers none of them.

**Caution when fixing:** deny beats allow, so a blanket `Edit(**)` deny cannot be paired with a
narrower `Edit(<dir>/**)` allow — the deny wins and blocks the directory the session needs. Scope
blanket denies to read-only modes only.

**Related follow-up (2026-08-10):** the same rework should carry a plan's sandbox
`filesystem.allow_write` through to `sandbox.filesystem.allowWrite`, which is the OS-enforced,
additive form that actually reaches a subprocess. The generator now emits that key (for the codex
lane's state dirs — `CODEX_SANDBOX_WRITE_PATHS`), so the mechanism is present and proven; plan
`allow_write` is simply not routed into it, and still lands only in the ignored `Write(...)` form.
Until it is, a plan author asking for subprocess write access gets silence, not an error.

## GC Flags Tier-1 Files for Section Extraction With No Oversized Sections (2026-07-31)

`analyze_gc_metrics` flags a tier-1 file whenever its **total** exceeds `DEFAULT_MAX_TIER1_LINES`
(250), independently of whether any individual section exceeds the section threshold. All six
tier-1 files here currently report `0 oversized sections` yet appear as extraction targets, and
the GC system prompt's first instruction is "Extract oversized tier-1 sections into tier-2 topic
files" — sections the analyzer itself says do not exist.

The agent is left to invent a split with no guidance on where the seams are, which is exactly the
condition under which a restructuring run drops content.

**Fix:** when a file is over budget but has no oversized section, say so in the prompt and ask for
a split proposal by topic cohesion instead of naming a section-extraction target that isn't there.

## Long Codex Runs Starve the Loom Heartbeat (2026-08-07)

A foreground codex-lane run (`loom-codex-forwarder`) is ONE Bash tool call that blocks until codex returns. The
session heartbeat (`.work/heartbeat/<stage-id>.json`) is refreshed by exactly two writers, both
shell hooks — `hooks/session-start.sh:61-72` (initial) and `hooks/post-tool-use.sh:66-91` (after
every tool use), registered at `loom/src/hooks/config.rs:47-48`. No Rust production code writes a
heartbeat (`write_heartbeat`, `monitor/heartbeat.rs:264`, has only test callers). PostToolUse cannot
fire until the Bash call returns, so a codex run longer than the stage's budget makes the daemon
print `appears hung` for a stage that is perfectly healthy.

Budget: `DEFAULT_HUNG_TIMEOUT_SECS = 300` (`monitor/heartbeat.rs:21`), overridable per stage with
`subagent_timeout_secs` → `Stage::effective_subagent_timeout_secs()` (`models/stage/methods.rs:107-110`),
resolved at `monitor/detection.rs:475-488`. `MonitorConfig::hung_timeout` (`monitor/config.rs:17,29`)
is only the fallback for a session whose stage cannot be resolved by id.

**`MonitorEvent::SessionHung` is ADVISORY ONLY.** One emit site (`monitor/detection.rs:505-511`),
one match arm (`orchestrator/core/event_handler.rs:187-209`) that is a `clear_status_line()` plus a
single `eprintln!` — the code carries the comment _"ADVISORY ONLY: nothing is killed and nothing is
retried."_ It warns ONCE per session (dedupe set `reported_hung_sessions`, `detection.rs:48`,
cleared on a fresh beat at `:456-457` and on `Healthy` at `:521`). Contrast the siblings that DO
act: `SessionCrashed` (`event_handler.rs:153`), `SessionNeedsHandoff` (kills + re-queues, `:110`),
`BudgetExceeded` (`:218`). Nothing kills, retries, or transitions a stage on SessionHung — the
warning is noise, not damage.

**Mitigation is doctrine, not a monitor change.** Keep each codex task bounded, and set
`subagent_timeout_secs` on stages that legitimately block for longer. CLAUDE.md Rule 6 now caps any
single bounded check at 300s and tells the orchestrator to re-arm rather than wait open-endedly.

**Deliberately OUT OF SCOPE: raising `MonitorConfig::hung_timeout`.** A global raise would blind the
monitor to genuinely dead sessions on every other stage in order to silence a cosmetic warning on
one lane, and the per-stage override already covers the real case. Do NOT "fix" this by editing the
default — it was considered and rejected as disproportionate.

## `loom status` "Stale" Badge Is Not Stage-Aware (2026-08-07)

Two independent 300s constants with different consumers:

- **detection** — `orchestrator::monitor::heartbeat::DEFAULT_HUNG_TIMEOUT_SECS`
  (`monitor/heartbeat.rs:21`), per-stage overridable via `subagent_timeout_secs`.
- **display** — `models::constants::STALENESS_THRESHOLD_SECS` (`models/constants.rs:37`), hardcoded
  at `commands/status/data/collector.rs:49` and `commands/status/render/activity.rs:21`.

`subagent_timeout_secs` reroutes only the detection one. A stage with `subagent_timeout_secs: 900`
stays healthy to the orchestrator until 900s but renders `Stale` / "session may be hung" in
`loom status` from 301s — which can push an operator into intervening on a healthy stage.

Flagged twice (implementation, then confirmed by integration-verify) and NOT fixed on purpose:
`determine_activity_status(session, staleness_secs)` takes no `Stage`, so making it stage-aware
means threading the effective timeout through that call site AND the render path — a status
subsystem change with its own test surface, unrelated to the codex lane. Fix if picked up later:
pass `Stage::effective_subagent_timeout_secs()` into `determine_activity_status` and the activity
renderer instead of the constant.

## ~~Orchestrator Control Subprocesses Were Unbounded~~ (RESOLVED 2026-08-08)

`mistakes.md`'s "Diagnostics: Restart Destroys the Evidence" records the 10-hour daemon-freeze
lesson: every external command issued from the poll loop must have a wall-clock deadline.

tmux spawn/probe/teardown operations now use `process::run_bounded_output` with 20s/5s/5s
deadlines. Native terminal detection and process discovery use 2s deadlines, while desktop
notifications use 2s (`notify-send`) or 3s (`osascript`). A timeout kills and reaps the child process
group and returns an error or an unavailable probe result instead of wedging the scheduler.

## ~~Sandbox Secret Denials Could Fail Open~~ (RESOLVED IN CODE 2026-08-08)

Generated settings now carry sensitive read restrictions into the host sandbox's `denyRead`, add
resolved `.work/admin.token` and `.work/user.token` paths, and set `failIfUnavailable: true` whenever
the sandbox is enabled. Stage startup treats a required settings-write failure as an infrastructure
block and does not spawn the session, so the configured boundary cannot silently disappear.

Unit and flow tests cover the generated policy and settings-write failure. A true Claude runtime
canary remains manual release validation: CI has no callable credentialed Claude sandbox runtime, so
it cannot prove Bash, interpreter, build-script, symlink, and file-tool denial end to end.

## ~~Repair Counted a Failed Skill-Index Rebuild as Fixed~~ (RESOLVED 2026-08-08)

Hook repair now propagates `skill_index::execute()` failure through `fix_hooks_with` instead of
discarding it. `hook_repair_propagates_skill_index_write_failure` supplies regression coverage for
the previously silent write-failure path.

## `evaluate_new_session` Fails a Working Spawn on a Benign `~/.tmux.conf` Warning (2026-08-08)

The rule "any stderr with exit 0 is a failure" is a plan mandate, pinned by unit tests and carried by
an explicit stage decision — so it was **deliberately left as-is**. But tmux prints `~/.tmux.conf`
deprecation warnings to stderr _while creating the session fine_, so one benign warning now: fails the
spawn, kills a **working** server via the abort path, and writes the sticky
`.work/terminal-backend-fallback` marker — disabling tmux for the whole repo until someone runs
`loom run --backend tmux`.

The `has-session` probe that immediately follows is the authoritative signal and would distinguish the
two cases. Gating the stderr rule on that probe is a design call for the plan owner, not an
integration-verify defect.

## ~~PID Recycling Could Route a SIGTERM to a Stranger~~ (RESOLVED 2026-08-09)

All backend and daemon signaling now routes through `process::ProcessIdentity` (PID plus kernel start
time). A start-time mismatch is definitive death, and missing start-time evidence is `Unverifiable`;
`terminate_verified` refuses destructive signaling in both cases. Native and tmux PID-file checks use
the same identity service, and daemon stop additionally requires the singleton lock to be held.

The durable rule is fail closed: never discard failed identity evidence and then re-probe the raw PID.

## `loom attach` Overview Panes Are Live, Writable Agent Terminals (2026-08-08)

The overview's panes are full interactive attach clients (no `-r`), so a stray keystroke goes into a
live autonomous agent's input, and `C-b x` kills the **stage's** pane — the inner servers have no
`remain-on-exit`, only the viewer window does.

Left as-is deliberately: the plan mandates the exact pane string. If the overview is meant to be a
_viewer_ rather than N live terminals, adding `-r` to `attach-session` on the **overview path only**
(never on `loom attach <stage-id>`, which is the intentionally interactive path) is a one-word change.

## Nothing Reaps the Overview Viewer Socket (2026-08-08)

`list_loom_sockets` skips `loom-view-<8hex>` by name so `clean`/`init` do not report it as
unattributable, and the skip comment concedes "Nothing currently reaps this socket". After
`loom attach` the viewer server and its nested clients outlive the operator's detach, and
`loom init --clean` now kills every attributed _session_ server while stranding the viewer.

It is the one socket whose name is a pure function of the repo root, so reaping it carries no
cross-checkout risk — roughly two lines in `cleanup_orphaned_sessions` and `clean::sessions` once
`viewer_socket_name` is crate-visible.

## ~~Dead `NativeBackend::spawn_base_conflict_session` Surface~~ (RESOLVED 2026-08-09)

The uncalled spawn method and `signals/base_conflict.rs` were removed. `SessionType::BaseConflict`
remains only where persisted historical records and main-repository merge attribution must still be
understood; it is no longer exposed as a production spawn route.

## `loom knowledge` Still Cannot Correct an Entry In Place (2026-08-08)

Concrete instance of the "No Delete-Section Verb" gap above: this plan found two tier-1/tier-2 sections
that had become factually **wrong** (a "~15-20 struct literal breakages" count that is now 3, and a
"Session Spawning Pattern" describing an `Arc<NativeBackend>` the orchestrator no longer holds).
Because `loom knowledge update` only appends, the distill stage could only append `CORRECTION (...)
supersedes ...` sections beneath them.

**Cost:** the wrong text stays in the file and still greps, so a future agent reading top-down hits the
stale claim first. An `update --replace-section <heading>` verb would let distillation actually retire
a stale entry instead of layering over it.

## PreToolUse File Guards Cannot Eliminate Path-Swap Races (2026-08-08)

The canonical `worktree-file-guard.sh` now covers Read, Write, Edit, Glob, and Grep; canonicalizes
paths; compares path components; and rejects both leaf and parent symlinks. This closes the concrete
absolute-path, common-prefix, and final-symlink escapes found in the security review.

The guard is still a check before a separate built-in tool performs the real open. A hostile process
can replace a parent path after the hook returns, so the guard cannot bind its decision to the inode
the tool later uses. Treat it as defense in depth; the host OS sandbox remains the authoritative
boundary. A race-free design requires a dedicated file-operation broker that performs traversal and
the actual open relative to an already-open worktree directory using no-follow semantics. Do not
describe the current hook boundary as TOCTOU-free.

## Completion Broker: Nonce Burn After Transition Can Return Err for a Landed Completion (2026-08-09)

`daemon/server/control_complete.rs::handle_complete_stage` consumes the replay nonce AFTER
`update_stage(...).try_complete(...)`. This ordering is deliberate: a daemon crash between the
transition and the burn is benign — a replay of the same nonce is rejected by
`validate_active_identity` because the stage is no longer `Executing`, so no completion can be
duplicated, and (unlike the old burn-first ordering) none can be lost to a pre-effect burn.

The residual edge introduced by the reorder: a genuine IO failure on the replay-marker directory
(disk full, permissions) occurring after the transition makes the handler return `Err` for a
completion that durably landed. Self-healing in practice — the stage file is `Completed` on disk
and the daemon reads state from disk — but the caller observes a false negative for a succeeded
operation. If this is ever observed in the wild, split the response so callers can distinguish
"completed, replay marker failed" from "not completed".

## Four Hooks Still Regex Raw Command Strings (2026-08-11)

`git-add-guard.sh` was converted to a token scan (see
`mistakes/shell-command-matchers.md`) because regexing the raw command string cannot tell an
argument's _value_ from an argument's _mention_. That bug class is not specific to it. These
still match against raw strings and share it:

| Hook                        | Shared helper used            |
| --------------------------- | ----------------------------- |
| `commit-filter.sh`          | `strip_embedded_content`      |
| `prefer-modern-tools.sh`    | `strip_embedded_content`      |
| `worktree-isolation.sh`     | `strip_embedded_content`      |
| `subagent-verify-guard.sh`  | `strip_embedded_content`      |

`strip_embedded_content` in `hooks/_common.sh` was deliberately left unchanged during that fix:
all four depend on its current contract, and it has a Rust twin at
`loom/src/hooks/validators/bash.rs:71` that would have to move in lockstep. Widening the fix was
out of scope for a two-bug repair, not a judgement that these are safe.

Expect both directions of the same defect in each: a false positive when a quoted argument merely
mentions a matched form, and quieter false negatives where a quote character sits exactly where
the pattern expects a boundary. The migration path is ready — `loom_tokenize_command` is in
`_common.sh`, is permissive by design, and is not coupled to git-add-guard. Convert a hook when
it next misfires, add its cases to `hooks/tests/`, and keep its old regex as the
unterminated-quote fallback, exactly as `git-add-guard.sh` does.

## `loom memory` Is Unusable Without an Initialised `.work` (2026-08-11)

`loom memory note` exits non-zero with `.work directory not found. Run 'loom init' first.`
(`commands/memory/handlers.rs:45`), and even past that gate the recording handlers require a
stage id from `--stage` or `LOOM_STAGE_ID` (e.g. `note()` at handlers.rs:64-66). Neither holds in
an interactive or ad-hoc session.

This collides head-on with doctrine: the mandatory subagent preamble orders every subagent to
record mistakes and decisions via `loom memory`, while auto-memory is prohibited whenever
`doc/loom/knowledge/` exists — which it does here. Agents are therefore ordered to record and
given no working way to do it, and the failure is silent from the orchestrator's point of view.
Three agents lost insights to this in a single session before it was noticed.

**Fixed in-tree 2026-08-11** (`commands/memory/handlers.rs`): the four recording commands
(`note`, `decision`, `change`, `question`) now create `<repo_root>/.work/memory/` when cwd is
inside a git repo, and default the stage to the sentinel `ad-hoc`; `query`/`list`/`show` degrade
to exit 0 without creating anything. Outside a git repo the original error stands, so `.work` is
never scattered into arbitrary directories — see
[`find_repo_root_from_cwd` Returns `Some(cwd)` Outside Any Repo](../mistakes.md) for the trap that
guard exists to dodge.

**Still true until the built binary is installed:** a `loom` on PATH from before this change keeps
the old behaviour. When delegating outside a loom run against an older binary, tell subagents
explicitly that `loom memory` will fail, that auto-memory is still forbidden, and that they must
return insights in their final report for the orchestrator to record by hand.
