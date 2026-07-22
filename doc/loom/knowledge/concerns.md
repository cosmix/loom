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

This is a logically defensible design (commits exist, don't burn tokens redoing them), but the user-visible interaction is confusing. The "fix" — using `retry --force` a *second* time after acknowledging the handoff — is undocumented in the recovery flow.

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

## Daemon Singleton Not Enforced: Two `loom run` Processes Alive Concurrently (2026-05-13)

**Observed:** During the `autonomous-criteria-adjudication` plan's `integration-verify` stage, `loom status` (static) reported `○ daemon stopped` even though the orchestrator log (`.work/orchestrator.log`) was still being appended every ~5 seconds. `loom status --live` was still connected in another terminal. `ps -eo pid,etime,cmd | rg 'loom run'` revealed **two** daemon processes:

```text
  64657    11:19:57  loom run    # started ~06:30 UTC
1038911    01:39:24  loom run    # started ~16:11 UTC (lock mtime 16:13:18 UTC)
```

State files in `.work/`:

| File | State |
|------|-------|
| `orchestrator.sock` | **MISSING** |
| `orchestrator.pid` | **MISSING** |
| `orchestrator.lock` | Present, contains `1038911` (no newline), mtime 16:13:18 UTC |
| `orchestrator.log` | Actively growing; first dated entry is 16:13:18 UTC (matches lock mtime), no startup banner for the 06:30 daemon survives in the file |

`loom status` (static) thinks the daemon is down because it talks to `orchestrator.sock`, which no longer exists. `loom status --live` in another terminal was bound earlier and is still rendering stale state; new clients can't connect.

**Why this matters for the user-visible "stuck integration-verify" symptom:** With the IPC socket gone, the daemon is invisible to the operator. The stage status (`status: executing`, `started_at` 19h ago) looked frozen because no fresh updates were rendered via the static command, and the dashboard's TUI was reading a cache. Meanwhile the agent inside the container was genuinely stuck on a hung cargo test (separate concern), but the operator couldn't tell whether the daemon or the agent was at fault.

**Probable cause (best hypothesis):** A second `loom run` was invoked while the first was still alive — likely as an operator recovery action after the stage looked stalled. The startup path:

1. Rewrote `.work/orchestrator.lock` to the new PID (1038911) without verifying the old PID was actually dead, OR the lock-acquire path uses a non-blocking `flock` that succeeded because the old process had released its lock (e.g., on a SIGSTOP/SIGTSTP, or a dropped guard in a code path that doesn't re-acquire).
2. Bound a new socket at `.work/orchestrator.sock` — succeeded because either (a) the old socket file had been removed by a `loom stop` that failed to kill the process, or (b) `unlink + bind` is unconditional in the daemon startup path.
3. Did NOT find an existing PID file (or failed-soft on its presence) and did NOT signal/kill the old daemon.

Result: two competing daemons sharing the same `.work/` state, the older one inert or only partially functional, the newer one doing most of the work. The socket file went missing later (a third event we have no log evidence for — possibly `loom stop` was issued against the new daemon, removing the socket but leaving both processes alive because `loom stop` over a since-disconnected socket is a no-op or because both daemons trapped the signal and ignored it).

**What's needed:**

1. **`loom run` must enforce singleton invariant at startup.** Before claiming the lock or binding the socket, walk these in order: (a) read `orchestrator.pid` if present, (b) `kill -0 <pid>` to test liveness, (c) if alive AND its argv matches `loom run`, refuse to start with a clear `error: daemon already running (pid N)` message and exit non-zero. Do NOT delete state files in this path.
2. **PID file must be written on every successful startup and removed on clean shutdown.** Current state shows `orchestrator.pid` missing despite an active daemon — either it was never written, or it was deleted by a parallel/cleanup path. Both bugs deserve their own probe.
3. **Socket file existence and the daemon's aliveness should be reconciled by `loom status`.** When the static command can't connect to the socket but a `loom run` process matches in `ps`, report something more useful than "daemon stopped" — e.g., `daemon process N alive, socket missing — try 'loom repair'`.
4. **`loom repair` should detect duplicate daemons and offer to kill the older one** (preferring the one whose PID matches `orchestrator.lock`). Today `loom repair` doesn't appear to scan for this.
5. **Investigate whether the orchestrator-log file descriptor is held by both daemons.** Multiple writers to a single file with `O_APPEND` is benign per POSIX, but if either daemon does `truncate + write_at(0)` (i.e., overwrites with `O_TRUNC`), the other daemon's writes are silently lost. The log's first surviving line being timestamped to the newer daemon's startup suggests truncation happened.
6. **Suppress the `[Polling...]` TUI status line from the orchestrator-log file.** The log currently contains hundreds of these lines (visible interleaved with real WARN entries) — the TUI subscriber output is leaking into the daemon's stderr/stdout sink. Logs should only contain structured tracing output, not the TUI dashboard.

**Detection rules for future incidents:**

- `pgrep -af 'loom run'` returning more than one row is always wrong. Add a `loom repair` check.
- `loom status` reporting "daemon stopped" while `.work/orchestrator.log` is being actively appended to is always wrong — either the daemon is alive (bug: stale socket cleanup) or the log is being written by a stale child process (bug: orphaned background work).
- `orchestrator.pid` missing while any `loom run` process exists is always wrong.

**Where to look in code:**

- `daemon/server/lifecycle.rs` — daemonization, socket binding, PID file write. Check the order of: lock-acquire → PID-file-write → socket-bind. Each step must be atomic or roll back the previous on failure.
- `commands/run/mod.rs` — `loom run` entry point. Check whether it consults `orchestrator.pid` + `kill -0` before forking.
- `commands/stop.rs` — `loom stop` must ALWAYS kill the underlying process before deleting socket/pid. Verify there's no path where the socket is removed but the process survives.
- `commands/repair.rs` — extend with a "duplicate daemon" detector and a "socket-vs-process mismatch" detector.
- `daemon/server/core.rs` — confirm `unlink(socket_path)` before `bind` is guarded by a process-liveness check on the prior owner.

**Concrete evidence captured at time of writing:**

```text
$ ps -eo pid,etime,cmd | rg 'loom run'
  64657    11:19:57  loom run
1038911    01:39:24  loom run

$ cat .work/orchestrator.lock
1038911

$ ls .work/orchestrator.sock .work/orchestrator.pid
ls: .work/orchestrator.sock: No such file or directory
ls: .work/orchestrator.pid: No such file or directory

$ stat .work/orchestrator.log | rg Modify
Modify: 2026-05-13 20:50:35 +0300   # still growing every poll cycle

$ head -10 .work/orchestrator.log
Loaded base_branch from config: main
Warning: Failed to parse skill file ...
Warning: Failed to parse skill file ...
Warning: Failed to parse skill file ...
Orchestrator started, spawning ready stages...
[K2026-05-13T16:13:18.544430Z  WARN ... Recovering orphaned stage stage_id=integration-verify status=Blocked
2026-05-13T16:13:18.544458Z  WARN ... Failed to transition to NeedsHandoff during orphan recovery, bypassing
...
```

First dated log line is `2026-05-13T16:13:18.544430Z` — within 1s of the lock file's mtime. The 06:30 daemon's earlier log entries (10 hours of operation) are not present in this file; either the log was truncated at the second startup, or the first daemon was writing to a different sink (e.g., it had `eprintln!` redirected on stdout but the new daemon repointed the log fd).

## loom plan verify: Missing bypass-permissions Sandbox Validation (2026-05-14)

`loom plan verify` does not call `sandbox::config::validate_config`, so a plan with `sandbox.permission_mode=bypass-permissions` reports 0 errors from `plan verify` but fails at `loom init`. The validation exists in `commands/init/plan_setup.rs` and at spawn time, but not in the verify path (`commands/plan/verify.rs`).

**Recommended fix:** Thread `validate_config` into `commands/plan/verify.rs` so the same validation that blocks `loom init` is surfaced early at plan-authoring time.

## code_review Schema Field — TRULY DORMANT (2026-06-15)

## code_review Schema Field — Wired for Signal Generation Only (updated 2026-06-15)

`StageDefinition.code_review` (`plan/schema/types.rs:261`) is parsed by serde and now surfaced to integration-verify agent signals, but **still not stored on the Stage struct and not consumed by acceptance, completion, or goal-backward verification**.

**Current state (after PLAN-anti-slop-thoroughness):**

- `load_code_review_for_stage(stage_id, plan_path)` in `orchestrator/signals/generate.rs` reads `code_review` directly from the plan file (via `parse_plan()`) for IntegrationVerify spawns
- `render_review_dimensions()` emits a `## Review Dimensions` checkbox section in IV signals, honoring `require_all` (all-checkboxes vs any-checkbox framing)
- `plan/schema/mod.rs` re-exports `CodeReviewConfig` so generate.rs can import it
- `create_stage_from_definition()` (`commands/init/plan_setup.rs`) still does NOT copy code_review to Stage — Stage struct has no `code_review` field

**What's still needed to fully wire it:**

1. Add `code_review: Option<CodeReviewConfig>` to Stage struct (`models/stage/types.rs`)
2. Copy field in `create_stage_from_definition()` (`plan_setup.rs`)
3. Consider consuming during acceptance or completion (currently not enforced)

## PLAN-anti-slop-thoroughness: before_stage Wired vs. Plan Description

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
