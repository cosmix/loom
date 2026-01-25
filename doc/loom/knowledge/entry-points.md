# Entry Points

> Key files agents should read first to understand the codebase.
> This file is append-only - agents add discoveries, never delete.

## CLI Entry Point

- `loom/src/main.rs` - CLI entry point using clap with `#[derive(Parser)]`
- `loom/src/lib.rs` - Module exports (14 public modules)

## Core Command Dispatch

Commands are dispatched through a `Commands` enum match statement in `main()`:

| Command  | Entry File                      | Purpose                                     |
| -------- | ------------------------------- | ------------------------------------------- |
| `init`   | `commands/init/execute.rs`      | Initialize `.work/` from plan file          |
| `run`    | `commands/run/mod.rs`           | Start orchestrator (daemon or foreground)   |
| `status` | `commands/status.rs`            | Display dashboard with stage/session status |
| `stop`   | `commands/stop.rs`              | Shutdown running daemon                     |
| `stage`  | `commands/stage/`               | Stage lifecycle management                  |
| `merge`  | `commands/merge/execute/mod.rs` | Merge completed stages                      |
| `attach` | `commands/attach.rs`            | Attach to running sessions                  |

## Orchestrator Core

- `orchestrator/core/orchestrator.rs` - Main orchestration loop (5-second polling)
- `orchestrator/core/stage_executor.rs` - Stage spawning logic
- `orchestrator/core/event_handler.rs` - Crash detection and recovery
- `orchestrator/core/persistence.rs` - Load/save state to disk

## Key Data Models

- `models/stage/types.rs` - Stage state machine (10 states)
- `models/stage/transitions.rs` - State transition validation
- `models/session/types.rs` - Session lifecycle (6 states)
- `models/plan.rs` - Plan container with lifecycle tracking

## Plan Parsing

- `plan/parser.rs` - Markdown plan document parser
- `plan/schema/types.rs` - YAML metadata schema (LoomMetadata, StageDefinition)
- `plan/schema/validation.rs` - Stage validation rules
- `plan/graph/mod.rs` - Execution DAG building

## Git Operations

- `git/worktree/operations.rs` - Create/remove worktrees at `.worktrees/{stage-id}/`
- `git/worktree/base.rs` - Base branch resolution for dependencies
- `git/merge.rs` - Merge automation and conflict handling
- `git/branch.rs` - Branch creation, deletion, ancestry checks

## File System State

- `fs/work_dir.rs` - `.work/` directory management
- `fs/stage_files.rs` - Stage file naming (`{depth}-{stage-id}.md`)
- `fs/session_files.rs` - Session file operations
- `fs/knowledge.rs` - Knowledge directory (`doc/loom/knowledge/`)

## Daemon Architecture

- `daemon/server/core.rs` - DaemonServer struct and socket binding
- `daemon/server/lifecycle.rs` - Daemonization and server loop
- `daemon/protocol.rs` - IPC message types (Request/Response enums)
- `daemon/server/broadcast.rs` - Status/log streaming to clients

## Session Monitoring

- `orchestrator/monitor/core.rs` - Monitor struct for health tracking
- `orchestrator/monitor/detection.rs` - Crash and hung session detection
- `orchestrator/monitor/heartbeat.rs` - Heartbeat protocol (5-minute timeout)
- `orchestrator/monitor/context.rs` - Context usage tracking (Green/Yellow/Red)

## Signal Generation

- `orchestrator/signals/crud.rs` - Signal file CRUD operations
- `orchestrator/signals/format.rs` - Manus pattern formatting (4-section KV-cache optimized)
- `orchestrator/signals/merge.rs` - Merge conflict resolution signals
- `orchestrator/signals/recovery.rs` - Recovery signal generation

## Terminal Backend

- `orchestrator/terminal/mod.rs` - TerminalBackend trait definition
- `orchestrator/terminal/native/mod.rs` - Native backend implementation
- `orchestrator/terminal/native/spawner.rs` - Claude Code session spawning
- `orchestrator/terminal/emulator.rs` - 11 terminal emulator support

## Verification System

- `verify/criteria/runner.rs` - Acceptance criteria execution
- `verify/transitions/state.rs` - Stage state transitions
- `verify/transitions/persistence.rs` - Stage file persistence
- `verify/learning_protection.rs` - Learning file snapshot/restore

## Handoff System

- `handoff/detector.rs` - Context threshold detection (60%/75%)
- `handoff/generator/mod.rs` - Handoff file generation
- `handoff/schema.rs` - HandoffV2 structured format

## Key Configuration Files

- `.work/config.toml` - Active plan reference and settings
- `.work/stages/{depth}-{stage-id}.md` - Stage state files (YAML frontmatter)
- `.work/sessions/{session-id}.md` - Session tracking files
- `.work/signals/{session-id}.md` - Agent instruction signals
- `doc/plans/PLAN-*.md` - Plan definition files

## TUI Module

