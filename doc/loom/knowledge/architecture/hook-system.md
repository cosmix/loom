# Hook System

> Hook embedding and install, the SessionStart hookSpecificOutput contract, and the two subagent enforcement hooks.

## Hook System Architecture (hooks/)

The `hooks/` module provides Claude Code hooks integration for session lifecycle management. It is a **top-level module** — currently imported by `orchestrator/` and `git/worktree/`, which is a known layering violation (both should import a stable hooks interface instead).

**Layering:** `hooks/` is used by `orchestrator/core/stage_executor.rs` (worktree hook setup) and `git/worktree/settings.rs` (settings injection). The intended fix is to extract hooks as a fully independent top-level module with no reverse imports.

**Global vs session hooks distinction:**

- **Global hooks** include commit filtering, Git-add protection, Bash isolation, the canonical five-tool file guard, plan-path protection, and the forwarding guard. They are installed under `~/.claude/hooks/loom/` and registered by `fs/permissions/hooks.rs`, so they persist across sessions.
- **Session hooks** (session-start.sh, post-tool-use.sh, pre-compact.sh, session-end.sh, learning-validator.sh): generated fresh per-session by `hooks/generator.rs:generate_hooks_settings()`. Merged into worktree's `settings.local.json` with duplicate detection.

## Hook System — Session-Start Behavior and hookSpecificOutput Pattern

### session-start.sh Behavior (Updated 2026-06-15)

- Captures stdin into a variable (not drained) using cross-platform gtimeout/timeout/cat, 1s timeout
- Validates LOOM_STAGE_ID, LOOM_SESSION_ID, LOOM_WORK_DIR — silently exits if missing
- Writes initial heartbeat: `.work/heartbeat/<LOOM_STAGE_ID>.json`
- Logs SessionStart event to `.work/hooks/events.jsonl`
- **Parses `.source` field from stdin JSON**: when `.source == "compact"` or `"resume"`, emits `hookSpecificOutput.additionalContext` JSON with a re-anchor pointer (signal file path), redirecting the agent back to its signal after context compaction or resume
- Stdin must be captured (not drained with `>/dev/null`) so the source field can be parsed — same pattern as `post-tool-use.sh`

**Compaction recovery flow (current):**

```text
pre-compact.sh phase 1 → blocks compaction + creates handoff
pre-compact.sh phase 2 → allows compaction
Claude Code emits SessionStart with source="compact"
session-start.sh → parses source → emits hookSpecificOutput additionalContext re-anchor
```

### hookSpecificOutput JSON Pattern

