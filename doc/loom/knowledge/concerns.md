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

### Release Asset Verification Gap

Only binary files are signature-verified via minisign. Non-binary release assets lack verification:

- CLAUDE.md.template
- agents.zip
- skills.zip

**Recommended:** Add SHA256 checksum verification for all release assets.

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

## container/mod.rs Exceeds 400-line Limit (2026-05-11, updated 2026-05-12)

`loom/src/orchestrator/terminal/container/mod.rs` is 1141 lines — 185% over the 400-line CLAUDE.md code size limit (grew from 661 → 975 → 1141 during mounts-hardening + this plan). Functional and all tests pass; refactor deferred.

**Recommended split:** Extract spawn logic into `spawn.rs`, mount construction into `mounts.rs` (build_mounts and helpers), env building into `env.rs`. The submodule files `fingerprint.rs`, `image.rs`, `lifecycle.rs`, `logs_capture.rs`, `network.rs`, `probe.rs`, `resources.rs`, `runtime.rs` are already appropriately sized.

## forward_credentials Default Is Empty (2026-05-11)

`loom init --backend container` writes `forward_credentials = []` to `.work/config.toml` (empty — no credentials forwarded by default). The plan spec suggested defaulting to `["claude"]` (mount `~/.claude/.credentials.json`). The current implementation is stricter (explicit opt-in) but requires manual operator action to authenticate Claude Code inside the container.

**Agent escalation gap (partially closed):** `.work/config.toml` is covered by the ro base mount — an agent running inside the container cannot modify `forward_credentials` for the next stage. Host-side editing by the operator is still possible (not a security concern — operator trust is assumed).

**Impact:** Container sessions without `"claude"` in `forward_credentials` cannot authenticate. Until there's a `loom container credentials add` command or the default changes, edit `.work/config.toml` manually on the host.

## Probe Network Mismatch: bridge vs loom-net-\<stage\> (2026-05-11)

The firewall enforcement probe (`container/probe.rs`) runs on the default bridge network. Production stage containers attach to `--network=loom-net-<stage-id>` (CNI/netavark). iptables rule injection behavior can differ between bridge networking and CNI-managed networks (especially on rootless Podman with slirp4netns). A probe that passes on bridge does not guarantee enforcement on the production network.

**Hardening path:** Pass the production network name to the probe container (`--network=loom-net-<fingerprint>` or a freshly-created ephemeral network matching the production config) and clean it up after the probe. Until then, `--allow-insecure-runtime` should be considered for rootless Podman environments.

## host_repo_root() Trusts Parent of .work Symlink (2026-05-11)

`host_repo_root()` in `commands/init/execute.rs` derives the host repo root by canonicalizing the parent of the `.work` symlink. There is no sanity check that the rw mount targets (e.g., `.worktrees/<stage-id>`) actually exist on the host before `docker|podman run` is invoked. A missing directory would cause the runtime to create it as a root-owned directory, silently defeating the rw overlay intent.

**Hardening path:** Verify that each rw overlay target exists (or create it with the correct mode) before constructing the run args in `build_mounts`.

## BaseConflict Carve-out is Heuristic (2026-04-27)

`attribute_main_repo_merge` carves out `loom/_base/*` merges with a heuristic on the current branch name and on `SessionType::BaseConflict` session metadata. If a base-merge ever runs from a non-`loom/_base/*` branch (manual flow, future refactor) and no `BaseConflict` session is alive, attribution would tie the active merge to the stage whose branch HEAD shows up in `MERGE_HEAD` — leading to a spurious revert.

**Hardening path:** Tag base merges explicitly via session metadata (e.g., a marker file or distinct `SessionType::BaseConflict` always present during the base-merge window) and key the carve-out off that signal alone, not the current branch name. Until then, the heuristic is documented here so future work knows where to look.

## ~~Container Orphan Retry Collision~~ (RESOLVED 2026-05-12)

**Fixed in plan:** PLAN-container-backend-hardening (stage: fix-orphan-cleanup)

