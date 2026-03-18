# Loom

Loom is an agent orchestration system for Claude Code. It coordinates AI agent sessions across git worktrees, enabling parallel task execution with crash recovery, context handoffs, and structured verification.

## What Loom Solves

- Context exhaustion in long agent sessions
- Lost execution state when sessions crash or end
- Manual handoff/restart overhead
- Weak coordination across multi-stage work

## Key Capabilities

- Persistent orchestration state in `.work/`
- Git worktree isolation for parallel stage execution
- Stage-aware signals and recovery flows
- Goal-backward verification (`truths`, `artifacts`, `wiring`, `wiring_tests`)
- Plan-level and stage-level sandbox controls
- Optional agent teams guidance in stage signals

## Platform Support

- Linux: primary development and full CI test runs
- macOS: supported for build/terminal integration, CI does build-only verification
- Windows: not supported (WSL may work but is best-effort)

## Quick Start

Loom is under active development and not yet published to GitHub Releases. You need to build locally with the Rust toolchain installed.

### 1. Install Loom

```bash
git clone https://github.com/cosmix/loom.git
cd loom
bash ./dev-install.sh
```

`dev-install.sh` builds the release binary (`cargo build --release`) and runs `install.sh`, which installs `loom-*` prefixed agents and skills (non-destructively, preserving user customizations), hooks, and configuration into `~/.claude/` and the CLI binary to `~/.local/bin/loom`. Orchestration rules are written directly to `~/.claude/CLAUDE.md` (existing file is backed up).

### 2. Write a Plan

Plans are how loom knows what to build. Open Claude Code in your target project and use the `/loom-plan-writer` skill to create one:

```bash
cd /path/to/project
claude  # start Claude Code CLI
```

Inside the Claude Code session:

1. Enter plan mode (`/plan`)
2. Load the plan-writing skill by typing `/loom-plan-writer`
3. Describe what you want to build and discuss with Claude
4. Claude will write the plan to `doc/plans/PLAN-<name>.md`

### 3. Run Loom

Once your plan is written:

```bash
loom init doc/plans/PLAN-<name>.md
loom run
loom status --live
loom stop
```

`loom init` parses the plan, creates stage state, and installs/configures project hook wiring automatically. For an existing repo that is missing Claude Code hook setup, run `loom repair --fix`.

### What Gets Installed

| Location | Contents |
| --- | --- |
| `~/.claude/agents/loom-*.md` | Specialized subagents (per-item, non-destructive) |
| `~/.claude/skills/loom-*/` | Domain knowledge modules (per-item, non-destructive) |
| `~/.claude/hooks/loom/` | Session lifecycle hooks |
| `~/.claude/CLAUDE.md` | Orchestration rules |
| `~/.local/bin/loom` | Loom CLI |

## Core Workflow

1. Open Claude Code, enter plan mode (`/plan`), and use `/loom-plan-writer` to write a plan to `doc/plans/`.
2. Run `loom init <plan-path>` to parse metadata and create stage state.
3. Run `loom run` to start daemon + orchestrator.
4. Track progress with `loom status --live`.
5. Recover, verify, merge, or retry stages as needed.

## CLI Reference

### Primary Commands

```bash
loom init <plan-path> [--clean]
loom run [--manual] [--max-parallel N] [--foreground] [--watch] [--no-merge]
loom status [--live] [--compact] [--verbose]
loom stop
loom resume <stage-id>
loom check <stage-id> [--suggest]
loom diagnose <stage-id>
```

### Stage Commands

```bash
loom stage complete <stage-id> [--session <id>] [--no-verify] [--force-unsafe --assume-merged]
loom stage block <stage-id> <reason>
loom stage reset <stage-id> [--hard] [--kill-session]
loom stage waiting <stage-id>
loom stage resume <stage-id>
loom stage hold <stage-id>
loom stage release <stage-id>
loom stage skip <stage-id> [--reason <text>]
loom stage retry <stage-id> [--force]
loom stage recover <stage-id> [--force]
loom stage merge-complete <stage-id>
loom stage verify <stage-id> [--no-reload] [--dry-run]
loom stage human-review <stage-id> [--approve|--force-complete|--reject <reason>]
loom stage dispute-criteria <stage-id> <reason>
loom stage retry-merge [stage-id]
```

### Stage Outputs

```bash
loom stage output set <stage-id> <key> <value> [--description <text>]
loom stage output get <stage-id> <key>
loom stage output list <stage-id>
loom stage output remove <stage-id> <key>
```

### Knowledge / Memory

```bash
loom knowledge show [file]
loom knowledge update <file> <content>
loom knowledge init
loom knowledge list
loom knowledge check [--min-coverage N] [--src-path <path>] [--quiet]
loom knowledge gc [--max-file-lines N] [--max-total-lines N] [--quiet]
loom knowledge bootstrap [--model <name>] [--skip-map] [--quick]

loom memory note <text> [--stage <id>]
loom memory decision <text> [--context <why>] [--stage <id>]
loom memory change <text> [--stage <id>]
loom memory question <text> [--stage <id>]
loom memory query <search> [--stage <id>]
loom memory list [--stage <id>] [--entry-type <type>]
loom memory show [--stage <id>] [--all]
```

