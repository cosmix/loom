# loom - Self-Propelling Agent Orchestration CLI

A command-line tool that transforms Claude Code planning into automated,
parallel execution with context tracking and handoff capabilities.

## Overview

loom automates the manual Claude Code workflow:

1. Create a plan with Claude (in plan mode)
2. Break it down into stages with dependencies
3. Execute stages (parallel where possible, sequential where needed)
4. Verify completion, handle context exhaustion automatically

loom manages the entire lifecycle - from parsing plan documents to spawning
isolated Claude sessions, monitoring progress, and merging completed work.

## Key Features

- **Plan-driven execution** - Go from plan document to execution with 2-3 commands
- **Parallel stage execution** - Stages run in isolated git worktrees without conflicts
- **Automatic dependency resolution** - Stage dependencies are respected automatically
- **Context exhaustion handling** - Automatic handoffs when Claude context fills
- **Human verification gates** - Approve critical stages before triggering dependents
- **Session management** - Attach/detach to running Claude sessions via tmux

## Architecture

### Core Concepts

| Concept        | Description                                 |
| -------------- | ------------------------------------------- |
| **Plan**       | Parent container for stages in `doc/plans/` |
| **Stage**      | Unit of work with dependencies and criteria |
| **Session**    | A Claude Code instance executing a stage    |
| **Assignment** | Stage assignment + context restoration info |
| **Worktree**   | Git worktree for parallel stage isolation   |
| **Handoff**    | Context dump at 75% context threshold       |

### Workspace Structure

```text
.work/
├── config.toml           # Active plan, settings
├── execution-graph.toml  # Parsed stage DAG
├── stages/               # Stage state files
│   ├── stage-1.md       # status, assigned session, progress
│   └── stage-2.md
├── sessions/             # Session state files
│   ├── session-abc.md   # context health, assigned stage
│   └── session-def.md
├── signals/              # Stage assignments for sessions
│   └── session-abc.md   # what to do, context restoration
├── handoffs/             # Context dumps
│   └── stage-1-handoff-001.md
└── worktrees/            # Worktree metadata
    ├── stage-1.toml     # path, branch, session, status
    └── stage-2.toml
```

## Requirements

- **Rust toolchain** - 1.70+ (for building loom)
- **Git** - 2.15+ (worktree support)
- **tmux** - 3.0+ (session management)
- **Claude Code** - Available as `claude` command

## Installation

```bash
cargo install --path .
```

## Usage

### Primary Commands (90% of usage)

#### Initialize loom for a Plan

```bash
loom init [plan-path] [--clean]
```

Parses the plan document, extracts stages and dependencies, creates execution
graph, and sets up `.work/` directory structure.

**Options:**

- `plan-path` - Path to the plan file (optional)
- `--clean` - Clean up stale resources before initialization (removes old
  `.work/`, prunes worktrees, kills orphaned tmux sessions)

**Example:**

```bash
loom init doc/plans/PLAN-auth.md
loom init doc/plans/PLAN-auth.md --clean
```

#### Start Execution

```bash
loom run [--stage <id>] [--manual] [--max-parallel <n>] [--attach] [--foreground]
```

Creates git worktrees for ready stages, spawns Claude sessions (unless
`--manual`), monitors progress, and triggers dependent stages upon completion.
By default, the orchestrator runs in the background.

**Options:**

- `--stage <id>` - Run only a specific stage
- `--manual` - Don't spawn sessions automatically; just prepare signals
- `--max-parallel <n>` - Maximum parallel sessions (default: 4)
- `--attach` - Attach to existing orchestrator session
- `--foreground` - Run orchestrator in foreground (not recommended)

**Example:**

```bash
# Run all ready stages automatically (background)
loom run

# Attach to running orchestrator
loom run --attach

# Run a specific stage manually
loom run --stage stage-2-api --manual
```

#### Check Status

```bash
loom status
```

Shows plan progress, stage states, session health, and context levels at a glance.

#### Verify a Stage (Human Gate)

```bash
loom verify <stage-id>
```

Runs acceptance criteria, prompts for human approval/rejection, and triggers
dependent stages if approved.

**Example:**

```bash
loom verify stage-1-models
```

#### Resume Failed/Blocked Stage

```bash
loom resume <stage-id>
```

Creates a new session with handoff context, continuing from where the previous
session left off.

**Example:**

```bash
loom resume stage-2-api
```

#### Merge Completed Stage

```bash
loom merge <stage-id> [--force]
```

Merges the worktree branch back to main and removes the worktree on success.
If conflicts occur, prints resolution instructions.

**Options:**

- `--force` - Force merge even if stage is not Completed/Verified or has active sessions

**Example:**

```bash
loom merge stage-1-models
loom merge stage-1-models --force
```

#### Attach to Running Session

```bash
loom attach [target]
loom attach list
loom attach all [--gui] [--detach]
```

Attaches your terminal to a running Claude session for observation or
intervention. Detach with `Ctrl+B D`.

**Subcommands:**

- `list` - List all attachable sessions
- `all` - Attach to all running sessions in a unified tmux view
  - `--gui` - Open separate GUI terminal windows instead of tmux session
  - `--detach` - Detach other clients from sessions before attaching

**Example:**

```bash
# List all attachable sessions
loom attach list
loom attach

# Attach to a specific stage
loom attach stage-1-models

# Attach to all sessions
loom attach all
loom attach all --gui
```

