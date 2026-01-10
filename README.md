# Loom

Loom is a self-propelling agent orchestration system for Claude Code.
It combines a state management CLI, specialized AI agents, and reusable skills
to create workflows that survive context exhaustion, session crashes, and
manual handoffs. When context runs low, agents externalize their state and
signal fresh sessions to continue seamlessly.

## The Problem

AI agent sessions hit hard limits:

- **Context exhaustion** - Long tasks exceed the context window, forcing lossy
  summarization or manual restarts
- **Lost state** - When sessions end, work-in-progress vanishes unless manually
  documented
- **Manual handoffs** - Resuming work requires re-explaining context,
  decisions, and next steps
- **No coordination** - Multiple agents cannot easily pass work between each
  other

## The Solution

Loom solves these problems with three integrated components:

| Component    | Purpose                                          |
| ------------ | ------------------------------------------------ |
| **loom CLI** | Manages persistent work state across sessions    |
| **Agents**   | 19 specialized AI agents organized by domain     |
| **Skills**   | 56 reusable knowledge modules loaded dynamically |

Together, they implement the **Signal Principle**: _"If you have a signal,
answer it."_ Agents check for pending signals on startup and resume work
automatically.

## Why Agents + Skills?

Claude Code's power comes from combining **agents** (specialized subagents with
focused expertise) and **skills** (reusable knowledge modules). This
architecture provides:

### Context Efficiency

Each subagent runs in its own context window. Instead of one massive
conversation that hits context limits, work is distributed across multiple
focused agents. This means:

- **Larger projects**: Break down 50-file refactors into parallel subagent
  tasks
- **Preserved context**: The main conversation stays clean while subagents
  handle details
- **Better results**: Each agent focuses on its specialty without context
  pollution

### Versatility Through Composition

Skills are loaded dynamically based on the task. A `software-engineer` agent
working on a Python API can automatically load `python`, `api-design`, and
`testing` skills--getting specialized knowledge without bloating every
conversation.

### Parallel Execution

Subagents can run in parallel (up to 10 concurrent). A single prompt like
"refactor authentication across all services" can spawn multiple agents working
simultaneously on different files, dramatically reducing total time.