`preemptive_remove_existing` (best-effort `rm -f`) at the top of `spawn_common` now clears stale containers before each spawn. Failure rollback in `stage_executor.rs` calls `cleanup_worktree` + `cleanup_branch` + container `rm -f` for standard stages; container removal only for knowledge stages. See mistakes.md — Container Retry Collisions for prevention rules.

## ~~Container settings.local.json Path Leakage~~ (RESOLVED 2026-05-12)

**Fixed in plan:** PLAN-container-backend-hardening (stage: fix-container-hooks)

After worktree creation, `.claude/settings.local.json` is now appended to `<worktree>/.git/info/exclude`. Uses per-worktree exclude (not top-level `.gitignore`) to avoid polluting the repo. Knowledge stages write to the main repo's `.git/info/exclude`. See mistakes.md — Hook Installation Asymmetry and Per-Worktree Gitignore Exclusion for prevention rules.

## ~~Container Git Identity Gap~~ (RESOLVED 2026-05-12)

**Fixed in plan:** PLAN-container-backend-hardening (stage: fix-git-identity)

`git_user_name` and `git_user_email` fields added to `ProjectContainerConfig` (`plan/schema/execution.rs`). Populated at `loom init --backend container` time from host `git config --global`. Injected as all four `GIT_AUTHOR_*` / `GIT_COMMITTER_*` env vars (only when both are present). Validated against control chars and length via `validate_git_identity()` at init and read boundaries. See `README.md` § Container Backend — Git Identity for operator docs.

## container/mod.rs Size Update (2026-05-12)

`container/mod.rs` grew from ~975 lines to **1141 lines** during the current hardening work. Still 185% over the 400-line limit. Refactor deferred.

## Container Spawn-Failure Rollback: Zero Integration Test Coverage (2026-05-12)

The failure rollback chain in `orchestrator/core/stage_executor.rs` — `preemptive_remove_existing` → `remove_container_on_failure` + `git::remove_worktree` + `git::delete_branch` + `try_mark_blocked` — has no direct integration test coverage. The retry-after-failure scenario (container spawn fails, stage retries cleanly) is unverified end-to-end.

**Mitigations in place:**

- `preemptive_remove_existing_is_infallible` unit test verifies the rm -f preamble contract
- `smoke_rm_f_missing_container_exits_zero` (in `tests/container_smoke.rs`) validates podman's exit-0 contract on missing containers
- Wiring check confirms `remove_worktree|delete_branch` patterns exist in `stage_executor.rs`

**Gap:** No test injects a failing `TerminalBackend::spawn_session` and asserts that each rollback helper was called in sequence. To add: a unit-test seam that wraps `TerminalBackend` with a failing stub and verifies the rollback sequence.

## Deferred: Context Velocity

The heartbeat JSON written by `post-tool-use.sh` always records `"context_percent": null`. Context velocity tracking (how fast the agent is consuming context budget) was listed as a planned metric but deferred because extracting context percentage requires parsing the stream-json JSONL output of the Claude process, which the `post-tool-use` hook does not currently do.

**Current state:** `context_percent` field exists in the heartbeat JSON schema but is always `null`. The monitor reads it but never observes a non-null value through the hook path.

**What's needed:** Stream-json events (specifically `"type":"system"` with a `usage` subkey, or similar) need to be parsed from the container's stdout to extract token counts. A separate sidecar process (or stdout tap in the container entrypoint) would be the cleanest approach without modifying the hook flow.

**Where to look when implementing:**

- `hooks/post-tool-use.sh` — heartbeat writer (add context_percent extraction here)
- `orchestrator/monitor/context.rs` — context health thresholds (Green/Yellow/Red)
- `orchestrator/monitor/detection.rs` — where heartbeat data is consumed
- Stream-json `"system"` event shape: `{"type":"system","subtype":"init","session_id":"...","usage":{"input_tokens":N,...}}`

## Container Spawn: Fragile dependence on host worktree `.claude/settings.local.json` (2026-05-13)

**Observed during:** `autonomous-criteria-adjudication` plan, `integration-verify` stage. After three sandboxed crashes (cargo PATH issue, since fixed) and `loom stage retry --force`, the daemon refused to spawn with:

