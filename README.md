# Claude Flux

Claude Flux is a self-propelling agent orchestration system for Claude Code.
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

Flux solves these problems with three integrated components:

| Component    | Purpose                                          |
| ------------ | ------------------------------------------------ |
| **Flux CLI** | Manages persistent work state across sessions    |
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
  https://raw.githubusercontent.com/cosmix/claude-flux/main/install.sh | bash
```

Initialize Flux in your project:

```bash
cd /path/to/project
flux init
flux status
```

Start Claude Code. It will automatically detect the Flux configuration and
check for pending work.

## How It Works

### The Signal Principle

On every session start, agents:

1. Check `.work/signals/` for pending work matching their role
2. If a signal exists, load context from referenced files and execute
   immediately
3. If no signal, ask the user what to do

This creates continuity across sessions without manual intervention.

### Core Concepts

| Concept     | Description                                          |
| ----------- | ---------------------------------------------------- |
| **Runner**  | An agent instance with a specific role               |
| **Track**   | A unit of work like a feature, bug fix, or refactor  |
| **Signal**  | A message telling a runner what work needs attention |
| **Handoff** | Structured state snapshot for session transitions    |

### Context Thresholds

Agents monitor their context usage and act accordingly:

| Level  | Usage  | Action                                       |
| ------ | ------ | -------------------------------------------- |
| Green  | < 60%  | Normal operation                             |
| Yellow | 60-74% | Consider creating handoff soon               |
| Red    | >= 75% | Create handoff immediately, then start fresh |

### State Persistence

All state lives in `.work/` as markdown files:

```text
.work/
├── runners/      # Agent instance states (se-001.md, tl-001.md)
├── tracks/       # Work unit details (feature-auth.md)
├── signals/      # Pending work items
└── handoffs/     # Session transition documents
```

This git-friendly format enables version control, team collaboration, and
manual inspection.

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
  https://raw.githubusercontent.com/cosmix/claude-flux/main/install.sh | bash
```

### Clone and Install

```bash
git clone https://github.com/cosmix/claude-flux.git
cd claude-flux
bash install.sh
```

### Manual Install

```bash
# Copy agents and skills
cp -r agents ~/.claude/
cp -r skills ~/.claude/

# Install configuration
cat CLAUDE.template.md >> ~/.claude/CLAUDE.md

# Download Flux CLI for your platform
# Linux x86_64 (glibc):
FLUX_URL="https://github.com/cosmix/claude-flux/releases/latest/download"
curl -fsSL "$FLUX_URL/flux-x86_64-unknown-linux-gnu" -o ~/.local/bin/flux
chmod +x ~/.local/bin/flux

# macOS Apple Silicon:
curl -fsSL "$FLUX_URL/flux-aarch64-apple-darwin" -o ~/.local/bin/flux
chmod +x ~/.local/bin/flux
```

### What Gets Installed

The installation places these components:

| Location              | Contents                              |
| --------------------- | ------------------------------------- |
| `~/.claude/agents/`   | 19 specialized AI agents              |
| `~/.claude/skills/`   | 56 reusable knowledge modules         |
| `~/.claude/CLAUDE.md` | Orchestration rules and configuration |
| `~/.local/bin/flux`   | Flux CLI binary                       |

The [`CLAUDE.template.md`](CLAUDE.template.md) configuration includes:

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
# Check Flux CLI
flux --version
flux doctor

# Start Claude Code and verify agents are available
claude
> What agents and skills are available?
```

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

## Flux CLI Reference

### Initialization and Health

```bash
flux init                    # Initialize .work/ directory in current project
flux status                  # Show dashboard with runners, tracks, signals
flux validate                # Check state file integrity
flux doctor                  # Diagnose configuration issues
```

### Track Management

Tracks represent units of work (features, bugs, refactors).

```bash
flux track new "Feature Name"     # Create new work track
flux track list                   # List all tracks
flux track show <id>              # View track details
flux track close <id>             # Close completed track
```

### Runner Management

Runners are agent instances assigned to work.

```bash
flux runner create <name> -t <type>    # Create runner with role type
flux runner list                       # List all runners
flux runner assign <runner> <track>    # Assign runner to track
flux runner release <runner>           # Release runner from current track
```

Runner types map to agent roles:

| Type                       | Example IDs    | Role                    |
| -------------------------- | -------------- | ----------------------- |
| `software-engineer`        | se-001, se-002 | Implementation          |
| `senior-software-engineer` | sse-001        | Architecture, design    |
| `tech-lead`                | tl-001         | Cross-team coordination |
| `security-engineer`        | sec-001        | Security review, audit  |

### Signal Management

Signals tell runners what work needs attention.

```bash
flux signal set <runner> <type> <message>   # Send signal to runner
flux signal show                            # View all pending signals
flux signal clear <id>                      # Clear/complete signal
```

Signal types: `start`, `review`, `debug`, `test`, `document`

### Self-Update

```bash
flux self-update              # Update Flux CLI, agents, skills, and config
```

## Workflow Example

A complete example from initialization to autonomous handoff:

```bash
# 1. Initialize Flux in your project
cd /path/to/project
flux init

# 2. Create a track for your work
flux track new "User Authentication"
# Output: Created track: user-authentication (t-001)

# 3. Create and assign a runner
flux runner create auth-impl -t software-engineer
# Output: Created runner: se-001

flux runner assign se-001 user-authentication

# 4. Signal the runner to start work
flux signal set se-001 start "Implement JWT auth with refresh tokens"

# 5. Check status
flux status
# Shows: se-001 assigned to t-001 with pending start signal

# 6. Start Claude Code - it picks up the signal automatically
claude
# Claude reads .work/, sees the signal, begins work

# When context reaches 75%, Claude:
# - Creates handoff in .work/handoffs/
# - Updates signal with next steps
# - Prompts to start fresh session
# - New session loads signal + handoff, continues seamlessly
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

| Agent               | Model | Purpose                              |
| ------------------- | ----- | ------------------------------------ |
| `tech-lead`         | opus  | Cross-team coordination, planning    |
| `security-engineer` | opus  | Security review, threat modeling     |
| `doc-editor`        | haiku | Markdown formatting, doc consistency |

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
| `fluxcd`     | GitOps continuous delivery                  |
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
