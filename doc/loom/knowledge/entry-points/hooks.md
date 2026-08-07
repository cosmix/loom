# Hooks

> Every hook script and the event it binds to, _common.sh's seven helpers, and the registration sites a new hook needs.

## Hooks

- `hooks/*.sh` - Shell scripts (commit-guard.sh, commit-filter.sh, etc.)
- `fs/permissions/hooks.rs` - install_loom_hooks()
- `fs/permissions/settings.rs` - ensure_loom_permissions(), create_worktree_settings()
- `fs/permissions/constants.rs` - Embedded hook scripts via include_str!()
- `orchestrator/hooks/config.rs` - HookEvent enum
- `orchestrator/hooks/generator.rs` - setup_hooks_for_worktree()

## Shared Hook Utility

- `hooks/_common.sh` - Source guard + `strip_embedded_content()` — sourced by all PreToolUse hooks. MUST be installed alongside hooks (in `~/.claude/hooks/loom/`). Registered in `constants.rs` as `HOOK_COMMON`.

## Hook System (loom/src/hooks/)

- `hooks/mod.rs` - Module root; re-exports `HookEvent`, `HooksConfig`, `generate_hooks_settings`, `setup_hooks_for_worktree`, `find_hooks_dir`
- `hooks/config.rs` - `HookEvent` enum (6 variants) + `HooksConfig` struct + `to_settings_hooks()`
- `hooks/generator.rs` - `generate_hooks_settings()` (merge session hooks into settings.json), `setup_hooks_for_worktree()`, `find_hooks_dir()`
- `hooks/events.rs` - `log_hook_event()`, `read_recent_events()`, event log CRUD
- `hooks/validators/` - Validator scripts for PreToolUse hooks (commit-filter, git-add-guard, worktree-isolation, prefer-modern-tools)

**6 hook events:**

| Event | Script | Purpose |
| --- | --- | --- |
| `SessionStart` | `session-start.sh` | Initial heartbeat |
| `PostToolUse` | `post-tool-use.sh` | Heartbeat update after every tool call |
| `PreCompact` | `pre-compact.sh` | Trigger handoff before context compaction |
| `SessionEnd` | `session-end.sh` | Cleanup on normal exit |
| `Stop` | `learning-validator.sh` | Memory usage check on stop |
| `PreferModernTools` | `prefer-modern-tools.sh` | Suggest fd/rg over find/grep in Bash |

**Settings placement:** Session hooks → `<worktree>/.claude/settings.local.json`. Global hooks (commit-filter, git-add-guard, worktree-isolation) configured via `fs/permissions.rs:configure_loom_hooks()`.

**Env vars injected via settings env block:**

- `LOOM_WORK_DIR` — path to `.work/` directory (the ONLY loom var persisted; stable per repo)

**Per-session identity (LOOM_MAIN_AGENT_PID, LOOM_STAGE_ID, LOOM_SESSION_ID):** Explicitly REMOVED from all settings env blocks (`scrub_session_identity_env` in `fs/permissions/settings.rs`). Set ONLY by the wrapper script exports so they always reflect the running session — settings env overrides process env, so persisted values from an earlier session would shadow the fresh exports (see mistakes.md 2026-07-22). Because Claude Code applies the MAIN repo's settings env to worktree sessions, the main-repo files are also healed in the run path: `scrub_main_repo_settings_identity` at `loom run` startup and inside the `sync.rs` fold-back (see mistakes.md 2026-07-23).

**Hooks discovery:** `find_hooks_dir()` checks `$LOOM_HOOKS_DIR` env first, then `~/.claude/hooks/loom/`. Returns `None` if not installed.

**Permissions:** Absolute paths use `//` prefix in allow entries (e.g., `Read(//home/user/.work/signals/**)`). Single `/` means project-relative — wrong for `.work/` which resolves outside the worktree due to symlink.

## Hook Scripts — What Each Does

