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

## Verification System

CriterionResult (verify/criteria/result.rs:7-60): Single command result with success, stdout/stderr, exit_code, duration, timed_out.

AcceptanceResult (result.rs:64-110): AllPassed or Failed enum aggregating CriterionResults with pass/fail counts.

## Monitor System

MonitorEvent (monitor/events.rs:7-95): Events include StageCompleted, SessionContextWarning/Critical, SessionCrashed, SessionHung, CheckpointCreated, BudgetExceeded.

ContextHealth (monitor/context.rs:10-17): Green (<50%), Yellow (50-64%), Red (65%+) thresholds for handoff triggers.

RecoveryType (events.rs:98-108): Crash (PID dead), Hung (no heartbeat), ContextRefresh (graceful), Manual (user triggered).

Verification flow: acceptance criteria → stage complete → progressive merge.

## Goal-Backward Verification System

Module at loom/src/verify/goal_backward/:

- mod.rs - Main orchestration (run_goal_backward_verification)
- result.rs - GoalBackwardResult, VerificationGap, GapType enums
- truths.rs - Verify observable behaviors via shell commands
- artifacts.rs - Verify files exist with real implementation
- wiring.rs - Verify critical connections via grep patterns

Flow: StageDefinition (truths/artifacts/wiring) → run_goal_backward_verification() → GoalBackwardResult

Storage: .work/verifications/<stage-id>.json via loom/src/fs/verifications.rs

## Codebase Mapping System

Module at loom/src/map/:

- mod.rs - Exports analyze_codebase and AnalysisResult
- analyzer.rs - Orchestrates all detectors, produces AnalysisResult
- detectors.rs - Project type, dependencies, entry points, structure, conventions, concerns

Writes to: architecture.md, stack.md, conventions.md, concerns.md

## Context Budget Enforcement

Stages define context_budget (1-100%, default 65%, max 75%).
When exceeded, BudgetExceeded event triggers auto-handoff.

Key files:

- loom/src/orchestrator/monitor/events.rs:88-94 - BudgetExceeded event
- loom/src/orchestrator/monitor/detection.rs:242-261 - Budget detection
- loom/src/orchestrator/core/event_handler.rs:242-294 - Handler

## .work Directory Creation

### WorkDir (fs/work_dir.rs:79-316)

- initialize() creates .work/ structure with subdirs: runners, tracks, signals, handoffs, archive, stages, sessions, logs, crashes, checkpoints, task-state
- load() validates existing structure, auto-creates missing dirs
- main_project_root() resolves symlinks to true repo root (critical for worktrees)

## Worktree Symlinks

### Symlink Setup (git/worktree/settings.rs)

Worktrees get three symlinks:

- .work -> ../../.work (shared state)
- .claude/CLAUDE.md -> ../../../.claude/CLAUDE.md (instructions)
- CLAUDE.md -> ../../CLAUDE.md (project guidance)

Functions: ensure_work_symlink():16, setup_claude_directory():40, setup_root_claude_md():85
All use relative paths for portability. .claude/ is real dir for session-specific hooks.

## .work Directory Structure (Updated 2026-01-29)

WorkDir (fs/work_dir.rs) creates .work/ with subdirectories:
signals, handoffs, archive, stages, sessions, crashes.

REMOVED: runners, tracks, logs, checkpoints, task-state (dead code cleanup).

## Review Findings - Layering Violations (2026-01-29)

The following architecture layering violations were identified and require refactoring to restore proper dependency direction.

### Critical Violations

1. **daemon imports commands** - daemon/server/orchestrator.rs imports mark_plan_done_if_all_merged from commands/run
   - Fix: Move to fs/plan_lifecycle.rs

2. **orchestrator imports commands** - orchestrator/core/merge_handler.rs imports check_merge_state from commands/status/merge_status
   - Fix: Move to git/merge/status.rs

### More Violations

1. **git/worktree imports orchestrator** - git/worktree/settings.rs imports hook configuration from orchestrator/hooks
   - Fix: Extract hooks/ as top-level module

2. **models imports plan/schema** - Core types WiringCheck and StageType defined in plan/schema but used by models
   - Fix: Move type definitions to models/, keep re-exports in plan/schema

### Correct Dependency Direction

commands/ → orchestrator/ → models/ (top layers)
    ↓             ↓              ↓
daemon/    git/          plan/schema/ (middle layers)
              ↓
            fs/ (bottom layer)

