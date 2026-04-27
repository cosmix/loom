# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
>
> **Related files:** [architecture.md](architecture.md) for system overview, [conventions.md](conventions.md) for coding standards.

## Table of Contents

- [State Machine Pattern](#state-machine-pattern)
- [File-Based State Pattern](#file-based-state-pattern)
- [Signal Generation Pattern](#signal-generation-pattern)
- [Progressive Merge Pattern](#progressive-merge-pattern)
- [Daemon IPC Pattern](#daemon-ipc-pattern)
- [Polling Orchestration Pattern](#polling-orchestration-pattern)
- [Monitoring Patterns](#monitoring-patterns)
- [Hook Patterns](#hook-patterns)
- [TUI Patterns](#tui-patterns)
- [Knowledge Systems Pattern](#knowledge-systems-pattern)
- [Stage Completion Pattern](#stage-completion-pattern)
- [Goal-Backward Verification Pattern](#goal-backward-verification-pattern)
- [Error Handling Pattern](#error-handling-pattern)
- [Security Patterns](#security-patterns)
- [Process Management Pattern](#process-management-pattern)

---

## State Machine Pattern

Stage has 12 states: WaitingForDeps -> Queued -> Executing -> Completed (terminal). From Executing: Blocked, NeedsHandoff, WaitingForInput, MergeConflict, CompletedWithFailures, MergeBlocked, NeedsHumanReview. Skipped is terminal. **Critical invariant**: dependents become Queued only when deps have `status == Completed AND merged == true`. Session has 6 states: Spawning -> Running -> Completed/Crashed/ContextExhausted, plus Paused<->Running. All transitions validated via `try_transition()`.

## File-Based State Pattern

All state persisted to `.work/` as markdown with YAML frontmatter. Benefits: git-friendly diffing, human-readable, crash recovery via file re-read. No explicit file locking; relies on daemon single-writer model. Stage files named with topological depth prefix (e.g., `01-knowledge-bootstrap.md`).

## Signal Generation Pattern

Uses Manus KV-cache optimization with four sections:

1. **Stable prefix** (~1000 bytes): Worktree rules, execution rules, CLAUDE.md reminders. SHA-256 hashed. Rarely changes. Includes self-review checklist (standard) or detailed review dimensions (integration-verify).
2. **Semi-stable** (~1500-2500 bytes): Knowledge refs, memory/knowledge management, agent teams, sandbox, skill recommendations. Changes per stage type.
3. **Dynamic** (variable): Target metadata, plan overview, dependency status, handoff content, git history, files, tasks. Changes per session.
4. **Recitation** (end): Memory entries (last 10), task state, critical context. Placed last for maximum attention weight.

Four stage-type-specific prefix generators: standard, knowledge, integration-verify, knowledge-distill. Six signal types: Regular, Knowledge, Recovery, Merge, MergeConflict, BaseConflict. Signals are self-contained via `EmbeddedContext` struct.

KnowledgeDistill prefix: focuses on memory reading and knowledge curation; includes `loom memory show --all` and `loom knowledge update` guidance; always uses sonnet.

**Data flow:** Stage Ready -> start_stage() -> create worktree -> Session.new() -> build_signal_context() -> format_signal_content() -> write_signal_file() -> spawn Claude Code.

## Progressive Merge Pattern

Dependencies merged to main before dependent stages execute: `Stage A completes -> Merge A to main -> Stage B starts`. Base branch resolution: no deps = init_base_branch or default; all deps merged = main; single dep not merged = dependency branch (legacy fallback). MergeLock prevents concurrent merges (30s timeout, 5min stale cleanup).

## Daemon IPC Pattern

Unix socket with 4-byte big-endian length-prefixed JSON (max 10MB). Supports SubscribeStatus (streaming 1s), Stop, Ping. Socket at `.work/orchestrator.sock`, mode 0o600, max 100 connections. Graceful shutdown: Stop -> shutdown_flag -> wait threads -> cleanup socket/PID. Drop ensures cleanup on panic.

## Polling Orchestration Pattern

Main loop polls every 5 seconds: sync graph from stage files, sync queued status, spawn merge resolution sessions, start ready stages, poll monitor for events, handle events. Exit when all stages complete or (failed + no sessions + no ready).

## Monitoring Patterns

**Heartbeat**: Sessions write to `.work/heartbeat/{stage-id}.json`. Timeout: 300s. PID alive + stale = Hung; PID dead = Crashed; PID dead + stage Completed = normal exit. **Context health**: Green (0-60%), Yellow (60-75% auto-summarize), Red (75%+ trigger handoff). Custom `context_budget` per stage (1-100%, default 65%). **Retry**: Exponential backoff `min(30 * 2^retry_count, 300s)`. Retryable: SessionCrash, Timeout. Non-retryable: ContextExhausted, TestFailure, BuildFailure, CodeError. Max 3 retries.

## Hook Patterns

Hooks receive data via **stdin JSON**. Read with `timeout 1 cat`. Response: exit 0 = allow, exit 2 = block (stderr shown). Advanced JSON response supports `permissionDecision: allow/deny/ask` with `updatedInput`.

**Key hooks**: commit-guard.sh (Stop) blocks exit without commit; commit-filter.sh (PreToolUse:Bash) blocks subagent commits; prefer-modern-tools.sh blocks grep/find; post-tool-use.sh updates heartbeat; pre-compact.sh triggers handoff; session-start/end.sh handle lifecycle.

**Subagent detection**: Wrapper script exports `LOOM_MAIN_AGENT_PID`. Hook compares `$PPID`. Subagents blocked from: git commit, git add -A/., loom stage complete.

Hook installation: scripts embedded via `include_str!()` in constants.rs, installed to `~/.claude/hooks/loom/`, config in `.claude/settings.local.json`.

## TUI Patterns

Two modes: **static** (one-time print) and **live** (real-time via daemon socket). Live uses ratatui with vertical layout: header(3), progress(3), main(min 10, two 50/50 columns), footer(3). Left: Executing(60%)+Pending(40%). Right: Completed(60%)+Blocked(40%). Three-layer cleanup: panic hook, Ctrl+C signal handler, Drop with `cleaned_up` flag.

## Knowledge Systems Pattern

Three systems: **Facts** (.work/facts.toml, cross-stage KV), **Memory** (.work/memory/{session}.md, session journal), **Knowledge** (doc/loom/knowledge/, permanent). Memory placed in signal recitation section for max LLM attention. Promotion: `loom memory promote`. Knowledge is append-only. Protected files marked with `<!-- .loom-protected -->`. Knowledge commands use `project_root()` (cwd-relative) so worktree agents write to their worktree, not the main repo.

## Stage Completion Pattern

**Regular stages**: Load stage, run acceptance criteria (unless --no-verify), sync worktree permissions, run task verifications, progressive merge, mark Completed, trigger dependents. **Knowledge stages**: No worktree, commits required (directly to main), auto merged=true, skips merge. Acceptance commands: 5-min timeout, support `${WORKTREE}`, `${PROJECT_ROOT}`, `${STAGE_ID}` variables.

## Goal-Backward Verification Pattern

Three verification layers: **Truths** (shell commands, exit 0, extended: exit_code, stdout_contains, stderr_empty). **Artifacts** (files must exist, stub detection blocks TODO/FIXME/unimplemented!/todo!/pass/raise NotImplementedError). **Wiring** (grep patterns verify code connections). Required for `stage_type: standard` only. Limits: max 20 truths, 100 artifacts.

Before/after stage checks: before_stage runs AFTER worktree creation, BEFORE Executing (advisory). after_stage runs in complete.rs (blocking). Both use TruthCheck definitions.

Regression tests: `bug_fix: true` requires `regression_test` with file path and must_contain patterns. Bidirectional validation.

Advisory stderr warning detection: detect_stderr_warnings() in runner.rs scans for 9 suspicious patterns (connection refused, blocked, EACCES, etc.) after acceptance. Warnings only, no pass/fail change.

## Error Handling Pattern

`anyhow::Result<T>` throughout. Context via `.context()` and `.with_context()`. **Graceful degradation**: skill loading with warning fallback, `if let Ok()` for stage loading, `unwrap_or(false)` for liveness checks. Zero `unwrap()`/`expect()` in main code.

## Security Patterns

**Input validation**: `validate_id()` - alphanumeric + dash/underscore, max 128 chars, reserved names blocked. `safe_filename()` strips traversal. **Shell escaping**: `escape_shell_single_quote()` and `escape_applescript_string()` in emulator.rs. **Self-update**: minisign signature verification (50MB binary, 4KB sig), atomic install via temp->backup->rename->rollback. **Env var expansion**: positional replacement to handle overlapping names ($FOO vs $FOOBAR).

## Process Management Pattern

**Wrapper script** (pid_tracking.rs): Creates `.work/wrappers/{stage_id}-wrapper.sh`, sets env vars, writes PID, then `exec claude`. **PID discovery**: file read first, then `/proc` scan (Linux) or `ps aux`/`lsof` (macOS). **Liveness**: PID file -> kill -0 -> session.pid -> window by title. **Zombie prevention**: `spawn_reaper_thread()` calls `wait()`.

## Merge Anti-Respawn Pattern

When merge conflict session dies unresolved: session removed from `active_sessions`, signal file KEPT as anti-respawn guard. `spawn_merge_resolution_sessions()` checks `has_merge_signal_for_stage()` before spawning. Signal removed only when merge succeeds.

## Permission Sync Pattern

Three-component: path transformation (absolute->relative, parent traversal resolved), merge-not-overwrite (union+dedup), sync before acceptance. File locking via fs2 crate; always write to the locked handle.

## Sandbox Config Merging

Plan-level SandboxConfig merges with stage-level. Stage overrides plan. excluded_commands concatenate. Output: settings.local.json with sandbox.enabled, autoAllowBashIfSandboxed, network allowlist, permissions.

## Directory Hierarchy Pattern

Three-level: **Project Root**, **Worktree** (`.worktrees/<stage-id>/`), **working_dir** (YAML field). Path resolution: `EXECUTION_PATH = worktree_root + working_dir`. All acceptance/artifact/wiring paths relative to working_dir. Common mistake: `cargo test` failing because working_dir not set to Cargo.toml directory.

## Three-Layer Guidance Reinforcement

New agent guidance should be reinforced at: (1) Skill file (depth), (2) CLAUDE.md.template (authority), (3) cache.rs signals (runtime enforcement). Ensures guidance reaches agents regardless of entry point.

## Stage Necessity Test

Before creating stages: Q1: Does it create code another imports? Q2: Does it write files another writes? Q3: Does it need a verification checkpoint? If ALL NO -> merge into one stage with parallel subagents.

## Bootstrap Mode

`loom knowledge bootstrap` defaults to interactive mode (Stdio::inherit) for macOS compatibility. `--quick` opts into non-interactive (Stdio::null + -p flag). Exit codes 130/2 treated as user interrupt.

## Field Propagation Checklist

When adding new fields to StageDefinition: (1) plan/schema/types.rs, (2) models/stage/types.rs + Default, (3) commands/init/plan_setup.rs mapping, (4) plan/schema/tests/mod.rs make_stage(), (5) ALL test files constructing Stage, (6) validation.rs rules, (7) fs/stage_loading.rs, plan/graph/tests.rs, models/stage/methods.rs.

## Goal-Backward Verification Pattern [UPDATED]

Four verification layers: **Artifacts** (files must exist, stub detection blocks TODO/FIXME/unimplemented\!/todo\!/pass/raise NotImplementedError). **Wiring** (grep patterns verify code connections). **Wiring Tests** (runtime commands with success criteria). **Dead Code Check** (command + fail/ignore patterns).

Truths were removed as a standalone verification layer and unified into the acceptance field as AcceptanceCriterion::Extended(TruthCheck). Required for `stage_type: standard` and `integration-verify` — must have acceptance OR goal-backward checks.

Before/after stage checks: before_stage runs AFTER worktree creation, BEFORE Executing (advisory). after_stage runs in complete.rs (blocking). Both use TruthCheck definitions via verify_truth_checks() in truths.rs.

## AcceptanceCriterion Design Pattern

Uses `#[serde(untagged)]` enum with two variants:

- `Simple(String)` — plain shell command, deserializes from YAML string
- `Extended(TruthCheck)` — output validation, deserializes from YAML object with `command` field

Serde tries variants in order: strings match Simple first, objects fail Simple then match Extended. Error messages for malformed objects are poor (inherent untagged limitation). helper methods: `command()`, `is_extended()`, `Display` delegates to `command()`.

## Hook Content-Stripping Pattern

Hooks that validate bash commands must strip embedded text content before
pattern matching. The strip_embedded_content() function (in hooks/\_common.sh
for shell, validators/bash.rs for Rust) removes:

1. Heredoc bodies (awk state machine tracking <<MARKER to MARKER)
2. -m / --message quoted content (sed replacements)

Each hook sources \_common.sh via: source "$(dirname "$0")/\_common.sh"

Full hook inventory (13 scripts in hooks/):

- PreToolUse: worktree-isolation.sh, commit-filter.sh, git-add-guard.sh,
  prefer-modern-tools.sh, worktree-file-guard.sh
- PostToolUse: post-tool-use.sh, ask-user-post.sh
- Stop: commit-guard.sh, learning-validator.sh
- SessionStart: session-start.sh
- SessionEnd: session-end.sh
- PreCompact: pre-compact.sh
- UserPromptSubmit: skill-trigger.sh, ask-user-pre.sh

## Hook Content-Stripping Pattern (Updated 2026-03-31)

All PreToolUse hooks that match command patterns MUST use `strip_embedded_content()` before pattern matching to prevent false positives from keywords appearing inside commit messages or heredoc bodies.

**Architecture:**

- `_common.sh` provides `strip_embedded_content()` (shared across all shell hooks)
- `loom/src/hooks/validators/bash.rs` provides Rust equivalent `strip_embedded_content()`
- Phase 1: awk state machine strips heredoc bodies (`<<MARKER` to `^MARKER$`)
- Phase 2: sed strips `-m`/`--message` quoted content

**Usage pattern:**

1. Source `_common.sh` at top of hook
2. Call `stripped=$(strip_embedded_content "$cmd")`
3. Use `$stripped` for pattern detection (git -C, .worktrees/, ../../, grep, find)
4. Use original `$cmd` for patterns that MUST match message body (e.g., Co-Authored-By)

**Commit-filter dual-check:**

- STRIPPED_COMMAND for detecting `git commit` (prevents "commit" in messages from triggering)
- ORIGINAL COMMAND for Co-Authored-By check (anchor `^` prevents mid-line false positives)

**Security posture:** All stripping failures result in false positives (overly strict), never bypasses (permissive). This is the correct safety direction for development hooks.

**Hooks using this pattern:** worktree-isolation.sh, commit-filter.sh, git-add-guard.sh, prefer-modern-tools.sh

## Merge Recovery Flow [UPDATED 2026-04-16]

MergeConflict -> bail\!() forces original session to exit -> commit-guard.sh allows exit for MergeConflict status -> detection.rs recognizes as normal exit -> spawn_merge_resolution_sessions() kills any stale original session, then spawns resolver -> merge signal includes "Inherited Responsibilities" section explaining resolver owns the stage -> user directed to `loom stage merge <stage-id> --resolved`.

Key invariant: the original execution session MUST exit when merge conflict is detected. Three mechanisms enforce this:

1. `bail\!()` in `complete_with_merge()` propagates error and terminates the session
2. `commit-guard.sh` does NOT block exit for MergeConflict status
3. `spawn_merge_resolution_sessions()` actively kills stale sessions before spawning resolver

## macOS GUI App Launch Pattern (2026-04-27)

macOS apps installed in `/Applications/X.app` may ship a CLI binary inside `Contents/MacOS/` that is NOT added to PATH. To launch with arguments without requiring a manual PATH shim, use `open -na <AppName> --args <flags...>` from `Command::new("open")`. The CLI flags following `--args` are passed through to the new process exactly as if invoked directly — Ghostty's `--working-directory=`, `--title=`, and `-e CMD` all work this way (per Ghostty maintainer in ghostty-org/ghostty#9221).

**`-na` vs `-a`:** Always use `-na` (force new instance) when each invocation needs its own per-window args. With `-a`, an already-running singleton may ignore `--args` and just focus the existing window — `--working-directory` and `-e` would silently no-op. Trade-off: process accumulation, acceptable when each window corresponds to a finite stage.

**Where applied:** `emulator.rs` `Self::Ghostty` arm uses this on macOS while keeping the direct `ghostty <args>` invocation on Linux via `#[cfg(not(target_os = "macos"))]`. The arm-level cfg-gating pattern (rather than per-emulator-variant duplication) keeps cross-platform terminals together. Same approach applies to any future `.app`-distributed terminal emulator added to loom.

**When NOT to use:** Mac-only emulators (`TerminalApp`, `ITerm2`) already use AppleScript via `osascript`, which is itself PATH-independent — no `open` needed. Use `open -na ... --args` only when the underlying tool accepts CLI flags directly.
