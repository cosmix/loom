# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [architecture.md](architecture.md) for system overview, [conventions.md](conventions.md) for coding standards.

## Table of Contents

- [State Machine Pattern](#state-machine-pattern) - Stage/Session state machines
- [File-Based State Pattern](#file-based-state-pattern) - .work/ directory persistence
- [Signal Generation Pattern](#signal-generation-pattern) - Manus KV-cache optimization and signal types
- [Progressive Merge Pattern](#progressive-merge-pattern) - Dependency-ordered merging
- [Daemon IPC Pattern](#daemon-ipc-pattern) - Unix socket communication
- [Polling Orchestration Pattern](#polling-orchestration-pattern) - Main loop design
- [Monitoring Patterns](#monitoring-patterns) - Heartbeat, context health, retry
- [Hook Patterns](#hook-patterns) - Claude Code hook integration
- [TUI Patterns](#tui-patterns) - Terminal UI with ratatui
- [Knowledge Systems Pattern](#knowledge-systems-pattern) - Memory, facts, knowledge
- [Stage Completion Pattern](#stage-completion-pattern) - Completion and acceptance
- [Goal-Backward Verification Pattern](#goal-backward-verification-pattern) - Truths, artifacts, wiring
- [Error Handling Pattern](#error-handling-pattern) - anyhow and graceful degradation
- [Security Patterns](#security-patterns) - Input validation, shell escaping, socket security
- [Process Management Pattern](#process-management-pattern) - PID tracking, wrapper scripts, zombies
- [Learning Protection Pattern](#learning-protection-pattern) - Snapshot-based protection
- [Merge Lock Pattern](#merge-lock-pattern) - Concurrent merge prevention
- [Directory Hierarchy Pattern](#directory-hierarchy-pattern) - Three-level path model

---

## State Machine Pattern

Stage has 10 states: WaitingForDeps -> Queued -> Executing -> Completed (terminal). From Executing: Blocked, NeedsHandoff, WaitingForInput, MergeConflict, CompletedWithFailures, MergeBlocked. Skipped is terminal. **Critical invariant**: dependents become Queued only when deps have `status == Completed AND merged == true`. Session has 6 states: Spawning -> Running -> Completed/Crashed/ContextExhausted, plus Paused<->Running. All transitions validated via `try_transition()` before execution.

## File-Based State Pattern

All state persisted to `.work/` as markdown with YAML frontmatter. Benefits: git-friendly diffing, human-readable inspection, crash recovery via file re-read, no in-memory state loss. No explicit file locking for stages; relies on daemon single-writer model. Stage files named with topological depth prefix (e.g., `01-knowledge-bootstrap.md`).

## Signal Generation Pattern

Uses Manus KV-cache optimization with four sections: **Stable prefix** (never changes, cacheable - isolation rules), **Semi-stable** (per-stage - knowledge, facts), **Dynamic** (per-session - assignment, handoff), **Recitation** (per-session - immediate tasks, memory for max attention). Implementation: `cache.rs` (stable prefix), `format.rs` (sections). Four stage-type-specific prefix generators exist (standard, knowledge, code-review, integration-verify). Six signal types: Regular, Knowledge, Recovery, Merge, MergeConflict, BaseConflict. Signals are self-contained via `EmbeddedContext` struct - agents never read from main repo; the signal file is the single source of truth.

## Progressive Merge Pattern

Dependencies merged to main before dependent stages execute: `Stage A completes -> Merge A to main -> Stage B starts`. Ensures clean base with integrated dependency work. Base branch resolution: no deps = init_base_branch or default; all deps merged = merge point (main); single dep not merged = dependency branch (legacy fallback). MergeLock (`progressive_merge/lock.rs`) prevents concurrent merges via exclusive file at `.work/merge.lock` with 30s timeout and 5min stale auto-cleanup.

## Daemon IPC Pattern

Unix socket-based IPC with 4-byte big-endian length-prefixed JSON (max 10MB). Supports SubscribeStatus (streaming updates every 1s), Stop, and Ping. Socket at `.work/orchestrator.sock` with mode 0o600 (owner-only), max 100 connections. Graceful shutdown: client sends Stop, server checks shutdown_flag in accept loop, waits for threads, cleanup removes socket/PID/completion marker. Drop impl ensures cleanup on panic.

## Polling Orchestration Pattern

Main loop polls every 5 seconds: sync graph from stage files, sync queued status, spawn merge resolution sessions, start ready stages, poll monitor for events, handle events (crashes/completions). Exit when all stages complete or (failed + no sessions + no ready). `is_complete()` checks all stages Completed or Skipped.

## Monitoring Patterns

**Heartbeat**: Sessions write to `.work/heartbeat/{stage-id}.json` with timestamp, context_percent, last_tool. Timeout: 300s without heartbeat. PID alive + stale = Hung; PID dead = Crashed; PID dead + stage Completed = normal exit (skip crash event). **Context health**: Three tiers - Green (0-60%), Yellow (60-75% auto-summarize), Red (75%+ trigger handoff). Stages can set custom `context_budget` (1-100%, default 65%). **Retry**: Exponential backoff `min(30 * 2^retry_count, 300s)`. Retryable: SessionCrash, Timeout. Non-retryable: ContextExhausted, TestFailure, BuildFailure, CodeError. Max 3 retries default.

## Hook Patterns

Hooks receive data via **stdin JSON** (not env vars). Read with `timeout 1 cat` to prevent blocking. Response: exit 0 = allow, exit 2 = block (stderr shown to Claude). Advanced JSON response supports `permissionDecision: allow/deny/ask` with `updatedInput` for auto-correction.

**Key hooks**: commit-guard.sh (Stop) blocks exit without commit; commit-filter.sh (PreToolUse:Bash) blocks subagent commits and Co-Authored-By attribution; prefer-modern-tools.sh blocks grep/find; post-tool-use.sh updates heartbeat; ask-user-pre/post.sh manages WaitingForInput state; pre-compact.sh triggers handoff; session-start/end.sh handle lifecycle.

**Subagent detection**: Wrapper script exports `LOOM_MAIN_AGENT_PID`. Hook compares `$PPID` to this value. Main agent matches; subagent differs. Subagents blocked from: git commit, git add -A/., loom stage complete.

Hook installation: scripts embedded via `include_str!()` in constants.rs, installed to `~/.claude/hooks/loom/`, config added to `.claude/settings.local.json`.

## TUI Patterns

Two display modes: **static** (one-time print) and **live** (real-time dashboard via daemon socket). Live mode uses ratatui with vertical layout: header(3), progress bar(3), main content(min 10, two 50/50 columns), footer(3). Left column: Executing(60%)+Pending(40%). Right: Completed(60%)+Blocked(40%). `unified_stages()` merges all categories, sorted by DAG depth then ID. Status colors: Executing=Blue, Completed=Green, Blocked=Red. Context colors: 0-60%=Green, 60-75%=Yellow, 75%+=Red.

## Knowledge Systems Pattern

Three agent knowledge systems: **Facts** (.work/facts.toml, cross-stage KV pairs), **Memory** (.work/memory/{session}.md, session journal), **Knowledge** (doc/loom/knowledge/, permanent curation). Memory placed in signal recitation section for max LLM attention. Promotion: `loom memory promote <type> <target>` moves session insights to knowledge files. Knowledge is append-only (`append()`, never overwrite). Protected files marked with `<!-- .loom-protected -->`.

## Stage Completion Pattern

**Regular stages**: Load stage, run acceptance criteria (unless --no-verify), sync worktree permissions to main, run task verifications, progressive merge into main, mark Completed, trigger dependents. **Knowledge stages**: No worktree (main repo), auto-sets merged=true, skips merge. Acceptance commands run with 5-min timeout, support `${WORKTREE}`, `${PROJECT_ROOT}`, `${STAGE_ID}` variables. Session factory methods: `Session::new()`, `Session::new_merge()`, `Session::new_base_conflict()`.

## Goal-Backward Verification Pattern

Problem: tests passing does not equal feature working. Solution: three verification layers in stage definitions. **Truths**: shell commands returning exit 0 (extended: exit_code, stdout_contains, stderr_empty). **Artifacts**: files must exist with real implementation; stub detection blocks TODO/FIXME/unimplemented!/todo!/pass/raise NotImplementedError. **Wiring**: grep patterns verify code connections (source + pattern + description). Required only for `stage_type: standard`; Knowledge, IntegrationVerify, CodeReview are exempt. Limits: max 20 truths, 100 artifacts per stage.

## Error Handling Pattern

Uses `anyhow::Result<T>` throughout. Context via `.context()` and `.with_context(|| format!())`. Validation via `bail!()`. **Graceful degradation** on non-critical paths: skill loading with warning fallback, `if let Ok()` for stage loading, `unwrap_or(false)` for liveness checks. Zero `unwrap()`/`expect()` in main code; assertions only for invariants.

## Security Patterns

**Input validation**: IDs validated with `validate_id()` - alphanumeric + dash/underscore, max 128 chars, reserved names blocked. `safe_filename()` strips traversal. **Shell escaping**: `escape_shell_single_quote()` uses `'\''` pattern; `escape_applescript_string()` escapes backslashes and double quotes. Located in `emulator.rs`. **Self-update**: minisign signature verification (50MB binary limit, 4KB sig limit), atomic install via temp->backup->rename->rollback. Non-binary release assets (CLAUDE.md.template, agents.zip, skills.zip) lack verification. **Environment variable expansion**: use positional replacement, not global replace, to handle overlapping names ($FOO vs $FOOBAR).

## Process Management Pattern

**Wrapper script** (`pid_tracking.rs`): Creates `.work/wrappers/{stage_id}-wrapper.sh` that sets env vars (LOOM_SESSION_ID, LOOM_STAGE_ID, LOOM_WORK_DIR, LOOM_MAIN_AGENT_PID), writes PID to `.work/pids/{stage_id}.pid`, then `exec claude` (inherits shell PID for reliable tracking). **PID discovery**: file read first, then Linux `/proc` scan or macOS `ps aux`/`lsof` fallback. **Liveness check**: PID file -> kill -0 -> session.pid -> window existence by title. **Session kill**: close window by title, fallback SIGTERM to PID. **Zombie prevention**: `spawn_reaper_thread()` calls `wait()` in background thread.

## Learning Protection Pattern

Learning files protected from agent deletion via snapshot/restore: `snapshot_before_session()` saves state, session executes, `verify_after_session()` compares, restore if damaged. Protected marker: `<!-- .loom-protected -->` at file start.

## Merge Lock Pattern

`MergeLock` prevents concurrent merges via exclusive file at `.work/merge.lock`. Uses `create_new(true)` for atomic creation. Writes PID + timestamp. 30s acquisition timeout, 5min stale auto-cleanup. Released via Drop trait. Acquired by `merge_stage()` before git operations.

## Directory Hierarchy Pattern

Three-level model: **Project Root** (main repo), **Worktree** (`.worktrees/<stage-id>/`), **working_dir** (YAML field, subdirectory within worktree). Path resolution: `EXECUTION_PATH = worktree_root + working_dir`. If working_dir="." use worktree root; if "loom" join `.worktrees/<stage>/loom/`; missing subdirectory falls back to worktree root with warning. All acceptance/artifact/wiring paths are relative to working_dir. Common mistake: `cargo test` failing because working_dir is not set to the directory containing Cargo.toml.

## Permission Sync Pattern

Three-component flow: path transformation (absolute->relative, parent traversal resolved), merge-not-overwrite (`merge_permission_vecs` with union+dedup), sync before acceptance (ensures permissions persist for retry). File locking via fs2 crate during merge; always write to the locked handle, never a new File handle.

## Agent Anti-Patterns

**Binary usage**: Agents use `target/debug/loom` instead of `loom` from PATH, causing version mismatch and state corruption. **Direct state editing**: Agents edit `.work/stages/*.md` directly instead of using loom CLI, corrupting state. Both are prohibited in CLAUDE.md and signal stable prefix.
