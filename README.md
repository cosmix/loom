# Loom

Loom is an agent orchestration system for Claude Code. It coordinates AI agent sessions across git worktrees, enabling parallel task execution with automatic crash recovery and context handoffs. When context runs low, agents externalize their state and signal fresh sessions to continue seamlessly.

## The Problem

AI agent sessions hit hard limits:

- **Context exhaustion** - Long tasks exceed the context window
- **Lost state** - Work-in-progress vanishes when sessions end
- **Manual handoffs** - Resuming requires re-explaining context and decisions
- **No coordination** - Multiple agents cannot easily pass work between each other

## The Solution

Loom solves these problems with three integrated components:

| Component    | Purpose                                          |
| ------------ | ------------------------------------------------ |
| **loom CLI** | Manages persistent work state across sessions    |
| **Agents**   | 3 specialized AI agents (2 Opus, 1 Sonnet)       |
| **Skills**   | Reusable knowledge modules loaded dynamically    |

Together, they implement the **Signal Principle**: *"If you have a signal, answer it."* Agents check for pending signals on startup and resume work automatically.

## Quick Start

1. Clone the repo
2. `bash ./dev-install.sh`
3. Then:
   
```bash

# Initialize and run
cd /path/to/project
loom init doc/plans/my-plan.md   # Initialize with a plan
loom run                          # Start daemon and execute stages
loom status                       # Live dashboard (Ctrl+C to exit)
loom stop                         # Stop the daemon
```

Loom creates git worktrees for parallel stages and spawns Claude Code sessions in terminal windows automatically.

## Core Concepts

| Concept      | Description                                        |
| ------------ | -------------------------------------------------- |
| **Plan**     | Parent container for stages, lives in `doc/plans/` |
| **Stage**    | A unit of work within a plan, with dependencies    |
| **Session**  | A Claude Code instance executing a stage           |
| **Worktree** | Git worktree for parallel stage isolation          |
| **Handoff**  | Context dump when session exhausts context         |

### The Signal Principle

On every session start, agents:

1. Check `.work/signals/` for pending work matching their role
2. If a signal exists, load context and execute immediately
3. If no signal, ask the user what to do

This creates continuity across sessions without manual intervention.

### Context Management

Agents monitor context usage and create handoffs before hitting limits:

| Level  | Usage  | Action                          |
| ------ | ------ | ------------------------------- |
| Green  | < 50%  | Normal operation                |
| Yellow | 50-64% | Prepare handoff soon            |
| Red    | >= 65% | Create handoff, start fresh     |

Stages can customize their budget via `context_budget` (1-100%, default: 65%).

## Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                        loom run                             │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              Daemon Process (background)                    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Unix Socket - CLI connections, live updates        │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Orchestrator Loop (every 5s)                       │    │
│  │  - Poll stage/session files                         │    │
│  │  - Start ready stages in terminal windows           │    │
│  │  - Detect crashed sessions, generate handoffs       │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                           │
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
      ┌─────────┐    ┌─────────┐    ┌─────────┐
      │ Terminal│    │ Terminal│    │ Terminal│
      │ stage-1 │    │ stage-2 │    │ stage-3 │
      └─────────┘    └─────────┘    └─────────┘
```

### State Directory

All state lives in `.work/` as structured files:

```text
project/
├── .work/                    # Loom state (version controlled)
│   ├── config.toml           # Active plan, settings
│   ├── stages/               # Stage state files
│   ├── sessions/             # Session tracking
│   ├── signals/              # Agent assignments
│   └── handoffs/             # Context dumps
├── .worktrees/               # Git worktrees for parallel stages
└── doc/plans/                # Plan documents
```

## Installation

### Quick Install (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/cosmix/loom/main/install.sh | bash
```

### Manual Install

```bash
git clone https://github.com/cosmix/loom.git
cd loom
bash install.sh
```

### What Gets Installed

| Location              | Contents                              |
| --------------------- | ------------------------------------- |
| `~/.claude/agents/`   | 3 specialized AI agents               |
| `~/.claude/skills/`   | Reusable knowledge modules            |
| `~/.claude/CLAUDE.md` | Orchestration rules and configuration |
| `~/.local/bin/loom`   | Loom CLI binary                       |

### Shell Completions

```bash
# Bash
eval "$(loom completions bash)"

# Zsh
eval "$(loom completions zsh)"

# Fish
loom completions fish > ~/.config/fish/completions/loom.fish
```

## CLI Reference

### Primary Commands

```bash
loom init <plan-path> [--clean]       # Initialize with a plan
loom run [--stage <id>] [--watch]     # Execute stages (starts daemon)
loom status [--live]                  # Live dashboard
loom stop                             # Stop the daemon
loom resume <stage-id>                # Resume from handoff
loom merge <stage-id>                 # Merge completed stage to main
loom diagnose <stage-id>              # Diagnose a failed stage
```

### Stage Management

```bash
loom stage complete <stage-id>        # Mark stage complete
loom stage block <stage-id> <reason>  # Block with reason
loom stage reset <stage-id>           # Reset to ready state
loom stage hold <stage-id>            # Prevent auto-execution
loom stage release <stage-id>         # Allow held stage to execute
loom stage skip <stage-id>            # Skip a stage
loom stage retry <stage-id>           # Retry a blocked stage
```

### Knowledge Management