```text
podman run failed: Error: statfs /home/dkaponis/src/loom/.worktrees/integration-verify/.claude/settings.local.json: no such file or directory
```

The worktree existed and was tracked by git, but its `.claude/` directory was gone. `setup_worktree_hooks` is supposed to recreate `.claude/settings.local.json` on every spawn (`orchestrator/core/stage_executor.rs:340`), and `sandbox::write_settings` also creates it (`sandbox/settings.rs:80`). Neither produced a warning in `.work/orchestrator.log`, yet the file did not exist when `build_mounts` ran.

The mount-build defends with `if settings_local_host.exists()` (`orchestrator/terminal/container/mod.rs:588`), so a missing file should skip the mount — but the mount got pushed anyway and podman saw it. Either:

1. `setup_worktree_hooks` *appeared* to succeed but didn't write the file (silent no-op somewhere in the merge/write path).
2. The file existed at the moment `build_mounts` ran and was deleted between mount-build and `podman run` (TOCTOU).
3. The `exists()` check on line 588 has a subtle false-positive (e.g. a symlink to a deleted target — `Path::exists()` does follow symlinks, so a dangling one returns false, but a broken symlink whose target was deleted *after* canonicalize may evaluate inconsistently).

**Empirical workaround:** Manually creating `<worktree>/.claude/settings.local.json` with `{}` content unblocked the spawn. On the *next* spawn after that, `setup_worktree_hooks` correctly regenerated a real 5KB file. So once the directory exists, the regeneration path works — the bug is in the *first*-spawn-after-corruption case.

**What's needed:**

- Don't trust `exists()` at mount-build time. Either (a) make `setup_worktree_hooks` mandatory + fail-fast if its write didn't land, or (b) have `build_mounts` invoke (or assert) the hook-setup pipeline against its own input invariants before adding the mount.
- Surface hook-setup warnings from `stage_executor.rs:349` somewhere more visible than `eprintln!` to a noisy daemon log — they're load-bearing for container spawn, despite the comment "hooks are optional enhancement". For container backend they are not optional.
- Add an end-to-end test that deletes `<worktree>/.claude/` and runs a spawn, asserting the next attempt either regenerates the file *or* fails with a clear error pointing at the hook-setup step (not a podman statfs error).

**Where to look:**

- `orchestrator/core/stage_executor.rs:304-352` (sandbox + hook setup order)
- `orchestrator/terminal/container/mod.rs:551-590` (mount build with `exists()` guard)
- `hooks/generator.rs:226` (`setup_hooks_for_worktree`)
- `sandbox/settings.rs:72` (`write_settings`)

## Container Spawn: `loom repair` blind to missing worktree `.claude/` (2026-05-13)

**Observed:** With `<worktree>/.claude/settings.local.json` missing — the exact precondition that crashes container spawn (see preceding entry) — `loom repair` reports "No issues found - workspace is healthy!" The user lost time investigating because the supposedly authoritative health check said nothing was wrong.

**What's needed:** `loom repair` should walk every active worktree owned by a non-terminal stage with `BackendType::Container` and verify `.claude/settings.local.json` is present + parseable JSON. The `--fix` mode should call the same `setup_worktree_hooks` path the spawner uses, not roll its own template, so the two stay in sync. Same check for the per-session container-main-settings overlay for non-worktree stages.

**Where to look:**

- `commands/repair.rs` (or wherever the repair checks live — grep `pub fn repair` / "No issues found")
- Cross-check against `orchestrator/terminal/container/mod.rs::build_mounts` so the repair scan tracks the actual mount preconditions.

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

## Worktree-File-Guard Defeats Bash Background-Mode Output Capture (2026-05-13)

**Observed:** Inside a container-backed integration-verify session, the agent ran `cargo test 2>&1 | tail -100` via Claude Code's Bash background mode. Claude Code writes background-task output to `/tmp/claude-1000/<repo-tag>/<uuid>/tasks/<task-id>.output`. The agent then tried to `Read` that file (with the Read tool) to inspect progress. Every `Read` returned `✗ blocked by hook: Read hook error: [/home/loom/.claude/hooks/loom/worktree-file-guard.sh]:` and an empty body. The agent fell back to a `while [ $wait_count -lt 20 ]; do ... sleep 15; done` shell loop polling the same file with `stat -c%s`. That bash trick worked (no Read tool, no hook), but `cargo test 2>&1 | tail -100` only writes to the output file when `tail` finishes, so the file stayed at 0 bytes the whole time and the loop completed 5 minutes later with nothing useful.

