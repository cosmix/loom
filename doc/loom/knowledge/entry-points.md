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

## TUI Module Detail (commands/status/ui/)

Entry point: `run_tui(work_path: &Path)` in tui.rs:638

| File            | Purpose                                                     |
| --------------- | ----------------------------------------------------------- |
| mod.rs          | Public exports: run_tui, GraphWidget, Theme, StatusColors   |
| tui.rs          | TuiApp struct, event loop, rendering functions              |
| theme.rs        | StatusColors constants and Theme style helpers              |
| widgets.rs      | progress_bar(), context_bar(), status_indicator() functions |
| graph_widget.rs | GraphWidget implementing ratatui Widget trait for DAG       |
| layout.rs       | LayoutHelper for responsive terminal layout                 |

Key data structures:

- TuiApp: Terminal backend, running flag, LiveStatus state, spinner, last_error
- LiveStatus: Vec<StageInfo> for each category, compute_levels(), unified_stages()
- UnifiedStage: id, status, merged, timestamps, level, dependencies

## Hook Configuration Detail (fs/permissions/)

Entry point: `ensure_loom_permissions(repo_root: &Path)` in settings.rs:22

| File         | Purpose                                                     |
| ------------ | ----------------------------------------------------------- |
| mod.rs       | Public API exports                                          |
| constants.rs | Embedded hook scripts via include_str!(), permission arrays |
| hooks.rs     | loom_hooks_config(), install_loom_hooks(), configure_hooks  |
| settings.rs  | ensure_loom_permissions(), create_worktree_settings()       |
| trust.rs     | add_worktrees_to_global_trust() for ~/.claude.json          |
| sync.rs      | sync_worktree_permissions() with file locking               |

Hook types:

- PreToolUse: AskUserQuestion -> ask-user-pre.sh (marks WaitingForInput)
- PostToolUse: AskUserQuestion -> ask-user-post.sh (resumes stage)
- Stop: \* -> commit-guard.sh (blocks exit without commit)

## Daemon Server Detail (daemon/server/)

Entry point: `DaemonServer::start(&self)` in lifecycle.rs:51

| File            | Purpose                                               |
| --------------- | ----------------------------------------------------- |
| mod.rs          | Public export of DaemonServer                         |
| core.rs         | DaemonServer struct with paths and shared state       |
| lifecycle.rs    | start(), run_foreground(), run_server(), cleanup()    |
| client.rs       | handle_client_connection() processes Request enum     |
| broadcast.rs    | spawn_log_tailer(), spawn_status_broadcaster()        |
| status.rs       | collect_status() reads stage files, detects worktrees |
| orchestrator.rs | spawn_orchestrator() thread for stage execution       |

Protocol (daemon/protocol.rs):

- Request: SubscribeStatus, SubscribeLogs, Stop, Unsubscribe, Ping
- Response: Ok, Error, StatusUpdate, LogLine, Pong
- StageInfo: id, name, pid, timestamps, worktree_status, status, merged, deps

## CLAUDE.md.template Entry Point

- `CLAUDE.md.template` (project root, 576 lines) - Canonical binding rules template
- Contains 10 Critical Rules + 5 Standard Rules for Claude Code agents
- Key sections: Plan Location, Context Limits, Worktree Isolation, Knowledge Management
- Installation: Prepends timestamp header, writes to `~/.claude/CLAUDE.md`
- Update via `loom self-update` downloads from GitHub releases

## Skill System Entry Points

- `skills/` directory - 60+ skill subdirectories (auth, testing, react, etc.)
- Each skill: `skills/<skill-name>/SKILL.md` (single file per skill)
- Distribution: Downloaded as `skills.zip` from GitHub releases
- Installation target: `~/.claude/skills/`
- Update logic: `loom/src/commands/self_update/mod.rs:229-235`

## Hook Shell Scripts (hooks/)

| Script                 | Event Type            | Purpose                                   |
| ---------------------- | --------------------- | ----------------------------------------- |
| session-start.sh       | PreToolUse (Bash)     | Initialize heartbeat on first tool        |
| post-tool-use.sh       | PostToolUse           | Update heartbeat, enforce commit guard    |
| pre-compact.sh         | PreCompact            | Trigger handoff before context compaction |
| session-end.sh         | SessionEnd            | Handle session completion                 |
| learning-validator.sh  | Stop                  | Validate learnings, blocks exit on damage |
| commit-guard.sh        | Stop                  | Enforce commits in worktrees              |
| ask-user-pre.sh        | PreToolUse (AskUser)  | Mark stage WaitingForInput                |
| ask-user-post.sh       | PostToolUse (AskUser) | Resume stage after input                  |
| prefer-modern-tools.sh | PreToolUse (Bash)     | Guide CLI tool selection                  |
| subagent-stop.sh       | SubagentStop          | Extract learnings from subagents          |

## Hook Registration Entry Points

- `orchestrator/hooks/config.rs:10-28` - HookEvent enum (7 event types)
- `orchestrator/hooks/generator.rs` - Settings file generation for worktrees
- `fs/permissions/constants.rs:1-52` - Embedded scripts via include_str!()
- `fs/permissions/hooks.rs:82-106` - install_loom_hooks() function
- `commands/hooks.rs:19-46` - `loom hooks install` command

## Stage Completion (commands/stage/complete.rs)

- complete() at line 121 - Main dispatcher
- complete_knowledge_stage() at line 31 - No merge path
- resolve_acceptance_dir() at line 466 - Dir resolution

## Acceptance Criteria (verify/criteria/)

- runner.rs:16 - run_acceptance() main entry
- executor.rs:36 - run_single_criterion_with_timeout()
- executor.rs:99 - spawn_shell_command() sh -c
- config.rs - DEFAULT_COMMAND_TIMEOUT (5 min)

## Stage Verify Command

- `loom/src/commands/stage/verify.rs` - Re-verify and complete stages that failed acceptance criteria
