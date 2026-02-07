# Architecture

> High-level component relationships, data flow, and module dependencies.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [patterns.md](patterns.md) for design patterns, [entry-points.md](entry-points.md) for code navigation, [conventions.md](conventions.md) for coding standards.

## Project Overview

Loom is a Rust CLI (~15K lines) for orchestrating parallel Claude Code sessions across git worktrees. It enables concurrent task execution with automatic crash recovery, context handoffs, and progressive merging.

## Directory Structure

```text
loom/src/
  main.rs, lib.rs          # CLI entry (clap), module exports
  commands/                 # CLI implementations (~4K lines)
    init/, run/, stage/, status/, merge/, memory/, knowledge/, track/, runner/
  daemon/server/            # Background daemon (~1.5K lines)
    core.rs, lifecycle.rs, protocol.rs, status.rs, client.rs, orchestrator.rs
  orchestrator/             # Core engine (~4K lines)
    core/                   # Main loop, stage executor, persistence, recovery
    terminal/               # TerminalBackend trait + native OS spawning
    monitor/                # Session health, heartbeat, failure tracking
    signals/                # Signal generation (Manus format, cache, CRUD)
    continuation/           # Context handoff management
    progressive_merge/      # Merge orchestration + lock
    auto_merge.rs
  models/                   # Domain models (~1K lines)
    stage/ (types, transitions, methods)
    session/ (types, methods)
  plan/                     # Plan parsing (~1.5K lines)
    parser.rs, schema/ (types, validation), graph/ (DAG builder)
  fs/                       # File operations (~500 lines)
    work_dir.rs, knowledge.rs, memory.rs
  git/                      # Git operations (~800 lines)
    worktree/ (base, operations), merge/, branch/
  verify/                   # Acceptance + goal-backward verification (~600 lines)
    criteria/, transitions/, goal_backward/
  sandbox/                  # Claude Code sandbox config generation
    config.rs, settings.rs
  hooks/                    # Hook script definitions
  parser/frontmatter.rs     # Canonical YAML frontmatter extraction
  validation.rs             # Input validation (IDs, names)
  completions/              # Shell completion (static + dynamic)
  process/                  # PID liveness checking

.work/                      # Runtime state (gitignored)
  config.toml, stages/*.md, sessions/*.md, signals/*.md,
  handoffs/*.md, orchestrator.sock, orchestrator.pid
```

## Core Abstractions

### ExecutionGraph (plan/graph/builder.rs)

DAG of stages with dependency tracking. `get_ready()` returns stages with all deps satisfied (status == Completed AND merged == true). Cycle detection via DFS at build time.

### Stage State Machine (models/stage/)

```text
WaitingForDeps --> Queued --> Executing --> Completed --> Verified
                     |            |
                     v            +--> Blocked, NeedsHandoff, WaitingForInput,
                  Skipped              MergeConflict, CompletedWithFailures, MergeBlocked
```