Net effect: the agent burned ~5 minutes of context blind to a real hung test, and then started another `cargo test` that hung the same way. Three crash cycles in a row originated from this interaction.

**Why:** `worktree-file-guard.sh` is a `Read` PreToolUse hook installed for every container-backed worktree. It correctly blocks reads of anything outside the worktree path. Claude Code's Bash background mode writes its task buffer to `$TMPDIR` (= `/tmp/claude-<uid>/`), which is intentionally outside the worktree. The two policies are individually correct but produce a dead zone: background-mode output is unreadable by the agent that started it.

**What's needed:**

- Either (a) teach the agent (via signal text or skill) to NOT use Bash background mode in container-backed sessions — use foreground Bash with capped output instead, or (b) carve out `/tmp/claude-*/tasks/*.output` in `worktree-file-guard.sh` as an exception, or (c) configure Claude Code to write its background task buffers under the worktree (e.g., via `$CLAUDE_TASKS_DIR` if such a knob exists).
- Surface this in the standard-stage signal prefix so agents working inside container worktrees know foreground-only is mandatory. Currently the agent has no way to detect why its `Read` was blocked except the bare hook error.
- A `loom repair` check could detect agents stuck in this pattern by reading `.work/tool-events.jsonl` for sequences of `Read` errors against `/tmp/claude-*/tasks/*.output`.

**Where to look:**

- `hooks/validators/worktree-file-guard.sh` (or wherever the hook lives — grep for "worktree-file-guard")
- `orchestrator/signals/cache.rs` (standard-stage prefix; that's where the foreground-only rule belongs if we go path (a))
- `tests/container_smoke.rs` could grow a regression test that runs Bash background mode inside a container and asserts the output is reachable (or that the agent gets a clear "use foreground" hint).

## Container PID 1 (Claude Code) Does Not Reap Zombies (2026-05-13)

**Observed:** Inside `loom-integration-verify`, after ~13 minutes of `cargo test` activity, `ps -ef` showed several hundred `[git] <defunct>` zombies parented to PID 1 (Claude Code). The test suite forks `git` extensively (verify/baseline, plan parsing) and Claude doesn't `wait()` on subprocesses it doesn't own, so they accumulate.

```text
loom       1       0  2 17:28 ?  claude --print --output-format stream-json ...
loom   21195       1  0 17:35 ?  [git] <defunct>
loom   21196       1  0 17:35 ?  [git] <defunct>
...   (~200 more)
```

**Why this is a real concern (not cosmetic):**

- Linux's default `pid_max` is 4 million but kernels often gate on per-uid limits. A long-running container with a non-reaping PID 1 will eventually exhaust the table.
- Several test paths likely call `git` synchronously; if their `Child::wait()` is dropped (e.g., spawned and forgotten) the zombies pile up.
- `podman` does not inject `tini` by default. The container entrypoint (`loom/resources/entrypoint.sh`) is `firewall.sh` → `resolver_loop` (bash) → exec target. None of these reap.

**What's needed:**

- Image-level fix: add `tini` (or `dumb-init`) to the Dockerfile and make it the literal PID 1, then exec the agent under it. `tini -- claude --print ...`. The image fingerprint already covers Dockerfile.tmpl content, so this is a clean change.
- Alternative without an init: ensure every `Child` in test code is `.wait()`ed (lint with clippy's `let_underscore_must_use`).
- A monitor probe inside the container that counts `Z` state processes and emits a soft signal if > 50.

**Where to look:**

- `loom/resources/Dockerfile.tmpl` (add `tini` install + ENTRYPOINT)
- `loom/resources/entrypoint.sh` (wrap the resolver_loop under `exec tini --`)
- `verify/baseline/capture.rs`, `verify/baseline/compare.rs` (frequent `git` callers in test paths)