| Script | Hook Type | Key Behavior |
|--------|-----------|-------------|
| `session-start.sh` | SessionStart | Writes initial heartbeat; captures stdin and parses `.source` field; on `source == "compact"` or `"resume"` emits `hookSpecificOutput.additionalContext` JSON re-anchor pointer |
| `post-tool-use.sh` | PostToolUse | Updates heartbeat; logs to `.work/tool-events.jsonl`; no longer checks compaction-recovery markers (removed) |
| `pre-compact.sh` | PreCompact | Block-then-allow: first call exits 2 (blocks) + creates pending flag + calls `loom handoff`; second call exits 0 (allows); does NOT create a recovery marker file |
| `session-end.sh` | SessionEnd | Creates handoff if stage not completed |
| `learning-validator.sh` | Stop | Advisory check for session memory usage |
| `commit-guard.sh` | Stop (global) | Blocks exit if uncommitted changes or stage still Executing |
| `prefer-modern-tools.sh` | PreToolUse:Bash | Emits `hookSpecificOutput.additionalContext` JSON warning to use `rg`/`fd` instead |
| `commit-filter.sh` | PreToolUse:Bash | Blocks subagent git commits via `loom_is_subagent()` process-tree check; blocks Claude attribution |
| `subagent-verify-guard.sh` | PreToolUse:Bash | Blocks **subagents** from running project-wide build/test/lint/typecheck suites; at most one narrowly-scoped check allowed; `integration-verify` stages carved out; unmatched commands are allowed (a false block strands a subagent); no opt-out env var |
| `git-add-guard.sh` | PreToolUse:Bash | Blocks the all-files staging forms and staging of `.work` |
| `worktree-isolation.sh` | PreToolUse:Bash/Edit/Write | Blocks cross-worktree ops and path traversal |
| `worktree-file-guard.sh` | PreToolUse:Read/Glob/Grep | Blocks file tool paths outside worktree |
| `plans-path-guard.sh` | PreToolUse:Edit/Write | **Unconditional** (fires in interactive sessions too) — blocks plan writes under `.claude/plans/` or `.claude/projects/*/plans/`, redirecting to `doc/plans/PLAN-*.md` |
| `codex-forward-guard.sh` | PreToolUse:Bash/Edit/Write/Read/Task/Agent | Pins codex forwarders to forwarding: when payload `agent_type` is `loom-codex-forwarder`/`codex:codex-rescue` (or, fallback, the subagent transcript carries `LOOM-CODEX-FORWARD-ONLY`), only a Bash call containing `codex-companion.mjs` is allowed. Fail-open for everyone else; does NOT use `loom_is_subagent` (false for in-process subagents) |
| `git-pre-commit-hook.sh` | git `pre-commit` | Blocks commits containing `.work` or `.worktrees`; appended to `.git/hooks/pre-commit` by `loom init`, not installed to `~/.claude/hooks/loom/` |
| `skill-trigger.sh` | UserPromptSubmit | Scores keywords, emits skill suggestions as `hookSpecificOutput.additionalContext` |
| `ask-user-pre.sh` | PreToolUse:AskUserQuestion | Marks stage WaitingForInput |
| `ask-user-post.sh` | PostToolUse:AskUserQuestion | Resumes stage |
| `_common.sh` | Utility (sourced, not registered) | Exports 7 helpers — see below |

### `hooks/_common.sh` Helpers

| Function | Role |
| --- | --- |
| `strip_embedded_content()` | Strips heredoc bodies and `-m`/`--message` text before matching, so a *mention* is not a match. **Known limit:** cannot strip a multi-line `-m` body |
| `loom_is_subagent()` | **The subagent gate.** True only when `LOOM_MAIN_AGENT_PID` is a *live ancestor* AND at least one Claude process sits between it and the caller. Returns false inside agent-team teammates (not in the main agent's tree) |
| `loom_current_worktree()` | Worktree detection by directory, NOT just the env var |
| `loom_debug()` | Gated debug logging |
| `is_ancestor()`, `find_nearest_claude_ancestor()`, `count_claude_processes_between()` | Internal helpers for `loom_is_subagent` — documented as internal; hooks should call `loom_is_subagent`, never these |

### Registration Sites for a New Hook

A hook missing from any of these is **silently dead**, not an error:

1. An `include_str!` const plus a `LOOM_HOOKS` entry in `fs/permissions/constants.rs`
2. The config builder `fs/permissions/hooks.rs::loom_hooks_config_for_dir` (session-lifecycle
   events are wired separately in `hooks/config.rs`)
3. **Both** `all_hooks` arrays in `install.sh` — there are two independent copies
4. Tests: `fs/permissions/tests/hooks_tests.rs::test_hooks_config_structure` asserts the exact
   `PreToolUse` array length and per-index order, plus a `hooks/tests/` case registered in
   `hooks/tests/run-all.sh`

**Worktree detection gotcha:** `_common.sh:loom_current_worktree()` checks TWO conditions — current directory contains `.worktrees/` AND `LOOM_WORKTREE_PATH` points into `.worktrees/` with the directory existing. LOOM_STAGE_ID alone is insufficient (it leaks into plain sessions from prior runs).