- `commands/status/ui/tui.rs:638` - run_tui() entry point
- `commands/status/ui/graph_widget.rs` - DAG visualization widget

## Hook Configuration

- `fs/permissions/settings.rs:22` - ensure_loom_permissions() entry point
- `fs/permissions/hooks.rs` - Hook installation and configuration

## CLAUDE.md Template

- `CLAUDE.md.template` - Canonical binding rules template for agents
- `commands/self_update/mod.rs` - Installation and update logic

## Skill System

- `skills/<skill-name>/SKILL.md` - Individual skill definitions
- `commands/self_update/mod.rs:229-235` - Skill download and installation

## Hook Shell Scripts

- `hooks/` directory - Hook scripts (commit-guard.sh, learning-validator.sh, etc.)

## Hook Registration

- `orchestrator/hooks/config.rs:10-28` - HookEvent enum
- `fs/permissions/hooks.rs:82-106` - install_loom_hooks()
- `commands/hooks.rs:19-46` - `loom hooks install` command

## Stage Completion

- `commands/stage/complete.rs:121` - complete() main dispatcher
- `commands/stage/complete.rs:31` - complete_knowledge_stage()

## Acceptance Criteria

- `verify/criteria/runner.rs:16` - run_acceptance() entry point
- `verify/criteria/executor.rs:36` - run_single_criterion_with_timeout()

## Stage Verify

- `commands/stage/verify.rs` - Re-verify completed stages

## Shell Completions

- `completions/generator.rs` - Static completion via clap_complete
- `completions/dynamic/mod.rs` - Context-aware dynamic completion

---

## Discovery Documentation Summary (2026-01-25)

Comprehensive analysis of 288 discovery documentation files from `doc/discovery/`.

### Command System Architecture

| Command Area | Key Files                                                   | Purpose                                                        |
| ------------ | ----------------------------------------------------------- | -------------------------------------------------------------- |
| `run`        | `commands/run/mod.rs`, `foreground.rs`, `plan_lifecycle.rs` | Start orchestrator, manage plan status (PLAN→IN_PROGRESS→DONE) |
| `stage`      | `commands/stage/complete.rs`, `verify.rs`, `recover.rs`     | Stage lifecycle: completion, verification, recovery            |
| `status`     | `commands/status/ui/tui/app.rs`, `render/*.rs`              | TUI dashboard with live updates via daemon socket              |
| `merge`      | `commands/merge/execute/mod.rs`, `recovery.rs`              | Merge with conflict detection, resolution session spawning     |
| `init`       | `commands/init/execute.rs`, `plan_setup.rs`                 | Workspace init, stage file creation with depth prefixes        |
| `memory`     | `commands/memory/handlers.rs`                               | Per-session memory journal (note/decision/question)            |
| `graph`      | `commands/graph/display.rs`, `tree.rs`                      | Execution DAG visualization with topological levels            |

### Orchestrator Core Components

| Component          | File                                      | Function                                            |
| ------------------ | ----------------------------------------- | --------------------------------------------------- |
| Main Loop          | `orchestrator/core/orchestrator.rs`       | 5-second polling, session spawn, crash recovery     |
| Stage Executor     | `orchestrator/core/stage_executor.rs`     | Worktree creation, signal generation, session spawn |
| Event Handler      | `orchestrator/core/event_handler.rs`      | Dispatches StageCompleted, SessionCrashed, etc.     |
| Crash Handler      | `orchestrator/core/crash_handler.rs`      | Failure classification, exponential backoff retry   |
| Completion Handler | `orchestrator/core/completion_handler.rs` | Auto-merge BEFORE marking completed                 |
| Merge Handler      | `orchestrator/core/merge_handler.rs`      | Conflict detection, merge session spawning          |

### Monitor Subsystem

| Component   | File                                  | Purpose                                         |
| ----------- | ------------------------------------- | ----------------------------------------------- |
| Core        | `orchestrator/monitor/core.rs`        | Coordinates detection, heartbeat, checkpoints   |
| Detection   | `orchestrator/monitor/detection.rs`   | Stage/session state change detection            |
| Heartbeat   | `orchestrator/monitor/heartbeat.rs`   | Hung detection (300s timeout), crash detection  |
| Context     | `orchestrator/monitor/context.rs`     | Green (<50%), Yellow (50-64%), Red (≥65%)       |
| Checkpoints | `orchestrator/monitor/checkpoints.rs` | Task completion polling, verification injection |

### Signal System (Manus KV-Cache Pattern)

Signals in `.work/signals/{session-id}.md` use 4-section structure:

1. **STABLE PREFIX** - Fixed rules, CLAUDE.md reminders (cached)
2. **SEMI-STABLE** - Knowledge summary, skill recommendations
3. **DYNAMIC** - Target info, assignment, dependencies, handoff
4. **RECITATION** - Task progression, immediate tasks, memory (max attention)

Key files: `orchestrator/signals/generate.rs`, `format.rs`, `crud.rs`

### Git Operations