CRITICAL RULE: Lower layers MUST NEVER import from higher layers. This violation creates maintenance hazard.

## Worktree Isolation

Loom enforces isolation at multiple layers to enable safe parallel stage execution.

| Layer | Implementation | Purpose |
|-------|----------------|---------|
| Git | Separate worktrees with branches | File isolation |
| Sandbox | settings.local.json | Permissions |
| Signal | Stable prefix rules | Instructions |
| Hooks | Shell scripts | Enforcement |

### Git Worktree Layer (git/worktree/)

Worktrees at `.worktrees/<stage-id>/` with branch `loom/<stage-id>`.

**Symlinks for shared state:**

- `.work` -> `../../.work` (orchestration state)
- `.claude/CLAUDE.md` -> `../../../.claude/CLAUDE.md` (instructions)
- Root `CLAUDE.md` -> `../../CLAUDE.md` (project guidance)

**Key files:**

- `git/worktree/operations.rs` - create_worktree(), get_or_create_worktree()
- `git/worktree/settings.rs` - ensure_work_symlink(), setup_claude_directory()

.claude/ is a real directory (not symlink) to allow session-specific settings.json.

### Sandbox Layer (sandbox/)

Generates Claude Code `settings.local.json` with permission boundaries.

**MergedSandboxConfig** (config.rs) merges plan + stage configs:

- `filesystem.deny_read/deny_write/allow_write` - File access
- `network.allowed_domains` - Web access
- `excluded_commands` - Blocked CLI commands

**Special stage types:** Knowledge and IntegrationVerify stages auto-add `doc/loom/knowledge/**` to allow_write.

**Key files:**

- `sandbox/config.rs` - merge_config(), expand_paths()
- `sandbox/settings.rs` - generate_settings_json(), write_settings()

### Signal Isolation Layer (orchestrator/signals/cache.rs)

Two stable prefixes with explicit isolation rules:

**generate_stable_prefix()** for worktree stages:

- Worktree Context header with self-contained signal claim
- Isolation Boundaries (STRICT): CONFINED to worktree
- Path Boundaries: ALLOWED (., .work) vs FORBIDDEN (../.., absolute)

Git Staging warnings: Never use bulk staging. Subagent restrictions: Never commit or complete stage. Binary usage: Use loom from PATH only.

generate_knowledge_stable_prefix() for knowledge stages: NO WORKTREE, NO COMMITS, NO MERGING. EXPLORATION FOCUS with knowledge update commands.

### Hooks Enforcement Layer (hooks/)

Shell scripts that enforce isolation at runtime via Claude Code hooks.

**HookEvent types** (hooks/config.rs): SessionStart, PostToolUse, PreCompact, SessionEnd, Stop, PreferModernTools.

**Key enforcement hooks (hooks/*.sh):**

commit-guard.sh (Stop): Blocks exit if uncommitted changes or stage incomplete. Detects worktree via path or branch prefix. Returns JSON with blocking reason.

commit-filter.sh (PreToolUse:Bash): Blocks forbidden patterns. 1) Subagent git operations via LOOM_MAIN_AGENT_PID comparison. 2) Claude/Anthropic Co-Authored-By attribution.

### Subagent Isolation

Three-layer defense against subagent conflicts:

1. Documentation: CLAUDE.md.template Rule 5 with subagent restrictions
2. Signal injection: cache.rs stable prefix includes subagent warnings
3. Hook enforcement: commit-filter.sh blocks git ops from subagents

**Detection mechanism:** Wrapper script exports LOOM_MAIN_AGENT_PID. Hook compares PPID to this value. Main agent: PPID matches. Subagent: PPID differs.

**Environment variables for hooks:** LOOM_STAGE_ID, LOOM_SESSION_ID, LOOM_WORK_DIR, LOOM_MAIN_AGENT_PID. Set via settings.json env section and wrapper script.

## CodeReview Stage Type

New stage type for code review. YAML: stage_type: code-review
- Enum: StageType::CodeReview (models/stage/types.rs:26)
- Sandbox: Special handling (sandbox/config.rs:69-73)
- Validation: Exempt from goal-backward checks (validation.rs:362, 471)

## CodeReview Stage Type (continued)

- Signals: generate_code_review_stable_prefix() (signals/format/mod.rs:72)
- Skill: skills/code-review/SKILL.md documents usage
- Detection: Requires explicit stage_type field in plan YAML. ID/name detection happens after validation.

## CodeReview Test Verification Entry