### Secondary Commands (Power Users)

#### Manage Sessions

```bash
loom sessions list
loom sessions kill <session-id>
```

List active sessions or kill a specific session.

#### Manage Worktrees

```bash
loom worktree list
loom worktree clean
```

List active worktrees or clean up stale worktrees.

#### View/Edit Execution Graph

```bash
loom graph show
loom graph edit
```

View the dependency graph or manually edit it.

#### Manage Individual Stages

```bash
loom stage complete <stage-id> [--session <id>] [--no-verify]
loom stage block <stage-id> <reason>
loom stage reset <stage-id> [--hard] [--kill-session]
loom stage waiting <stage-id>
loom stage resume <stage-id>
```

Manually transition a stage to a specific state.

**Subcommands:**

- `complete` - Mark a stage as complete (runs acceptance criteria by default)
  - `--session <id>` - Also mark associated session as completed
  - `--no-verify` - Skip acceptance criteria verification
- `block` - Block a stage with a reason
- `reset` - Reset a stage to ready state
  - `--hard` - Also reset worktree to clean state (git reset --hard)
  - `--kill-session` - Kill associated session if running
- `waiting` - Mark a stage as waiting for user input (used by hooks)
- `resume` - Resume a stage from waiting state (used by hooks)

#### Clean Up Resources

```bash
loom clean [--all] [--worktrees] [--sessions] [--state]
```

Clean up loom resources.

**Options:**

- `--all` - Remove all loom resources
- `--worktrees` - Remove only worktrees and their branches
- `--sessions` - Kill only loom tmux sessions
- `--state` - Remove only `.work/` state directory

#### Self Update

```bash
loom self-update
```

Update loom and configuration files.

## Plan Document Format

Plans are markdown files in `doc/plans/` with embedded YAML metadata:

````markdown
# PLAN: Implement User Authentication

## Overview

[Freeform description of the plan...]

## Stages

[Freeform breakdown of stages...]

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1-models
      name: "Data Models"
      description: "Create User, Session, Token models"
      dependencies: []
      parallel_group: "foundation" # stages in same group run in parallel
      acceptance:
        - "cargo test --lib models"
        - "cargo clippy -- -D warnings"
      files:
        - "src/models/*.rs"

    - id: stage-2-api
      name: "API Endpoints"
      description: "Implement auth endpoints"
      dependencies: [stage-1-models]
      acceptance:
        - "cargo test --lib api"
      files:
        - "src/api/*.rs"

    - id: stage-3-frontend
      name: "Frontend Components"
      description: "Build login/signup UI"
      dependencies: [stage-1-models]
      parallel_group: "implementation" # parallel with stage-2
      acceptance:
        - "npm test"
      files:
        - "src/components/*.tsx"
```

<!-- END loom METADATA -->
````

### Metadata Fields

- **id** - Unique stage identifier (kebab-case recommended)
- **name** - Human-readable stage name
- **description** - What the stage accomplishes
- **dependencies** - List of stage IDs that must complete first
- **parallel_group** (optional) - Stages in the same group can run in parallel
- **acceptance** - Shell commands to verify completion
- **files** - Glob patterns for files modified by this stage

## Git Worktree Integration

loom uses git worktrees to provide clean file isolation for parallel stage execution:

```text
project/
├── .git/
├── .work/              # loom state (in main worktree)
├── .worktrees/         # Parallel execution worktrees
│   ├── stage-1/       # Full checkout for stage 1
│   │   ├── .work/     # Symlink to main .work/
│   │   └── [project files]
│   └── stage-2/
├── doc/plans/
└── src/
```

### Worktree Lifecycle

1. `loom run` detects ready stages (dependencies satisfied)
2. Creates worktree: `git worktree add .worktrees/stage-1 -b loom/stage-1`
3. Session executes in worktree directory
4. On completion, human runs `loom merge stage-1`
5. loom merges branch and removes worktree
6. Dependent stages become ready and can execute

## Session Management

loom spawns Claude sessions inside **tmux** sessions, enabling human
observation and intervention.

### Automatic Mode (Default)

```bash
loom run
# Creates: tmux new-session -d -s loom-stage-1 -c .worktrees/stage-1
# Runs: claude (inside tmux session)
```

Sessions run detached in the background.

### Manual Mode

```bash
loom run --manual
# Prepares signals and prints:
# "Stage 1 ready. Start session in .worktrees/stage-1/:
#  cd .worktrees/stage-1 && claude"
```

### Attach/Detach

```bash
loom sessions list
# Shows running sessions:
# SESSION          STAGE              STATUS      CONTEXT
# loom-stage-1     stage-1-models     running     45%
# loom-stage-2     stage-2-api        running     23%

loom attach stage-1
# Attaches to tmux session. Detach with Ctrl+B D
```

## Context Exhaustion & Handoffs

When a Claude session reaches 75% context usage:

1. Session creates a handoff document in `.work/handoffs/`
2. Handoff includes: context summary, completed work, remaining tasks (with
   `file:line` refs), key decisions
3. Session updates stage status to `needs_handoff`
4. Session terminates cleanly
5. loom detects `needs_handoff` and spawns a new session with handoff context

This enables unlimited work on complex stages without manual intervention.

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy

# Format code
cargo fmt
```

## License

MIT
