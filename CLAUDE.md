# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Loom is a self-propelling agent orchestration CLI written in Rust. It coordinates Claude Code sessions across git worktrees, enabling parallel task execution with automatic crash recovery and context handoffs.

## Build Commands

```bash
cd loom
cargo build                    # Development build
cargo build --release          # Release build
cargo test                     # Run all tests
cargo test stage_transitions   # Run single test file
cargo test --test e2e          # Run end-to-end tests
cargo clippy -- -D warnings    # Lint with warnings as errors
cargo fmt --check              # Check formatting
```

Tests use `serial_test` for isolation - many tests cannot run in parallel.

## Architecture

### Source Layout (`loom/src/`)

```text
main.rs              CLI entry point (clap-based command definitions)
lib.rs               Module exports

commands/            Command implementations (init, run, status, stage, etc.)
daemon/              Unix socket server for IPC (protocol.rs, server.rs)
orchestrator/
  core/              Main orchestrator loop and state machine
  terminal/          Backend abstraction (native terminals, tmux)
  monitor/           Session health monitoring and context tracking
  signals/           Signal generation for agent communication
  auto_merge.rs      Git merge automation
  retry.rs           Failure retry with backoff
models/
  stage/             Stage state machine (types.rs, transitions.rs)
  session/           Session lifecycle tracking
plan/
  parser.rs          Markdown plan document parser
  schema/            YAML metadata validation
  graph/             Execution DAG building
fs/                  File operations for .work/ directory
git/                 Git operations (worktrees, branches, merges)
verify/              Acceptance criteria execution
```

### Key Abstractions

**ExecutionGraph** (`plan/graph/`): DAG of stages with dependency tracking. Determines which stages are ready to execute.

**Stage** (`models/stage/types.rs`): Unit of work with lifecycle:
`WaitingForDeps → Queued → Executing → Completed/Blocked/NeedsHandoff`

**Session** (`models/session/`): Represents a Claude Code process executing a stage. Tracks PID, context usage, timestamps.

**Orchestrator** (`orchestrator/core/orchestrator.rs`): Main loop that polls stage/session state, spawns ready stages, handles crashes.

**TerminalBackend** (`orchestrator/terminal/`): Abstraction for spawning sessions. Native backend opens OS terminal windows; tmux backend uses tmux sessions.

### State Directory (`.work/`)

All state is file-based for git-friendliness:

- `config.toml` - Active plan reference and settings
- `stages/*.md` - Stage state files (markdown with YAML frontmatter)
- `sessions/*.md` - Session tracking files
- `signals/*.md` - Agent assignment signals
- `handoffs/*.md` - Context dump records
- `orchestrator.sock` - Unix socket for daemon IPC
- `orchestrator.pid` - Daemon process ID

### Daemon Architecture

`loom run` spawns a background daemon that:

1. Listens on Unix socket for status/stop commands
2. Polls stage files every 5 seconds
3. Creates git worktrees for parallel stages
4. Spawns Claude Code in native terminal windows
5. Detects crashes via PID liveness checks
6. Generates git-based crash reports

### Plan Format

Plans are markdown files in `doc/plans/` with embedded YAML for use by loom:

````markdown
<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-id
      name: "Human Name"
      description: "Task details"
      dependencies: ["other-stage"]
      acceptance: ["cargo test"]
      files: ["src/**/*.rs"]
```
````

<!-- END loom METADATA -->