Used by hooks to inject context into Claude's next turn:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "<EventType>",
    "additionalContext": "<string content>"
  }
}
```

**Examples:**

- `prefer-modern-tools.sh`: PreToolUse warning when a command-position token invokes `grep`/`find`
- `skill-trigger.sh`: UserPromptSubmit skill suggestions
- `session-start.sh`: SessionStart re-anchor pointer on compact/resume source

**Why JSON over plain text:** Claude Code has reliability issues with plain-text stdout from certain hook types (see issue claude-code#13912); JSON additionalContext is more reliable for context injection.

**Construction:** Always use `jq -nc --arg ctx "..."  '{hookSpecificOutput: {hookEventName: "...", additionalContext: $ctx}}'` — never manually escape JSON strings.

### LOOM_* Env Vars Available to All Hooks

Set by wrapper script (pid_tracking.rs:463-479) before `exec claude`:

| Variable               | Purpose                                             |
| ---------------------- | --------------------------------------------------- |
| `LOOM_SESSION_ID`      | Current session ID                                  |
| `LOOM_STAGE_ID`        | Current stage ID                                    |
| `LOOM_WORK_DIR`        | Absolute path to `.work/`                           |
| `LOOM_MAIN_AGENT_PID`  | Process PID (set dynamically, NOT in settings.json) |
| `LOOM_WORKTREE_PATH`   | Absolute worktree path (worktree sessions only)     |
| `LOOM_MERGE_SESSION=1` | Set for merge resolution sessions only              |

**Per-session identity gotcha (LOOM_MAIN_AGENT_PID, LOOM_STAGE_ID, LOOM_SESSION_ID):** Must NOT be in ANY settings-file env block — settings `env` overrides the process environment, so a persisted value from an earlier session shadows the wrapper's fresh exports (wrong-stage `loom memory` entries, heartbeats for the wrong session, commit-filter misidentifying the main agent). The wrapper script is the ONLY writer; `fs/permissions/settings.rs::scrub_session_identity_env()` strips these keys wherever settings are generated, copied, or merged (`generate_hooks_settings`, `create_worktree_settings`, worktree settings.local.json copy, `refresh_worktree_settings_local`, `ensure_loom_hooks_local`). Only the stable `LOOM_WORK_DIR` is persisted in settings env. `refresh_worktree_settings_local` merges main-repo permissions INTO the worktree's own settings (worktree base wins for env/hooks/defaultMode). **Claude Code applies the MAIN repo's settings env to sessions in linked worktrees** (observed v2.1.217), so worktree-side scrubbing alone is insufficient — the run path heals the main files too: `scrub_main_repo_settings_identity()` at `loom run` startup (`prepare_repo_for_run`) and `scrub_session_identity_env()` inside the sync fold-back (`merge_permissions_with_lock`), which rewrites the main settings.local.json on every stage completion (see mistakes.md 2026-07-23).

### Hook Embedding (constants.rs)

`LOOM_HOOKS` (`fs/permissions/constants.rs`) holds **23 entries**, each embedded via `include_str!()` at compile time. `install_loom_hooks()` writes them to `~/.claude/hooks/loom/` with mode 0o755. Hooks are NOT read from disk by loom at runtime.

**Do not read "23 entries" as "23 hooks."** The arithmetic, verified against `ls hooks/*.sh` and the `LOOM_HOOKS` table:

```text
24 top-level scripts in hooks/
 −1  git-pre-commit-hook.sh   (excluded from LOOM_HOOKS; appended to .git/hooks/pre-commit by loom init)
 ───
 23  LOOM_HOOKS entries installed to ~/.claude/hooks/loom/
 −1  _common.sh               (a sourced library, not a registered hook)
 ───
 22  actual Claude Code hooks
```

So: **22 Claude Code hooks + 1 shared library + 1 git-side hook = 24 scripts** (64 files including
`hooks/tests/`). Re-derive these with `fd -t f -e sh . hooks --max-depth 1 | wc -l` and
`rg -c '^    \("' loom/src/fs/permissions/constants.rs` rather than trusting the numbers here —
they have gone stale twice.

## Subagent Isolation

Three-layer defense: documentation (CLAUDE.md Rule 5), signal injection (cache.rs prefix), and hook enforcement — which is now **two** hooks, not one:

| Hook                       | Enforces                                                                                                                                                                                                                |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `commit-filter.sh`         | subagents may not run git operations; blocks AI attribution in commit messages. Matches argv TOKENS (raw-regex fallback on an unterminated quote), so quoted prose about git is not a git invocation                     |
| `subagent-verify-guard.sh` | **still matches raw command strings** (see concerns.md). subagents may not run project-wide build/test/lint/typecheck suites — at most one narrowly-scoped check. `integration-verify` stages are carved out and may run the full suite. Deliberately has **no opt-out env var** |

**Detection is payload-first, not a PPID comparison.** Both hooks gate on `loom_is_subagent()` in
`hooks/_common.sh`, which first requires `LOOM_MAIN_AGENT_PID` to be a **live ancestor** of the
current process — this scopes the globally-installed hooks to a loom stage session. Once that
passes, it classifies the caller from the hook's stdin JSON payload via `loom_payload_agent_verdict`
(`.agent_type` / `.transcript_path` / `.session_id`, which the caller cannot forge): a Task-spawned
subagent always carries a non-empty `.agent_type` and is classified SUBAGENT immediately, and a
main-session payload is recognized by its main-shaped `.transcript_path` and classified MAIN
immediately. Only a payload-less caller, or one whose verdict is "unknown", falls back to the
process-tree walk (`find_nearest_claude_ancestor` / `count_claude_processes_between`). In that
FALLBACK ONLY, a 2-level claude chain is classified MAIN AGENT and a 3-level chain is a SUBAGENT
(verified empirically with `COMMIT_FILTER_DEBUG=1`).

**Consequence worth knowing:** agent-team _teammates_ are not in the main agent's process tree,
so `LOOM_MAIN_AGENT_PID` is set but is not a live ancestor and `loom_is_subagent` returns false
for them before either check runs. Hooks gated on it therefore do **not** fire inside teammates.
A Task-tool subagent, by contrast, runs **in-process** (the same claude process as the main
agent), so the process-tree walk alone finds no intervening Claude process between it and
`LOOM_MAIN_AGENT_PID` — it is the payload check, not the process tree, that correctly classifies
it as a subagent.
