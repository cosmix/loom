# Architecture

> High-level component relationships, data flow, and module dependencies.
> This file is append-only - agents add discoveries, never delete.

## Project Overview

Loom is a Rust CLI (~15K lines) for orchestrating parallel Claude Code sessions across git worktrees. It enables concurrent task execution with automatic crash recovery, context handoffs, and progressive merging.

## Directory Structure

```text
loom/                          # Rust crate root
├── Cargo.toml                 # Dependencies: clap, serde, tokio, anyhow, etc.
├── src/
│   ├── main.rs                # CLI entry point (clap command definitions)
│   ├── lib.rs                 # Module exports
│   │
│   ├── commands/              # CLI command implementations (~4K lines)
│   │   ├── init/              # Plan initialization (plan_setup.rs, execute.rs)
│   │   ├── run/               # Daemon startup (foreground.rs, config_ops.rs, graph_loader.rs)
│   │   ├── stage/             # Stage management (complete.rs, criteria_runner.rs, session.rs)
│   │   ├── status/            # Status display (display/, data/, formatters)
│   │   ├── merge/             # Manual merge operations
│   │   ├── memory/            # Session memory (handlers.rs, promote.rs)
│   │   ├── knowledge/         # Knowledge base management
│   │   ├── track/             # Progress tracking
│   │   ├── runner/            # Runner management
│   │   └── *.rs               # Other commands (stop, resume, diagnose, etc.)
│   │
│   ├── daemon/                # Background daemon (~1.5K lines)
│   │   └── server/
│   │       ├── core.rs        # Daemon core logic, PID management
│   │       ├── lifecycle.rs   # Socket binding, accept loop, shutdown
│   │       ├── protocol.rs    # IPC message types (Status, Stop, Subscribe)
│   │       ├── status.rs      # Status response building
│   │       ├── client.rs      # Client connection handling
│   │       └── orchestrator.rs # Orchestrator spawning
│   │
│   ├── orchestrator/          # Core orchestration engine (~4K lines)
│   │   ├── core/
│   │   │   ├── orchestrator.rs # Main polling loop (5s interval)
│   │   │   ├── stage_executor.rs # Stage lifecycle management
│   │   │   ├── persistence.rs  # Stage/session file I/O
│   │   │   └── recovery.rs     # Crash recovery logic
│   │   ├── terminal/
│   │   │   ├── backend.rs      # TerminalBackend trait
│   │   │   └── native/         # OS-specific spawning (Linux/macOS)
│   │   ├── monitor/
│   │   │   ├── core.rs         # Session health monitoring
│   │   │   ├── handlers.rs     # Event handlers for state changes
│   │   │   └── failure_tracking.rs # Failure detection and reporting
│   │   ├── signals/
│   │   │   ├── generate.rs     # Signal file generation (Manus format)
│   │   │   ├── crud.rs         # Signal CRUD operations
│   │   │   ├── merge.rs        # Merge signal generation
│   │   │   └── recovery.rs     # Recovery signal generation
│   │   ├── continuation/       # Context handoff management
│   │   │   ├── session_io.rs   # Session serialization
│   │   │   └── yaml_parse.rs   # YAML frontmatter parsing
│   │   ├── progressive_merge/  # Merge orchestration
│   │   └── auto_merge.rs       # Automatic merge execution
│   │
│   ├── models/                 # Domain models (~1K lines)
│   │   ├── stage/
│   │   │   ├── types.rs        # Stage struct, StageStatus enum
│   │   │   ├── transitions.rs  # State machine validation
│   │   │   └── methods.rs      # Stage operations
│   │   └── session/
│   │       ├── types.rs        # Session struct, SessionStatus enum
│   │       └── methods.rs      # Session operations
│   │
│   ├── plan/                   # Plan parsing (~1.5K lines)
│   │   ├── parser.rs           # Markdown plan document parser
│   │   ├── schema/
│   │   │   ├── types.rs        # PlanMetadata, StageDefinition structs
│   │   │   └── validation.rs   # Plan validation rules
│   │   └── graph/
│   │       └── builder.rs      # ExecutionGraph DAG construction
│   │
│   ├── fs/                     # File system operations (~500 lines)
│   │   ├── work_dir.rs         # WorkDir abstraction for .work/
│   │   ├── knowledge.rs        # Knowledge file operations
│   │   └── memory.rs           # Session memory operations
│   │
│   ├── git/                    # Git operations (~800 lines)
│   │   ├── worktree/
│   │   │   ├── base.rs         # Worktree creation/deletion
│   │   │   └── operations.rs   # Branch operations
│   │   └── merge/              # Merge operations
│   │
│   ├── verify/                 # Acceptance criteria (~600 lines)
│   │   ├── criteria/
│   │   │   └── mod.rs          # Criteria execution engine
│   │   └── transitions/        # Stage transition verification
│   │
│   ├── parser/                 # Shared parsing utilities
│   │   └── frontmatter.rs      # YAML frontmatter extraction (CANONICAL)
│   │
│   └── validation.rs           # Input validation (IDs, names, etc.)
│
├── doc/
│   ├── plans/                  # Loom execution plans (PLAN-*.md)
│   └── loom/
│       └── knowledge/          # Persistent knowledge base
│
└── .work/                      # Runtime state (gitignored)
    ├── config.toml             # Active plan reference
    ├── stages/*.md             # Stage state files
    ├── sessions/*.md           # Session tracking
    ├── signals/*.md            # Agent assignment signals
    ├── handoffs/*.md           # Context handoff records
    ├── orchestrator.sock       # Unix socket for IPC
    └── orchestrator.pid        # Daemon PID file
```