`loom knowledge bootstrap` launches a Claude-driven exploration session that populates `doc/loom/knowledge/`. By default it runs a deep `loom map` pass first, then starts Claude with permission to update knowledge files via `loom knowledge update`.

### Other Commands

```bash
loom sessions list
loom sessions kill <session-id...> | --stage <stage-id>
loom worktree list
loom worktree remove <stage-id>
loom graph
loom map [--deep] [--focus <area>] [--overwrite]
loom repair [--fix]
loom clean [--all|--worktrees|--sessions|--state]
loom self-update
loom completions <bash|zsh|fish>
```

## Plan Format

Plans live in `doc/plans/` with metadata in fenced YAML between loom markers.

````markdown
# PLAN-0001: Feature Name

<!-- loom METADATA -->
```yaml
loom:
  version: 1
  sandbox:
    enabled: true
  stages:
    - id: implement-api
      name: Implement API
      description: Add endpoint + tests
      working_dir: "."
      stage_type: standard
      dependencies: []
      acceptance:
        - "cargo test"
      files:
        - "loom/src/**/*.rs"
      truths:
        - "cargo test api_integration::returns_200"
      artifacts:
        - "loom/src/api/*.rs"
      wiring:
        - source: "loom/src/main.rs"
          pattern: "mod api;"
          description: "API module registered"
      execution_mode: team

    - id: integration-verify
      name: Integration Verify
      working_dir: "."
      stage_type: integration-verify
      dependencies: ["implement-api"]
      acceptance:
        - "cargo test --all-targets"
      truths:
        - "cargo test api_integration::returns_200"
```
<!-- END loom METADATA -->
````

### Stage Fields

| Field | Required | Notes |
| --- | --- | --- |
| `id` | Yes | Stage identifier |
| `name` | Yes | Human-readable title |
| `working_dir` | Yes | Relative execution directory (`.` allowed) |
| `description` | No | Optional summary |
| `dependencies` | No | Upstream stage IDs |
| `acceptance` | No | Shell criteria for stage completion |
| `setup` | No | Setup commands |
| `files` | No | File glob scope |
| `stage_type` | No | `standard` (default), `knowledge`, `integration-verify` |
| `truths` / `artifacts` / `wiring` | Conditionally required | Required for `standard` and `integration-verify` stages |
| `truth_checks` / `wiring_tests` / `dead_code_check` | No | Extended verification |
| `context_budget` | No | Context threshold (%) for handoff |
| `sandbox` | No | Per-stage sandbox override |
| `execution_mode` | No | `single` (default) or `team` hint |

### Stage Type Behavior

- `knowledge`: knowledge/bootstrap work, different verification expectations
- `standard`: implementation stage; must define goal-backward checks
- `integration-verify`: final quality gate combining code review and functional verification; must define goal-backward checks

## Verification Model

`loom check <stage-id>` validates outcomes, not just compilation/tests:

- `truths`: observable behaviors that must succeed
- `artifacts`: real implementation files exist
- `wiring`: critical integration links exist
- `wiring_tests`: runtime integration checks

For `standard` and `integration-verify` stages, at least one goal-backward check must be defined.

## Sandbox Configuration

Loom supports plan-level defaults plus stage-level overrides.

```yaml
loom:
  version: 1
  sandbox:
    enabled: true
    auto_allow: true
    excluded_commands: ["loom"]
    filesystem:
      deny_read:
        - "~/.ssh/**"
        - "~/.aws/**"
        - "../../**"
        - "../.worktrees/**"
      deny_write:
        - "../../**"
        - "doc/loom/knowledge/**"
      allow_write:
        - "src/**"
    network:
      allowed_domains: ["github.com", "crates.io"]
      additional_domains: []
      allow_local_binding: false
      allow_unix_sockets: []
```

Note: knowledge file writes are intentionally protected by sandbox defaults; knowledge updates should be done via `loom knowledge ...` commands.

## Agent Teams (Experimental)

Loom enables agent teams in spawned sessions (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) and injects team-usage guidance into stage signals.

Use teams when work needs coordination/discussion across agents (multi-dimension review, exploratory analysis). Use subagents for independent, concrete file-level tasks.

## State Layout

```text
project/
├── .work/
│   ├── config.toml
│   ├── stages/
│   ├── sessions/
│   ├── signals/
│   └── handoffs/
├── .worktrees/
└── doc/plans/
```

## Shell Completions

```bash
# bash
eval "$(loom completions bash)"

# zsh
eval "$(loom completions zsh)"

# fish
loom completions fish > ~/.config/fish/completions/loom.fish
```

## License

MIT