```bash
loom knowledge init                   # Initialize knowledge directory
loom knowledge show [file]            # Show knowledge
loom knowledge update <file> <text>   # Append to knowledge file

loom memory note <text>               # Record a note
loom memory decision <text>           # Record a decision
loom memory promote <type> <target>   # Promote to permanent knowledge
```

### Verification

```bash
loom verify <stage-id> [--suggest]    # Verify stage outcomes
loom map [--deep] [--focus <area>]    # Map codebase to knowledge files
```

### Sandbox

```bash
loom sandbox suggest                  # Auto-detect project type and suggest config
```

### Utilities

```bash
loom sessions list                    # List active sessions
loom sessions kill <id>               # Kill a session
loom worktree list                    # List worktrees
loom worktree clean                   # Clean unused worktrees
loom clean [--all]                    # Clean up resources
loom self-update                      # Update loom
```

## Plan Format

Plans live in `doc/plans/` with embedded YAML metadata:

````markdown
# PLAN-0001: Feature Name

## Problem Statement
...

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage Name"
      description: "What this stage accomplishes"
      dependencies: []
      acceptance:
        - "cargo test"
      files:
        - "src/**/*.rs"

    - id: stage-2
      name: "Dependent Stage"
      dependencies: ["stage-1"]
      acceptance:
        - "cargo test"
```

<!-- END loom METADATA -->
````

### Stage Fields

| Field            | Required | Description                                      |
| ---------------- | -------- | ------------------------------------------------ |
| `id`             | Yes      | Unique identifier (kebab-case)                   |
| `name`           | Yes      | Human-readable name                              |
| `description`    | Yes      | What this stage accomplishes                     |
| `dependencies`   | No       | Stage IDs that must complete first               |
| `acceptance`     | No       | Shell commands to verify completion              |
| `files`          | No       | Glob patterns for modified files                 |
| `context_budget` | No       | Max context % before handoff (default: 65)       |
| `truths`         | No       | Observable behaviors to verify                   |
| `artifacts`      | No       | Files that must exist with real implementation   |
| `wiring`         | No       | Critical connections to verify                   |

### Goal-Backward Verification

Standard tests don't guarantee a feature works end-to-end. Goal-backward verification validates **outcomes**:

```yaml
truths:
  - "curl -sf localhost:8080/login"          # Feature is reachable
artifacts:
  - "src/auth/*.rs"                          # Real code exists
wiring:
  - source: "src/main.rs"
    pattern: "use auth::"                    # Module is imported
```

Run `loom verify <stage-id>` to check all layers.

### Sandbox Configuration

Loom integrates with Claude Code's sandboxing to control agent permissions:

```yaml
loom:
  version: 1
  sandbox:
    enabled: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**"]
      deny_write: [".work/stages/**"]
      allow_write: ["src/**"]
    network:
      allowed_domains: ["github.com", "crates.io"]
```

**Get project-specific suggestions:**

```bash
loom sandbox suggest
```

This auto-detects your project type (Rust, Node, Python) and outputs recommended domain allowlists.

**Key sandbox options:**

| Option | Description |
|--------|-------------|
| `filesystem.deny_read` | Paths agents cannot read (e.g., SSH keys, credentials) |
| `filesystem.deny_write` | Paths agents cannot write |
| `filesystem.allow_write` | Exceptions to write restrictions |
| `network.allowed_domains` | Domains agents can access (empty = no network) |
| `excluded_commands` | Commands exempt from sandboxing (default: `["loom"]`) |

Stages can override plan-level settings with their own `sandbox` block. Knowledge and integration-verify stages automatically get write access to `doc/loom/knowledge/**`.

## Agent Hierarchy

| Agent                      | Model  | Purpose                                              |
| -------------------------- | ------ | ---------------------------------------------------- |
| `senior-software-engineer` | Opus   | Architecture, design patterns, complex debugging     |
| `software-engineer`        | Sonnet | Feature implementation, bug fixes, tests             |
| `code-reviewer`            | Opus   | Read-only code review, security review, architecture |

## Skills Library

Skills are knowledge modules loaded dynamically. Key categories:

| Category       | Examples                                              |
| -------------- | ----------------------------------------------------- |
| Languages      | `python`, `golang`, `rust`, `typescript`              |
| Code Quality   | `code-review`, `refactoring`, `testing`               |
| Infrastructure | `docker`, `kubernetes`, `terraform`, `ci-cd`          |
| Security       | `security-audit`, `threat-model`, `auth`              |
| Architecture   | `event-driven`, `feature-flags`, `background-jobs`    |
| Observability  | `logging-observability`, `grafana`, `prometheus`      |

## Customization

### Adding Agents

Create `~/.claude/agents/my-agent.md`:

```markdown
---
name: my-agent
description: What this agent does.
tools: Read, Edit, Write, Glob, Grep, Bash
model: sonnet
---

Your agent's system prompt here.
```

### Adding Skills

Create `~/.claude/skills/my-skill/SKILL.md`:

```markdown
---
name: my-skill
description: What this skill does.
allowed-tools: Read, Grep, Glob
---

# My Skill

## Instructions
...
```

## Further Reading

- [Claude Code Documentation](https://code.claude.com/docs/en/overview)
- [Claude Code Best Practices](https://www.anthropic.com/engineering/claude-code-best-practices)

## License

MIT