## Core Abstractions

### ExecutionGraph (plan/graph/builder.rs)

DAG of stages with dependency tracking. Determines execution order and parallelism.

```text
                    ┌─────────────────┐
                    │ ExecutionGraph  │
                    ├─────────────────┤
                    │ stages: Vec     │
                    │ dependencies    │
                    │ get_ready()     │──► Returns stages with all deps satisfied
                    └─────────────────┘
```

### Stage State Machine (models/stage/types.rs)

```text
WaitingForDeps ──► Queued ──► Executing ──► Completed
       │             │            │             │
       │             │            ▼             ▼
       │             │        Blocked      Verified
       │             │            │
       │             ▼            ▼
       └────────► NeedsHandoff ◄──┘
```

Transitions validated in `models/stage/transitions.rs`. Direct status assignment bypasses validation (used in recovery).

### Session Lifecycle (models/session/types.rs)

```text
Initializing ──► Running ──► Completed
                    │
                    ▼
              Crashed/Terminated
```

Sessions track: PID, terminal window ID, context usage %, timestamps.

### TerminalBackend Trait (orchestrator/terminal/backend.rs)

Abstraction for spawning Claude Code in terminal windows:

- `NativeBackend`: Uses gnome-terminal, konsole, or xterm on Linux; Terminal.app on macOS
- Tracks PIDs for crash detection

## Data Flow

### Plan Execution Flow

```text
1. User runs: loom init doc/plans/PLAN-foo.md
   └─► Parses plan, creates .work/, writes stage files

2. User runs: loom run
   └─► Spawns daemon (or runs foreground)
       └─► Daemon starts orchestrator loop

3. Orchestrator loop (every 5s):
   ├─► Load all stage files from .work/stages/
   ├─► Build ExecutionGraph
   ├─► Find ready stages (deps satisfied, not executing)
   ├─► For each ready stage:
   │   ├─► Create git worktree (.worktrees/<stage-id>/)
   │   ├─► Generate signal file (.work/signals/<session-id>.md)
   │   ├─► Spawn terminal with Claude Code
   │   └─► Update stage status to Executing
   └─► Check running sessions for crashes/completion

4. Claude Code agent:
   ├─► Reads signal file for assignment
   ├─► Executes tasks in worktree
   ├─► Runs: loom stage complete <stage-id>
   └─► Stage status ──► Completed

5. After all stages complete:
   └─► Progressive merge into main branch
```

### IPC Protocol (daemon/server/protocol.rs)

Daemon listens on Unix socket `.work/orchestrator.sock`:

```text
Client ──► DaemonMessage::Status ──► StatusResponse { stages, sessions, ... }
Client ──► DaemonMessage::Stop   ──► Daemon shutdown
Client ──► DaemonMessage::Subscribe ──► Stream of status updates
```

## Key Patterns

### File-Based State Persistence

All state stored as markdown files with YAML frontmatter. Git-friendly, crash-recoverable, human-readable.

```yaml
---
id: "stage-1"
status: "Executing"
started_at: "2024-01-15T10:30:00Z"
---
# Stage: stage-1
...
```

### Manus Signal Format

Signal files use the "Manus" format optimized for LLM KV-cache:

- Static preamble (cacheable)
- Dynamic assignment section
- Context restoration hints

### Trait-Based Composition

Orchestrator uses traits for extensibility:

- `Persistence`: Stage/session file I/O
- `Recovery`: Crash recovery strategies
- `EventHandler`: State change reactions
- `StageExecutor`: Stage lifecycle management

### Progressive Merge

Stages merged in dependency order with invariant: a stage can only merge after all its dependencies have `merged: true`.

## File Ownership Map

| Directory             | Owner Module                     | Purpose              |
| --------------------- | -------------------------------- | -------------------- |
| `.work/stages/`       | orchestrator/core/persistence.rs | Stage state          |
| `.work/sessions/`     | orchestrator/core/persistence.rs | Session state        |
| `.work/signals/`      | orchestrator/signals/            | Agent assignments    |
| `.work/handoffs/`     | orchestrator/continuation/       | Context dumps        |
| `.work/config.toml`   | commands/init/, commands/run/    | Plan reference       |
| `.worktrees/`         | git/worktree/                    | Isolated workspaces  |
| `doc/loom/knowledge/` | fs/knowledge.rs                  | Persistent learnings |

## Critical Paths

### Stage Completion (commands/stage/complete.rs)

1. Validate caller is in correct worktree
2. Run acceptance criteria
3. Update stage status to Completed
4. Trigger merge if all deps merged

### Crash Recovery (orchestrator/core/recovery.rs)

1. Detect stale session (PID dead, no heartbeat)
2. Generate crash report with git diff
3. Mark stage as Blocked or NeedsHandoff
4. Generate recovery signal for next session

### Acceptance Criteria (verify/criteria/mod.rs)

1. Execute shell commands in worktree
2. Capture stdout/stderr
3. Check exit codes
4. Report pass/fail with output

## Dependencies (Cargo.toml)

| Crate              | Purpose                            |
| ------------------ | ---------------------------------- |
| clap               | CLI argument parsing               |
| serde + serde_yaml | Serialization                      |
| anyhow             | Error handling                     |
| tokio              | Async runtime (daemon)             |
| toml               | Config file parsing                |
| chrono             | Timestamps                         |
| minisign           | Self-update signature verification |

## Security Model

- **Stage IDs validated**: No path traversal (validation.rs:68-77)
- **Acceptance criteria**: Runs arbitrary shell commands (trusted model)
- **Socket permissions**: Currently default (NEEDS FIX: should be 0600)
- **Self-update**: Signature verification with minisign