> **Learn more**:
> [Subagents Documentation](https://code.claude.com/docs/en/sub-agents) -
> [Agent Skills Blog Post](https://claude.com/blog/skills)

## Quick Start

Install everything with one command:

```bash
curl -fsSL \
  https://raw.githubusercontent.com/cosmix/claude-loom/main/install.sh | bash
```

Initialize loom with a plan and execute:

```bash
cd /path/to/project
loom init doc/plans/my-plan.md   # Initialize with a plan
loom run                          # Start daemon and execute stages
loom status                       # Live dashboard (Ctrl+C to exit)
loom stop                         # Stop the daemon
```

loom will create git worktrees for parallel stages and spawn Claude Code
sessions in native terminal windows automatically.

## How It Works

### The Signal Principle

On every session start, agents:

1. Check `.work/signals/` for pending work matching their role
2. If a signal exists, load context from referenced files and execute
   immediately
3. If no signal, ask the user what to do

This creates continuity across sessions without manual intervention.

### Core Concepts

| Concept      | Description                                        |
| ------------ | -------------------------------------------------- |
| **Plan**     | Parent container for stages, lives in `doc/plans/` |
| **Stage**    | A unit of work within a plan, with dependencies    |
| **Session**  | A Claude Code instance executing a stage           |
| **Worktree** | Git worktree for parallel stage isolation          |
| **Handoff**  | Context dump when session exhausts context         |

### Context Thresholds

Agents monitor their context usage and act accordingly:

| Level  | Usage  | Action                                       |
| ------ | ------ | -------------------------------------------- |
| Green  | < 60%  | Normal operation                             |
| Yellow | 60-74% | Consider creating handoff soon               |
| Red    | >= 75% | Create handoff immediately, then start fresh |

### State Persistence

All state lives in `.work/` as structured files:

```text
project/
├── .work/                    # loom state (version controlled)
│   ├── config.toml           # Active plan, settings
│   ├── execution-graph.toml  # Stage dependency DAG
│   ├── orchestrator.pid      # Daemon process ID
│   ├── orchestrator.sock     # Unix socket for IPC
│   ├── orchestrator.log      # Daemon log file
│   ├── stages/               # Stage state files
│   ├── sessions/             # Session state files
│   ├── signals/              # Stage assignments
│   ├── handoffs/             # Context dumps
│   └── worktrees/            # Worktree metadata
├── .worktrees/               # Git worktrees for parallel stages
│   ├── stage-1/              # Full checkout for stage-1
│   │   ├── .work/            # Symlink to main .work/
│   │   └── [project files]
│   └── stage-2/
└── doc/plans/                # Plan documents
```

This git-friendly format enables version control, team collaboration, and
manual inspection.

## Architecture

loom uses a **daemon architecture** for reliability:

```text
┌─────────────────────────────────────────────────────────────┐
│                        loom run                              │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              Daemon Process (background)                     │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Unix Socket (.work/orchestrator.sock)              │    │
│  │  - Accepts CLI connections (status, logs, stop)     │    │
│  │  - Streams live updates to `loom status` clients    │    │
│  │  - Handles shutdown commands                        │    │
│  └─────────────────────────────────────────────────────┘    │
│                           │                                  │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Orchestrator Loop (every 5s)                       │    │
│  │  - Poll stage/session files for changes             │    │
│  │  - Start ready stages (spawn native terminals)      │    │
│  │  - Detect crashed sessions (PID not alive)          │    │
│  │  - Generate git-based handoffs on crash             │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                           │
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
      ┌─────────┐    ┌─────────┐    ┌─────────┐
      │ Terminal│    │ Terminal│    │ Terminal│  (auto-detected:
      │ Window  │    │ Window  │    │ Window  │   kitty, alacritty,
      │ stage-1 │    │ stage-2 │    │ stage-3 │   gnome-terminal, etc.)
      └─────────┘    └─────────┘    └─────────┘
```

**Key benefits:**

- Sessions run in native terminal windows (more stable than tmux)
- Daemon survives terminal close
- PID-based liveness monitoring
- Git-based crash recovery

## How the CLAUDE.md Configuration Works

The CLAUDE.md instructions you add to `~/.claude/CLAUDE.md` become part of
Claude's system context. They guide Claude to:

1. **Recognize delegation opportunities** - When you ask for a complex task,
   Claude checks if specialist agents can handle parts of it

2. **Choose the right agent tier** - Senior agents (opus) for
   architecture/debugging, standard agents (sonnet) for implementation

3. **Parallelize independent work** - Claude spawns multiple subagents
   simultaneously when tasks don't depend on each other

4. **Manage context efficiently** - Instead of trying to do everything in one
   context window, Claude distributes work across focused subagents

### Example: Multi-File Refactoring

Without orchestration rules, Claude might try to refactor 20 files sequentially
in one context, eventually hitting limits.

With orchestration rules, Claude will:

1. Use tech-lead to analyze the refactoring scope
2. Spawn senior-software-engineer to design the approach
3. Spawn multiple software-engineer agents IN PARALLEL to refactor different
   files
4. Each agent loads relevant skills (python, refactoring, testing)
5. Results merge back without exhausting main context

## Installation

### Quick Install (Recommended)

```bash
curl -fsSL \
  https://raw.githubusercontent.com/cosmix/loom/main/install.sh | bash
```

### Clone and Install

```bash
git clone https://github.com/cosmix/loom.git
cd claude-loom
bash install.sh
```

### Manual Install

```bash
# Copy agents and skills
cp -r agents ~/.claude/
cp -r skills ~/.claude/

# Install configuration
cat CLAUDE.md.template >> ~/.claude/CLAUDE.md

# Download loom CLI for your platform
# Linux x86_64 (glibc):
loom_URL="https://github.com/cosmix/loom/releases/latest/download"
curl -fsSL "$loom_URL/loom-x86_64-unknown-linux-gnu" -o ~/.local/bin/loom
chmod +x ~/.local/bin/loom

# macOS Apple Silicon:
curl -fsSL "$loom_URL/loom-aarch64-apple-darwin" -o ~/.local/bin/loom
chmod +x ~/.local/bin/loom
```

### What Gets Installed

The installation places these components:

| Location              | Contents                              |
| --------------------- | ------------------------------------- |
| `~/.claude/agents/`   | 19 specialized AI agents              |
| `~/.claude/skills/`   | 56 reusable knowledge modules         |
| `~/.claude/CLAUDE.md` | Orchestration rules and configuration |
| `~/.local/bin/loom`   | loom CLI binary                       |

The [`CLAUDE.md.template`](CLAUDE.md.template) configuration includes:

- **Agent orchestration** - When to use senior (opus) vs standard (sonnet)
  agents
- **Parallel/sequential execution** - Guidelines for spawning agents
  efficiently
- **Context passing** - What information to provide subagents
- **Development standards** - Implementation, planning, documentation, code
  quality
- **Dependency management** - Always use package managers, never edit manifests
  manually
- **Progress tracking** - How to record and clean up task progress

### Verify Installation

```bash
# Check loom CLI
loom --version
loom --help

# Start Claude Code and verify agents are available
claude
> What agents and skills are available?
```

### Shell Completions

Enable tab completion for loom commands:

```bash
# Bash (add to ~/.bashrc)
eval "$(loom completions bash)"

# Zsh (add to ~/.zshrc)
eval "$(loom completions zsh)"

# Fish (add to ~/.config/fish/config.fish)
loom completions fish > ~/.config/fish/completions/loom.fish
```

Completions are dynamic - they read current project state for stage IDs, session IDs, and plan files.

## Project Specifications (Workflows)

Your project-specific workflows and specifications should be documented in your
project's `CLAUDE.md` file. This ensures Claude understands your team's
processes and can follow them automatically.

### Adding Workflows to CLAUDE.md

You can include workflows directly in your project's CLAUDE.md:

```markdown
# Project Workflows

## Feature Development

1. Create feature branch from main
2. Write tests first (TDD)
3. Implement feature
4. Run full test suite
5. Create PR with template
```

### Pointing to External Sources

Alternatively, reference external documentation:

```markdown
# Project Workflows

- Feature specs: [Notion workspace](https://notion.so/team/features)
- Sprint planning: [Linear project board](https://linear.app/team/project)
- API contracts: [Confluence page](https://confluence.company.com/api-specs)
```

**Note:** External sources may require MCP (Model Context Protocol) servers to
be configured for Claude to access them directly. Check the
[MCP documentation](https://modelcontextprotocol.io) for integration details.

### Best Practices

- Keep workflows concise and actionable
- Update CLAUDE.md when processes change
- Include links to detailed documentation for complex workflows
- Document team-specific conventions and standards

## loom CLI Reference

### Primary Commands

These commands cover 90% of typical usage:

```bash
# Initialize loom with a plan
loom init <plan-path> [--clean]
# Example: loom init doc/plans/PLAN-auth.md
# - Parses plan, extracts stages and dependencies
# - Creates execution graph in .work/
# - Sets up .work/ directory structure
# - --clean: remove old .work/, prune worktrees, kill orphaned sessions first

# Execute stages (starts daemon, spawns sessions in native terminals)
loom run [--stage <id>] [--manual] [--max-parallel <n>] [--watch] [--foreground] [--attach]
# - Starts background daemon if not running
# - Creates git worktrees for ready parallel stages
# - Spawns Claude sessions in native terminal windows (unless --manual)
# - Monitors progress, triggers dependent stages
# - --stage: run only specific stage
# - --manual: don't spawn sessions, just prepare signals
# - --max-parallel: max parallel sessions (default: 4)
# - --watch: continuous mode - keep running until all stages terminal
# - --foreground: run orchestrator in foreground (for debugging)
# - --attach: attach to existing orchestrator (shows logs)

# Live dashboard - shows plan progress and session health
loom status
# - Connects to daemon via Unix socket
# - Updates continuously until Ctrl+C
# - Shows: stage states, session health, context levels

# Stop the running daemon
loom stop
# - Sends graceful shutdown signal to daemon
# - Terminates all running sessions
# - Cleans up socket and PID files

# Human verification gate
loom verify <stage-id>
# - Runs acceptance criteria
# - Human approves/rejects
# - Triggers dependent stages if approved

# Resume from handoff
loom resume <stage-id>
# - Creates new session with handoff context
# - Continues from where previous session left off

# Merge or recover a stage (restart conflict resolution if interrupted)
loom merge <stage-id> [--force]
# Primary use: recovery from failed/interrupted merge sessions
# - When auto-merge succeeds: worktree branch merged to main, worktree removed
# - When conflicts occur: loom spawns a Claude Code session to resolve them
# - If that session terminates before completion, use this to restart it
# - After conflicts are resolved, use 'loom worktree remove' to clean up
# - --force: merge even if stage not complete or has active sessions

# Attach to running session or view logs
loom attach [target]
loom attach list
loom attach all [--gui] [--detach] [--windows] [--layout <LAYOUT>]
loom attach logs
# - Attaches terminal to running Claude session
# - list: list all attachable sessions
# - all: attach to all sessions (default: tiled pane view)
#   - --gui: open separate terminal windows (native backend)
#   - --detach: detach other clients first
#   - --windows: legacy window-per-session mode instead of tiled panes
#   - --layout: layout for tiled view (tiled, horizontal, vertical)
# - logs: stream daemon logs in real-time
```

### Secondary Commands

Power user commands for fine-grained control:

```bash
# Session management
loom sessions [list|kill <id>]

# Worktree management
loom worktree [list|clean|remove <stage-id>]
# - list: list all worktrees
# - clean: clean up unused worktrees
# - remove: remove specific worktree and branch after merge conflict resolution
#           (use this after resolving conflicts instead of loom merge)

# View/edit execution graph
loom graph [show|edit]

# Force stage state transitions
loom stage complete <stage-id> [--session <id>] [--no-verify]
loom stage block <stage-id> <reason>
loom stage reset <stage-id> [--hard] [--kill-session]
loom stage hold <stage-id>
loom stage release <stage-id>
loom stage waiting <stage-id>
loom stage resume <stage-id>
# - complete: mark stage complete, auto-verify if acceptance passes
#   - --session: mark associated session complete too
#   - --no-verify: skip acceptance criteria (marks Completed, not Verified)
#   - When acceptance passes: auto-transitions to Verified, triggers dependents
#   - When acceptance fails: stays Completed for manual review
# - block: block stage with reason
# - reset: reset to ready state
#   - --hard: also git reset --hard in worktree
#   - --kill-session: kill associated session
# - hold: prevent stage from auto-executing (even when ready)
# - release: allow held stage to execute
# - waiting: mark as waiting for user input (used by hooks)
# - resume: resume from waiting state (used by hooks)

# Clean up resources
loom clean [--all] [--worktrees] [--sessions] [--state]
# - --all: remove all loom resources
# - --worktrees: remove worktrees and branches only
# - --sessions: kill sessions only
# - --state: remove .work/ only
```

### Utility Commands

```bash
loom self-update             # Update loom CLI, agents, skills, and config
```

## Plan Document Format

Plans live in `doc/plans/` and contain structured YAML metadata embedded in
markdown code blocks:

````markdown
# PLAN-0001: User Authentication

## Problem Statement

We need to implement JWT-based authentication...

## Solution Approach

...

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1-jwt
      name: "JWT Token Service"
      description: "Implement core JWT token generation and validation"
      dependencies: []
      parallel_group: null
      acceptance:
        - "cargo test auth::token"
        - "cargo clippy -- -D warnings"
      files:
        - "src/auth/token.rs"
        - "src/auth/mod.rs"

    - id: stage-2-refresh
      name: "Refresh Token Logic"
      description: "Add refresh token rotation and storage"
      dependencies: ["stage-1-jwt"]
      parallel_group: null
      acceptance:
        - "cargo test auth::refresh"
      files:
        - "src/auth/refresh.rs"

    - id: stage-3-middleware
      name: "Auth Middleware"
      description: "HTTP middleware for token validation"
      dependencies: ["stage-1-jwt"]
      parallel_group: "api"
      acceptance:
        - "cargo test middleware::auth"
      files:
        - "src/middleware/auth.rs"
```

<!-- END loom METADATA -->
````

### Metadata Fields

| Field            | Required | Description                                |
| ---------------- | -------- | ------------------------------------------ |
| `version`        | Yes      | loom metadata schema version (currently 1) |
| `stages`         | Yes      | List of work stages with dependencies      |
| `id`             | Yes      | Unique stage identifier (kebab-case)       |
| `name`           | Yes      | Human-readable stage name                  |
| `description`    | Yes      | What this stage accomplishes               |
| `dependencies`   | No       | Stage IDs that must complete first         |
| `parallel_group` | No       | Group name for parallel execution          |
| `acceptance`     | No       | Shell commands to verify completion        |
| `files`          | No       | Glob patterns for modified files           |

## Workflow Example

A complete example from plan creation to execution:

```bash
# 1. Create a plan document (or use Claude in plan mode)
# Write to doc/plans/PLAN-auth.md with loom METADATA block

# 2. Initialize loom with the plan
cd /path/to/project
loom init doc/plans/PLAN-auth.md
# Output: Initialized .work/ directory structure with plan from doc/plans/PLAN-auth.md

# 3. Run stages (starts daemon, spawns terminal windows)
loom run --watch
# Output: Starting daemon...
#         Running in watch mode (continuous execution)...
#         Creating worktree for stage-1...
#         Spawning Claude session in terminal window...
# Watch mode keeps running, auto-spawning ready stages as dependencies complete

# 4. Monitor progress (live dashboard)
loom status
# Shows live-updating display:
#   Stages:   2
#   Sessions: 1
#   stage-1: Running (session: sess-001, context: 45%)
#   stage-2: Blocked (waiting on: stage-1)
# Press Ctrl+C to exit (daemon keeps running)

# 5. View daemon logs
loom attach logs
# Streams real-time logs from the orchestrator

# 6. Verify completed stages (only needed if auto-verification failed)
# With --watch mode, stages auto-verify when acceptance passes.
# Manual verify is only needed when acceptance failed or --no-verify was used.
loom verify stage-1
# Output: Running acceptance criteria...
#         All acceptance criteria passed!
#         Approve this stage? (y/n): y
#         Stage approved! Transitioning to Verified status...
#         Triggered 1 dependent stage: stage-2

# 7. Merge completed work to main
loom merge stage-1
# Output: Merging worktree branch loom/stage-1 to main...
#         Merge successful!
#         Removing worktree...

# 8. Stop the daemon when done
loom stop
# Output: Stopping daemon (PID 12345)...
#         Daemon stopped.

# When context reaches 75%, Claude automatically:
# - Creates handoff in .work/handoffs/
# - Updates session state
# - Exits gracefully
# - loom run or loom resume continues seamlessly

# 9. (Optional) Hold a stage to prevent auto-execution
loom stage hold stage-3
# Stage 'stage-3' held
# The stage will not auto-execute. Use 'loom stage release stage-3' to unlock.

# Release when ready
loom stage release stage-3
# Stage 'stage-3' released
```

## Agent Hierarchy

Agents are organized by domain with two tiers:

| Domain               | Senior (opus)              | Standard (sonnet)   |
| -------------------- | -------------------------- | ------------------- |
| Software Engineering | `senior-software-engineer` | `software-engineer` |
| Machine Learning     | `senior-ml-engineer`       | `ml-engineer`       |
| Infrastructure       | `senior-infra-engineer`    | `infra-engineer`    |
| Product Design       | `senior-product-designer`  | `product-designer`  |
| Analytics            | `senior-data-analyst`      | `data-analyst`      |
| Quality Assurance    | `senior-qa-engineer`       | `qa-engineer`       |
| Data Engineering     | `senior-data-engineer`     | `data-engineer`     |
| Technical Writing    | `senior-technical-writer`  | `technical-writer`  |

Special agents:

| Agent               | Model | Purpose                           |
| ------------------- | ----- | --------------------------------- |
| `tech-lead`         | opus  | Cross-team coordination, planning |
| `security-engineer` | opus  | Security review, threat modeling  |

### When to Use Each Tier

**Senior agents (opus)** for higher-level work:

- System design and architecture
- Complex debugging and root cause analysis
- Design pattern selection
- Code review and strategic decisions

**Standard agents (sonnet)** for implementation:

- Well-specified feature implementation
- Following established patterns
- Routine configurations and data processing
- Documentation for implemented features

## Skills Library

Skills are knowledge modules loaded dynamically based on task context. Agents
automatically load relevant skills without explicit invocation.

### Languages

| Skill        | Description                                         |
| ------------ | --------------------------------------------------- |
| `python`     | Pythonic idioms, type hints, async patterns, pytest |
| `golang`     | Go idioms, error handling, concurrency, modules     |
| `rust`       | Ownership, lifetimes, error handling, cargo, traits |
| `typescript` | Type system, generics, utility types, strict mode   |

### Code Quality

| Skill           | Description                                |
| --------------- | ------------------------------------------ |
| `code-review`   | Comprehensive reviews for correctness      |
| `refactoring`   | Restructure code without changing behavior |
| `testing`       | Unit, integration, and e2e test suites     |
| `documentation` | Technical docs, API references, READMEs    |

### Development

| Skill             | Description                                   |
| ----------------- | --------------------------------------------- |
| `api-design`      | RESTful APIs, GraphQL schemas, RPC interfaces |
| `database-design` | Schemas, relationships, indexes, migrations   |
| `git-workflow`    | Branching strategies, commits, conflicts      |
| `debugging`       | Systematic bug diagnosis and resolution       |

### Infrastructure

| Skill        | Description                                 |
| ------------ | ------------------------------------------- |
| `docker`     | Dockerfiles and docker-compose optimization |
| `kubernetes` | K8s deployments, services, configurations   |
| `terraform`  | Infrastructure as Code for cloud resources  |
| `ci-cd`      | Pipeline design and implementation          |
| `crossplane` | Kubernetes-native infrastructure management |
| `loomcd`     | GitOps continuous delivery                  |
| `argocd`     | Declarative GitOps for Kubernetes           |
| `kustomize`  | Kubernetes configuration customization      |
| `karpenter`  | Kubernetes node autoscaling                 |
| `istio`      | Service mesh configuration                  |
| `grafana`    | Observability dashboards                    |
| `prometheus` | Metrics and alerting                        |

### Security

| Skill             | Description                                |
| ----------------- | ------------------------------------------ |
| `security-audit`  | Comprehensive vulnerability assessment     |
| `security-scan`   | Quick routine checks (secrets, deps, SAST) |
| `threat-model`    | STRIDE/DREAD analysis, secure architecture |
| `dependency-scan` | CVE scanning and license compliance        |
| `auth`            | OAuth2, JWT, RBAC/ABAC, session management |

### Reliability

| Skill                   | Description                                    |
| ----------------------- | ---------------------------------------------- |
| `error-handling`        | Error types, recovery strategies, propagation  |
| `logging-observability` | Structured logging, tracing, metrics           |
| `concurrency`           | Async patterns, parallelism, race conditions   |
| `caching`               | Cache strategies, invalidation, Redis patterns |
| `code-migration`        | Version upgrades, framework migrations         |
| `rate-limiting`         | Throttling, backpressure, API quotas           |

### Architecture

| Skill             | Description                                   |
| ----------------- | --------------------------------------------- |
| `event-driven`    | Message queues, pub/sub, event sourcing, CQRS |
| `feature-flags`   | Rollouts, A/B testing, kill switches          |
| `background-jobs` | Job queues, schedulers, workers, idempotency  |
| `webhooks`        | Design, verification, retry logic             |
| `serialization`   | JSON/protobuf/msgpack, schema evolution       |

### Data

| Skill                | Description                                |
| -------------------- | ------------------------------------------ |
| `sql-optimization`   | Query analysis and performance tuning      |
| `data-visualization` | Charts, dashboards, visual analytics       |
| `data-validation`    | Schema validation, sanitization, contracts |
| `search`             | Elasticsearch, full-text search, indexing  |

### Documentation

| Skill               | Description                               |
| ------------------- | ----------------------------------------- |
| `technical-writing` | Clear prose, audience-aware docs          |
| `diagramming`       | Mermaid diagrams, architecture, sequences |
| `api-documentation` | OpenAPI specs, endpoint docs              |
| `md-tables`         | Markdown table alignment and formatting   |

### QA

| Skill                 | Description                           |
| --------------------- | ------------------------------------- |
| `test-strategy`       | Test pyramid, coverage goals          |
| `e2e-testing`         | Playwright/Cypress patterns, fixtures |
| `performance-testing` | Load testing, benchmarking, profiling |

### AI/ML

| Skill                | Description                               |
| -------------------- | ----------------------------------------- |
| `prompt-engineering` | LLM prompt design and optimization        |
| `model-evaluation`   | ML model performance and fairness testing |

### Frontend

| Skill           | Description                             |
| --------------- | --------------------------------------- |
| `accessibility` | WCAG compliance, a11y testing           |
| `i18n`          | Internationalization, translations, RTL |
| `react`         | React patterns, hooks, state management |

## Customization

### Adding Agents

Create a new `.md` file in `~/.claude/agents/`:

```markdown
---
name: my-agent
description: What this agent does. Use PROACTIVELY when relevant.
tools: Read, Edit, Write, Glob, Grep, Bash
model: sonnet
---

Your agent's system prompt here.
```

### Adding Skills

Create a new directory in `~/.claude/skills/` with a `SKILL.md`:

```markdown
---
name: my-skill
description: What this skill does and when to use it.
allowed-tools: Read, Grep, Glob
---

# My Skill

## Instructions

Step-by-step guidance.

## Best Practices

Key principles.

## Examples

Concrete examples.
```

## Further Reading

- [Claude Code Documentation](https://code.claude.com/docs/en/overview)
- [Subagents Deep Dive](https://code.claude.com/docs/en/sub-agents)
- [Agent Skills Introduction](https://claude.com/blog/skills)
- [Claude Code Best Practices](https://www.anthropic.com/engineering/claude-code-best-practices)

## License

MIT
