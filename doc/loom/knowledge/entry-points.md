# Entry Points

> Key files agents should read first to understand the codebase.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [architecture.md](architecture.md) for system overview, [patterns.md](patterns.md) for design patterns.

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

> **Pattern details:** See [patterns.md § Signal Generation Patterns](patterns.md#signal-generation-patterns) for 4-section structure and 6 signal types.

Key files: `orchestrator/signals/generate.rs`, `format.rs`, `crud.rs`, `cache.rs`

### Git Operations

> **Git commands:** See [conventions.md § Git Operations](conventions.md#git-operations) for command patterns.

| Area      | Key Files                               | Purpose                                  |
| --------- | --------------------------------------- | ---------------------------------------- |
| Worktrees | `git/worktree/operations.rs`, `base.rs` | Create worktrees, resolve base branch    |
| Branches  | `git/branch/operations.rs`, `naming.rs` | Branch naming, ancestry checks           |
| Merge     | `git/merge.rs`                          | Merge operations, conflict handling      |
| Cleanup   | `git/cleanup/batch.rs`                  | Post-merge cleanup                       |

### File System State Structure

> **Full directory layout:** See [architecture.md § Directory Structure](architecture.md#directory-structure) for complete .work/ structure.

Key subdirectories: config.toml, stages/, sessions/, signals/, handoffs/, memory/, crashes/, heartbeat/.

### Data Models

> **State machines:** See [patterns.md § State Machine Pattern](patterns.md#state-machine-pattern) for full diagrams and transition rules.

**Stage** (11 states): WaitingForDeps → Queued → Executing → terminal states

**Session** (6 states): Spawning → Running → terminal states

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

> **Full pattern documentation:** See [patterns.md](patterns.md) for detailed explanations with code examples.

1. **File-Based State** - All state in .work/ as markdown/YAML for git-friendliness
2. **Progressive Merge** - Merge immediately on completion to minimize conflict window
3. **Merge-Before-Complete** - Auto-merge attempted BEFORE marking stage completed
4. **Context Thresholds** - 65% red threshold triggers handoff BEFORE Claude Code's ~75% compaction
5. **Exponential Backoff** - Retry transient failures (crash/timeout) with 30s-300s backoff
6. **Window-Based Kill** - Prefer wmctrl/xdotool over PID for reliable terminal closure

## New CLI Commands

loom verify `<stage-id>` [--suggest]
  Entry: loom/src/commands/verify.rs
  Runs goal-backward verification (truths, artifacts, wiring)

loom map [--deep] [--focus `<area>`] [--overwrite]
  Entry: loom/src/commands/map.rs
  Analyzes codebase structure, writes to knowledge files

## Merge Verification Entry Points

- loom/src/orchestrator/core/merge_handler.rs - Auto-merge orchestration
- loom/src/commands/status/merge_status.rs - Merge state checking  
- loom/src/git/merge.rs - Git merge operations
- loom/src/git/branch.rs - is_ancestor_of() for ancestry checks

## Signal Generation Entry Points

- loom/src/orchestrator/signals/cache.rs - Stable prefix generation (agent rules)
- loom/src/orchestrator/signals/format.rs - Full signal formatting
- loom/src/orchestrator/signals/generate.rs - Signal file creation

## Shell Completions Dynamic Routing

New completion handlers added:

- memory session: Lists session IDs for memory commands
- memory entry-type: Shows note/decision/question/all for promote
- memory target: Shows knowledge file types for promote targets  
- checkpoint status: Shows pending/active/completed/all statuses
- knowledge files: Shows all 7 knowledge file types (architecture, entry-points, patterns, conventions, mistakes, stack, concerns)

## Git Hook Entry Points

### Hook Installation

- commands/hooks.rs - loom hooks install/list commands
- fs/permissions/hooks.rs - install_loom_hooks(), loom_hooks_config()
- fs/permissions/constants.rs - embedded hook scripts (include_str! from hooks/*.sh)

### Hook Event System

- orchestrator/hooks/config.rs - HookEvent enum, HooksConfig struct
- orchestrator/hooks/events.rs - HookEventLog, log_hook_event()
- orchestrator/hooks/generator.rs - setup_hooks_for_worktree()

## Process Module

- src/process/mod.rs - PID liveness checking via libc::kill(pid, 0)
- Re-exported in pid_tracking.rs as check_pid_alive

## Completions Module

- src/completions/mod.rs - Public exports
- src/completions/generator.rs - Static shell completion generation
- src/completions/dynamic/mod.rs - Dynamic context-aware completions

## Diagnosis Module

- src/commands/diagnose.rs - Stage failure diagnosis command

## Agent Teams Integration Points

### Settings System

- fs/permissions/settings.rs:35 - ensure_loom_permissions() entry
- fs/permissions/settings.rs:131 - create_worktree_settings() entry
- fs/permissions/constants.rs:79 - LOOM_PERMISSIONS constant
- fs/permissions/constants.rs:94 - LOOM_PERMISSIONS_WORKTREE constant
- fs/permissions/hooks.rs:14 - loom_hooks_config() returns hook JSON

## Schema Transformation Points

- plan/schema/types.rs:209 - StageDefinition struct (YAML input)
- models/stage/types.rs:86 - Stage struct (runtime model)
- commands/init/plan_setup.rs:327 - create_stage_from_definition() converter
- commands/init/plan_setup.rs:286 - detect_stage_type() pattern matching
- verify/goal_backward/mod.rs:27 - run_goal_backward_verification() entry

## Signal Generation Entry Points (Extended)

Stable prefixes (cache.rs): generate_stable_prefix(), generate_knowledge_stable_prefix(),
generate_code_review_stable_prefix(), generate_integration_verify_stable_prefix().

Sections (format/sections.rs): format_semi_stable_section(), format_dynamic_section(),
format_recitation_section(). Assembly (format/mod.rs): format_signal_with_metrics().

## Stage Timing Code Paths

### Setting Timestamps

- models/stage/methods.rs:163-169 - try_mark_executing() sets started_at
- models/stage/methods.rs:128-138 - try_complete() sets completed_at + duration_secs
- models/stage/methods.rs:218-229 - try_complete_merge() also sets timing
- commands/stage/state.rs:44-49 - reset() clears all timing fields

### Duration Formatting

- utils.rs:21-29 - format_elapsed() compact (30s, 1m30s, 1h1m)
- utils.rs:40-52 - format_elapsed_verbose() with spaces

### Status Display Timing

- daemon/server/status.rs:257-274 - get_stage_started_at() from frontmatter
- daemon/server/status.rs:279-288 - get_stage_completed_at() from frontmatter
- daemon/server/status.rs:323-433 - collect_completion_summary()
- commands/status/render/summary.rs:20-84 - print_completion_summary()
- commands/status/render/completion.rs:46-135 - render_completion_screen()
- commands/status/ui/tui/renderer.rs:83-89 - TUI stage duration calc

### Retry/Recovery Code Path

- orchestrator/monitor/detection.rs - PID liveness + heartbeat checks
- orchestrator/core/crash_handler.rs:14-109 - handle_session_crashed()
- orchestrator/retry.rs - classify_failure(), calculate_backoff()
- orchestrator/core/recovery.rs:36-169 - sync blocked stages for retry
- orchestrator/monitor/failure_tracking.rs:59-97 - consecutive failure escalation
