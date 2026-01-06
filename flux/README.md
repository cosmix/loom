# Flux - Plan-Driven Agent Orchestration CLI

A command-line tool that transforms Claude Code planning into automated,
parallel execution with context tracking and handoff capabilities.

## Overview

Flux automates the manual Claude Code workflow:

1. Create a plan with Claude (in plan mode)
2. Break it down into stages with dependencies
3. Execute stages (parallel where possible, sequential where needed)
4. Verify completion, handle context exhaustion automatically

Flux manages the entire lifecycle - from parsing plan documents to spawning
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

- **Rust toolchain** - 1.70+ (for building Flux)
- **Git** - 2.15+ (worktree support)
- **tmux** - 3.0+ (session management)
- **Claude Code** - Available as `claude` command

## Installation

```bash
cargo install --path .
```

## Usage

### Primary Commands (90% of usage)

#### Initialize Flux for a Plan

```bash
flux init <plan-path>
```

Parses the plan document, extracts stages and dependencies, creates execution
graph, and sets up `.work/` directory structure.

**Example:**

```bash
flux init doc/plans/PLAN-auth.md
```

#### Start Execution

```bash
flux run [--stage <id>] [--manual] [--max-parallel <n>]
```

Creates git worktrees for ready stages, spawns Claude sessions (unless
`--manual`), monitors progress, and triggers dependent stages upon completion.

**Options:**

- `--stage <id>` - Run only a specific stage
- `--manual` - Don't spawn sessions automatically; just prepare signals
- `--max-parallel <n>` - Maximum parallel sessions (default: 4)

**Example:**

```bash
# Run all ready stages automatically
flux run

# Run a specific stage manually
flux run --stage stage-2-api --manual
```

#### Check Status

```bash
flux status
```

Shows plan progress, stage states, session health, and context levels at a glance.

#### Verify a Stage (Human Gate)

```bash
flux verify <stage-id>
```

Runs acceptance criteria, prompts for human approval/rejection, and triggers
dependent stages if approved.

**Example:**

```bash
flux verify stage-1-models
```

#### Resume Failed/Blocked Stage

```bash
flux resume <stage-id>
```

Creates a new session with handoff context, continuing from where the previous
session left off.

**Example:**

```bash
flux resume stage-2-api
```

#### Merge Completed Stage

```bash
flux merge <stage-id>
```

Merges the worktree branch back to main and removes the worktree on success.
If conflicts occur, prints resolution instructions.

**Example:**

```bash
flux merge stage-1-models
```

#### Attach to Running Session

```bash
flux attach <stage-id|session-id>
flux attach list
```

Attaches your terminal to a running Claude session for observation or
intervention. Detach with `Ctrl+B D`.

**Example:**

```bash
# List all attachable sessions
flux attach list

# Attach to a specific stage
flux attach stage-1-models
```

### Secondary Commands (Power Users)

#### Manage Sessions

```bash
flux sessions [list|kill <id>]
```

List active sessions or kill a specific session.

#### Manage Worktrees

```bash
flux worktree [list|clean]
```

List active worktrees or clean up stale worktrees.

#### View/Edit Execution Graph

```bash
flux graph [show|edit]
```

View the dependency graph or manually edit it.

#### Force Stage State

```bash
flux stage <id> [complete|block|reset]
```

Manually transition a stage to a specific state.

## Plan Document Format

Plans are markdown files in `doc/plans/` with embedded YAML metadata:

````markdown
# PLAN: Implement User Authentication

## Overview

[Freeform description of the plan...]

## Stages

[Freeform breakdown of stages...]

---

<!-- FLUX METADATA -->

```yaml
flux:
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

<!-- END FLUX METADATA -->
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

Flux uses git worktrees to provide clean file isolation for parallel stage execution:

```text
project/
├── .git/
├── .work/              # Flux state (in main worktree)
├── .worktrees/         # Parallel execution worktrees
│   ├── stage-1/       # Full checkout for stage 1
│   │   ├── .work/     # Symlink to main .work/
│   │   └── [project files]
│   └── stage-2/
├── doc/plans/
└── src/
```

### Worktree Lifecycle

1. `flux run` detects ready stages (dependencies satisfied)
2. Creates worktree: `git worktree add .worktrees/stage-1 -b flux/stage-1`
3. Session executes in worktree directory
4. On completion, human runs `flux merge stage-1`
5. Flux merges branch and removes worktree
6. Dependent stages become ready and can execute

## Session Management

Flux spawns Claude sessions inside **tmux** sessions, enabling human
observation and intervention.

### Automatic Mode (Default)

```bash
flux run
# Creates: tmux new-session -d -s flux-stage-1 -c .worktrees/stage-1
# Runs: claude (inside tmux session)
```

Sessions run detached in the background.

### Manual Mode

```bash
flux run --manual
# Prepares signals and prints:
# "Stage 1 ready. Start session in .worktrees/stage-1/:
#  cd .worktrees/stage-1 && claude"
```

### Attach/Detach

```bash
flux attach list
# Shows running sessions:
# SESSION          STAGE              STATUS      CONTEXT
# flux-stage-1     stage-1-models     running     45%
# flux-stage-2     stage-2-api        running     23%

flux attach stage-1
# Attaches to tmux session. Detach with Ctrl+B D
```

## Context Exhaustion & Handoffs

When a Claude session reaches 75% context usage:

1. Session creates a handoff document in `.work/handoffs/`
2. Handoff includes: context summary, completed work, remaining tasks (with
   `file:line` refs), key decisions
3. Session updates stage status to `needs_handoff`
4. Session terminates cleanly
5. Flux detects `needs_handoff` and spawns a new session with handoff context

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