| Area      | Key Files                               | Operations                                                |
| --------- | --------------------------------------- | --------------------------------------------------------- |
| Worktrees | `git/worktree/operations.rs`, `base.rs` | Create at `.worktrees/{stage-id}/`, resolve base branch   |
| Branches  | `git/branch/operations.rs`, `naming.rs` | `loom/{stage-id}` naming, ancestry checks                 |
| Merge     | `git/merge.rs`                          | MergeResult: Success/Conflict/FastForward/AlreadyUpToDate |
| Cleanup   | `git/cleanup/batch.rs`                  | Post-merge: remove worktree, delete branch, prune         |

### File System State Structure

```
.work/
├── config.toml          # Active plan, base_branch
├── stages/              # {depth}-{stage-id}.md (YAML frontmatter)
├── sessions/            # {session-id}.md
├── signals/             # Agent instruction signals
├── handoffs/            # Context exhaustion dumps
├── memory/              # Per-session journals
├── task-state/          # Task progression YAML
├── checkpoints/         # Task completion records
├── crashes/             # Crash recovery logs
├── heartbeat/           # Session heartbeat JSON
└── hooks/events.jsonl   # Hook event log
```

### Data Models

**Stage** (11 states): WaitingForDeps → Queued → Executing → Completed/Blocked/NeedsHandoff/WaitingForInput/MergeConflict/CompletedWithFailures/MergeBlocked/Skipped

**Session** (6 states): Spawning → Running → Completed/Crashed/ContextExhausted/Paused

**Scheduling Invariant**: Stage ready only when ALL dependencies have `status == Completed` AND `merged == true`

### Terminal Spawning

| Component       | File                                           | Purpose                                                        |
| --------------- | ---------------------------------------------- | -------------------------------------------------------------- |
| Backend Trait   | `orchestrator/terminal/mod.rs`                 | Unified interface for spawn/kill/alive                         |
| Emulator Config | `orchestrator/terminal/emulator.rs`            | 11+ terminals (kitty, alacritty, gnome-terminal, etc.)         |
| Detection       | `orchestrator/terminal/native/detection.rs`    | Auto-detect terminal via $TERMINAL or DE settings              |
| PID Tracking    | `orchestrator/terminal/native/pid_tracking.rs` | Wrapper script writes PID, handles server-based terminals      |
| Window Ops      | `orchestrator/terminal/native/window_ops.rs`   | Close by title (wmctrl/xdotool on Linux, AppleScript on macOS) |

### Handoff System

| Component    | File                               | Purpose                                                     |
| ------------ | ---------------------------------- | ----------------------------------------------------------- |
| Detector     | `handoff/detector.rs`              | Context threshold monitoring (Yellow=prepare, Red=generate) |
| Generator    | `handoff/generator/mod.rs`         | Builds HandoffContent with state snapshot                   |
| Schema V2    | `handoff/schema/v2.rs`             | Structured YAML frontmatter for machine parsing             |
| Continuation | `orchestrator/continuation/mod.rs` | Resume session with handoff context                         |

### Plan Parsing Pipeline

1. **Extraction** (`plan/parser/extraction.rs`) - Find YAML in `<!-- loom METADATA -->` markers
2. **Validation** (`plan/parser/validation.rs`) - Schema validation, ID uniqueness, dependency resolution
3. **Graph Build** (`plan/graph/mod.rs`) - DAG with cycle detection, topological sort

### Verification System

| Component         | File                          | Purpose                                        |
| ----------------- | ----------------------------- | ---------------------------------------------- |
| Criteria Runner   | `verify/criteria/runner.rs`   | Sequential execution, captures all results     |
| Task Verification | `checkpoints/types.rs`        | FileExists, Contains, Command, OutputSet rules |
| Stage Transitions | `verify/transitions/state.rs` | Atomic status changes with validation          |

### Skills & Completions

- **Skill Index**: `skills/index.rs` - Loads SKILL.md from ~/.claude/skills/, matches triggers
- **Dynamic Completions**: `completions/dynamic/*.rs` - Context-aware tab completion for stages, sessions, plans

### Daemon Protocol

Socket at `.work/orchestrator.sock` with 4-byte length-prefixed JSON:

- **Requests**: SubscribeStatus, SubscribeLogs, Stop, Ping
- **Responses**: StatusUpdate, OrchestrationComplete, LogLine, Pong, Error

### Key Design Patterns

1. **File-Based State** - All state in .work/ as markdown/YAML for git-friendliness
2. **Progressive Merge** - Merge immediately on completion to minimize conflict window
3. **Merge-Before-Complete** - Auto-merge attempted BEFORE marking stage completed
4. **Context Thresholds** - 65% red threshold triggers handoff BEFORE Claude Code's ~75% compaction
5. **Exponential Backoff** - Retry transient failures (crash/timeout) with 30s-300s backoff
6. **Window-Based Kill** - Prefer wmctrl/xdotool over PID for reliable terminal closure