11 variants total. Terminal states: Completed, Skipped. Transitions validated in transitions.rs. See [patterns.md -- State Machine Pattern](patterns.md#state-machine-pattern) for full diagram.

### StageType Enum (plan/schema/types.rs)

- **Standard** (default) -- Regular implementation stages, require goal-backward verification
- **Knowledge** -- No worktree, no commits, exploration only, auto merged=true
- **IntegrationVerify** -- Final verification, exempt from goal-backward checks
- **CodeReview** -- Security/quality review, exempt from goal-backward checks

### Session Lifecycle (models/session/)

States: Spawning -> Running -> Completed | Crashed | ContextExhausted | Paused. Tracks PID, terminal window ID, context usage %, timestamps.

### TerminalBackend (orchestrator/terminal/)

Trait for spawning Claude Code in terminal windows. NativeBackend supports 11+ emulators (kitty, alacritty, gnome-terminal, etc.) via `TerminalEmulator` enum. PID tracking via wrapper scripts that write to `.work/pids/`.

## Data Flow

### Plan Execution Flow

```text
1. loom init doc/plans/PLAN-foo.md
   --> Parse plan, create .work/, write stage files

2. loom run
   --> Spawn daemon (or foreground) --> orchestrator loop

3. Orchestrator loop (5s poll):
   Load stage files --> Build ExecutionGraph --> Find ready stages
   --> Create worktree + signal --> Spawn terminal --> Monitor sessions

4. Agent reads signal, executes, runs: loom stage complete <id>

5. Progressive merge into main branch (dependency order)
```

### IPC Protocol (daemon/server/protocol.rs)

Unix socket at `.work/orchestrator.sock`. Messages: Status, Stop, Subscribe. Length-prefixed JSON (4-byte big-endian, max 10MB). Daemon polls status every 1 second for subscribers.

## File Ownership

| Directory             | Owner Module                     | Purpose              |
| --------------------- | -------------------------------- | -------------------- |
| `.work/stages/`       | orchestrator/core/persistence.rs | Stage state          |
| `.work/sessions/`     | orchestrator/core/persistence.rs | Session state        |
| `.work/signals/`      | orchestrator/signals/            | Agent assignments    |
| `.work/handoffs/`     | orchestrator/continuation/       | Context dumps        |
| `.work/config.toml`   | commands/init/, commands/run/    | Plan reference       |
| `.worktrees/`         | git/worktree/                    | Isolated workspaces  |
| `doc/loom/knowledge/` | fs/knowledge.rs                  | Persistent learnings |

## Worktree Isolation (4-Layer Defense)

1. **Git layer** -- Separate worktrees at `.worktrees/<stage-id>/` with branch `loom/<stage-id>`. Symlinks: `.work` -> shared state, `.claude/CLAUDE.md` -> instructions, root `CLAUDE.md` -> project guidance.

2. **Sandbox layer** -- MergedSandboxConfig (sandbox/config.rs) generates `settings.local.json` with filesystem deny/allow, network domains, excluded commands. Knowledge writes via `loom knowledge update` CLI only.

3. **Signal layer** -- Four stage-type-specific stable prefix generators in cache.rs (standard, knowledge, code-review, integration-verify). Include isolation rules and subagent restrictions.

4. **Hook layer** -- commit-guard.sh blocks exit without commit. commit-filter.sh blocks subagent git operations via LOOM_MAIN_AGENT_PID/PPID comparison. See [patterns.md -- Hook Patterns](patterns.md#hook-patterns).

## Subagent Isolation

Three-layer defense: documentation (CLAUDE.md Rule 5), signal injection (cache.rs prefix), hook enforcement (commit-filter.sh). Detection: wrapper script exports LOOM_MAIN_AGENT_PID; hook compares PPID to detect subagent context.

## Layering Violations (Known Issues)

Correct dependency direction: commands/ -> orchestrator/ -> models/ (top), daemon/ / git/ / plan/ (middle), fs/ (bottom). Lower layers must not import higher.

Known violations:

- daemon imports commands (mark_plan_done_if_all_merged) -- fix: move to fs/plan_lifecycle.rs
- orchestrator imports commands (check_merge_state) -- fix: move to git/merge/status.rs
- git/worktree imports orchestrator (hook config) -- fix: extract hooks/ as top-level
- models imports plan/schema (WiringCheck, StageType) -- fix: move types to models/

## Goal-Backward Verification (verify/goal_backward/)

Three verification layers for standard stages:

- **truths** -- Shell commands returning exit 0 (30s timeout, extended criteria: stdout_contains, stderr_empty)
- **artifacts** -- Files must exist with real implementation (stub detection: TODO, FIXME, unimplemented!, todo!)
- **wiring** -- Regex patterns verifying code connections in source files

Returns: GoalBackwardResult::Passed | GapsFound | HumanNeeded. Storage: `.work/verifications/<stage-id>.json`.

## Context Budget Enforcement

Stages define context_budget (1-100%, default 65%, max 75%). Monitor tracks Green (<50%), Yellow (50-64%), Red (65%+). BudgetExceeded event triggers auto-handoff. See [patterns.md -- Context Health Pattern](patterns.md#context-health-pattern).

## Security Model

- **ID validation**: Alphanumeric + dash/underscore, max 128 chars, no path traversal (validation.rs)
- **Acceptance criteria**: Runs arbitrary shell commands (trusted model)
- **Socket**: Mode 0o600 (owner only), max 100 connections, 10MB message limit, Unix only
- **Self-update**: minisign signature verification. Gap: non-binary release assets lack verification
- **Shell escaping**: escape_shell_single_quote(), escape_applescript_string() in emulator.rs

## Merge Lock (progressive_merge/lock.rs)

MergeLock prevents concurrent merges via exclusive file at `.work/merge.lock`. Atomic creation, PID + timestamp. Timeout 30s, stale lock auto-cleanup at 5min. Released via Drop.
