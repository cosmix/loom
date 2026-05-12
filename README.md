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
- Goal-backward verification (`artifacts`, `wiring`, `wiring_tests`, `dead_code_check`)
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

To validate the draft before running it:

```bash
loom plan verify doc/plans/PLAN-<name>.md
```

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

| Location                     | Contents                                             |
| ---------------------------- | ---------------------------------------------------- |
| `~/.claude/agents/loom-*.md` | Specialized subagents (per-item, non-destructive)    |
| `~/.claude/skills/loom-*/`   | Domain knowledge modules (per-item, non-destructive) |
| `~/.claude/hooks/loom/`      | Session lifecycle hooks                              |
| `~/.claude/CLAUDE.md`        | Orchestration rules                                  |
| `~/.local/bin/loom`          | Loom CLI                                             |

## Core Workflow

1. Open Claude Code, enter plan mode (`/plan`), and use `/loom-plan-writer` to write a plan to `doc/plans/`.
2. Run `loom init <plan-path>` to parse metadata and create stage state.
3. Run `loom run` to start daemon + orchestrator.
4. Track progress with `loom status --live`.
5. Recover, verify, merge, or retry stages as needed.

## CLI Reference

### Primary Commands

```bash
loom init <plan-path> [--clean] [--backend native|container] [--no-build]
loom run [--manual] [--max-parallel N] [--foreground] [--watch] [--no-merge]
loom status [--live] [--compact] [--verbose]
loom stop
loom resume <stage-id>
loom check <stage-id> [--suggest]
loom diagnose <stage-id>
```

`--backend container` builds a project-specific container image, runs a firewall enforcement smoke test, and pins the image digest to `.work/config.toml`. Subsequent `loom run` invocations use the pinned image without rebuilding.

`--no-build` skips the actual container image build during `loom init --backend container`, pinning `image_digest = "pending"`. Useful for CI setups where the image is built separately via `loom container build`.

`--allow-insecure-runtime` skips the firewall enforcement smoke test. Use on rootless Podman environments without slirp4netns ≥ 1.2.3, or on Apple Container, where iptables-based egress filtering is best-effort and the probe may produce a false negative.

### Plan Commands

```bash
loom plan verify <plan-path> [--strict] [--json] [--no-color]
```

`loom plan verify` validates a plan file without touching `.work/` or requiring a git repo. It runs the same fatal validation as `loom init` (schema errors, duplicate IDs, unknown dependencies, path safety) plus advisory warnings (structural issues, missing knowledge-bootstrap stage, sandbox gaps). Exits 0 on success, non-zero on fatal errors; `--strict` promotes warnings to errors.

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
loom stage retry <stage-id> [--force] [--context <message>]
loom stage merge [stage-id] [--resolved]
loom stage verify <stage-id> [--no-reload] [--dry-run]
loom stage human-review <stage-id> [--approve|--force-complete|--reject <reason>]
loom stage dispute-criteria <stage-id> <reason>
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
loom knowledge audit [--max-file-lines N] [--max-total-lines N] [--quiet]   # Report size/duplicate/promoted-block issues
loom knowledge gc [--model NAME] [--dry-run] [--quick]                       # Spawn Claude to compact (dedupe, summarize, drop stale)
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

### Container Commands

```bash
loom container build
loom container rebuild
loom container doctor
loom container shell <stage-id>
loom container logs <stage-id> [--follow] [--tail <N>] [--format human|json] [--show-thinking] [--verbose]
loom container list [--all] [--json]
```

`loom container build` builds the project container image (if not already cached). Equivalent to the build step in `loom init --backend container`.

`loom container rebuild` forces a rebuild of the container image, ignoring the cached fingerprint. Use when the Dockerfile template or firewall script has changed and the fingerprint doesn't yet reflect it (e.g., after a `loom` binary upgrade).

`loom container doctor` checks container runtime availability, image freshness, and network configuration.

`loom container shell <stage-id>` opens an interactive shell inside the container for a running stage session (`/repo` bind mount, hooks, firewall). Useful for debugging the container environment.

`loom container logs <stage-id>` tails or follows the stdout/stderr of a running or exited stage's container. Scans `.work/sessions/` for an active container-backed session matching the stage ID.

| Flag | Default | Description |
|------|---------|-------------|
| `--follow` / `-f` | off | Stream live output (like `docker logs -f`) |
| `--tail N` | 100 | Lines to show from the end |
| `--format human\|json` | `human` | `human` renders stream-json as readable text; `json` passes raw bytes through |
| `--show-thinking` | off | Show `[thinking]` prefixed lines from assistant thinking blocks |
| `--verbose` | off | Append a footer with suppressed event counts (`system`, `rate_limit_event`) |

The default `--format=human` parses the stream-json (JSONL) transcript emitted by Claude Code and renders: text blocks (with `---` separator), tool calls (`-> Tool(args)`), tool results (`<- ok (N bytes)` / `<- error: first line`), and hook blocks. Use `--format=json` to get the raw JSONL for scripting or log archiving.

`loom container list` shows all session-backed containers for this workspace. By default only running containers are shown; `--all` includes exited/removed containers. `--json` emits JSON Lines for scripting. For orphan containers left behind by a crashed daemon, run `loom clean --sessions`.

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
loom completions [<shell>] [--install] [--migrate]
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
        - command: "cargo test api_integration::returns_200"
          stdout_contains: ["test result: ok"]
      files:
        - "loom/src/**/*.rs"
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
        - command: "cargo test api_integration::returns_200"
          stdout_contains: ["test result: ok"]
```

<!-- END loom METADATA -->
````

### Stage Fields

| Field                              | Required               | Notes                                                                                                         |
| ---------------------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------- |
| `id`                               | Yes                    | Stage identifier                                                                                              |
| `name`                             | Yes                    | Human-readable title                                                                                          |
| `working_dir`                      | Yes                    | Relative execution directory (`.` allowed)                                                                    |
| `description`                      | No                     | Optional summary                                                                                              |
| `dependencies`                     | No                     | Upstream stage IDs                                                                                            |
| `acceptance`                       | Conditionally required | Shell criteria (strings or extended objects with stdout_contains etc.)                                        |
| `setup`                            | No                     | Setup commands                                                                                                |
| `files`                            | No                     | File glob scope                                                                                               |
| `stage_type`                       | No                     | `standard` (default), `knowledge`, `integration-verify`                                                       |
| `artifacts` / `wiring`             | Conditionally required | Required for `standard` and `integration-verify` (acceptance OR goal-backward)                                |
| `wiring_tests` / `dead_code_check` | No                     | Extended verification                                                                                         |
| `context_budget`                   | No                     | Context threshold (%) for handoff                                                                             |
| `sandbox`                          | No                     | Per-stage sandbox override                                                                                    |
| `sandbox.permission_mode`          | No                     | `auto`, `accept-edits`, `bypass-permissions`, `plan`, `default` (resolves: stage > plan > stage-type default) |
| `backend`                          | No                     | Per-stage backend override: `native` or `container` (overrides project default)                               |
| `execution_mode`                   | No                     | `single` (default) or `team` hint                                                                             |

### Stage Type Behavior

- `knowledge`: knowledge/bootstrap work, different verification expectations
- `standard`: implementation stage; must define goal-backward checks
- `integration-verify`: final quality gate combining code review and functional verification; must define goal-backward checks
- `knowledge-distill`: final stage; curates stage memories into permanent knowledge files

## Container Backend

Loom supports running stage sessions inside isolated containers (Docker, Podman, or Apple Container on macOS).

### Setup

```bash
loom init doc/plans/PLAN-<name>.md --backend container
```

This builds a project-specific container image, pins its digest to `.work/config.toml`, and configures all future sessions to run inside the container.

To skip the image build (e.g., in CI where the image is pre-built):

```bash
loom init doc/plans/PLAN-<name>.md --backend container --no-build
loom container build  # build separately when ready
```

### What Runs Inside the Container

- Host repo root is bind-mounted at `/repo` read-only as a base layer; only the stage-specific subtrees (worktree directory, `.work/memory`, `.work/sessions`, etc.) are overlaid read-write — git metadata is preserved while sensitive paths (`.git/`, sibling worktrees, `doc/plans/`, `.work/config.toml`) remain read-only
- Merge and BaseConflict sessions are exempt: they receive full read-write access to `/repo` for conflict resolution
- Stage cwd: `/repo/.worktrees/<stage-id>` | Merge/knowledge cwd: `/repo`
- Hooks from `~/.claude/hooks/loom` are mounted read-only at `/home/loom/.claude/hooks/loom`
- `settings.local.json` in the worktree is mounted read-only — agents cannot disable Claude Code hooks from inside the container
- Network egress is filtered by the plan's `loom.sandbox.network.allowed_domains` setting

### Credential Forwarding

By default no host credentials are forwarded into the container (explicit opt-in). To forward Claude Code credentials, add to `.work/config.toml`:

```toml
[project_execution.container]
forward_credentials = ["claude"]
```

Supported values: `"claude"` (mounts `~/.claude/.credentials.json`).

### Git Identity

Container sessions have no `~/.gitconfig`. `loom init --backend container` reads the host git identity and persists it in `.work/config.toml`:

```toml
[project_execution.container]
git_user_name = "Jane Developer"
git_user_email = "jane@example.com"
```

These are injected as `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME`, and `GIT_COMMITTER_EMAIL` environment variables into every container session. Both fields must be present — if either is missing, no identity env vars are injected (partial identity produces malformed commits).

To override or set manually after init, edit `.work/config.toml` on the host. Values are validated at init time and on each load: empty strings, values longer than 256 bytes, and values containing control characters are rejected.

### permission_mode

The `permission_mode` field controls Claude Code's permission prompting:

| Value                | Behavior                                                         |
| -------------------- | ---------------------------------------------------------------- |
| `auto`               | Claude's heuristics (default for standard/integration-verify)    |
| `accept-edits`       | Auto-accept file edits (default for knowledge/knowledge-distill) |
| `bypass-permissions` | No permission prompts — **container backend only**               |
| `plan`               | Use Claude Code plan mode                                        |
| `default`            | Claude Code's built-in default                                   |

`bypass-permissions` is only accepted when the backend is `container`. Using it with the native backend is rejected at `loom init` and spawn time.

```yaml
loom:
  sandbox:
    permission_mode: accept-edits # plan-level default
  stages:
    - id: my-stage
      sandbox:
        permission_mode: bypass-permissions # stage override (container only)
      backend: container
```

### Threat Model

The container backend provides defense-in-depth against agent misbehavior:

- IPv6 denied (AF_INET6)
- Cloud metadata service blocked (169.254.169.254)
- Localhost restricted (127.0.0.0/8 except 127.0.0.1)
- `*.internal` domains blocked
- Network allowlist is host-owned and mounted read-only — the agent cannot modify it
- Firewall script lives inside the image — the agent cannot replace it
- Host credentials are NOT forwarded by default

## Verification Model

`loom check <stage-id>` validates outcomes, not just compilation/tests:

- `acceptance`: shell criteria (simple strings or extended objects with `stdout_contains`, `exit_code`, etc.)
- `artifacts`: real implementation files exist
- `wiring`: critical integration links exist
- `wiring_tests`: runtime integration checks
- `dead_code_check`: detect unused code via command output patterns

For `standard` and `integration-verify` stages, acceptance criteria or at least one goal-backward check must be defined.

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

Loom provides context-aware tab completions for all commands, subcommands, flags, and dynamic values (stage IDs, plan files, session IDs, knowledge files).

### Quick Install

```bash
loom completions --install
```

Auto-detects your shell from `$SHELL` and writes completions to the standard location:

| Shell | Install Path                                      |
| ----- | ------------------------------------------------- |
| Bash  | `~/.local/share/bash-completion/completions/loom` |
| Zsh   | `~/.zfunc/_loom`                                  |
| Fish  | `~/.config/fish/completions/loom.fish`            |

Follow the printed post-install instructions to activate (e.g., for zsh, ensure `fpath=(~/.zfunc $fpath)` appears before `compinit` in `~/.zshrc`).

### Manual Setup

You can also write the completion script to a file yourself:

```bash
# bash
loom completions bash > ~/.local/share/bash-completion/completions/loom

# zsh — ensure ~/.zfunc is in fpath (add before compinit in ~/.zshrc):
#   fpath=(~/.zfunc $fpath)
#   autoload -Uz compinit && compinit
mkdir -p ~/.zfunc
loom completions zsh > ~/.zfunc/_loom

# fish
loom completions fish > ~/.config/fish/completions/loom.fish
```

### Migrating from Older Versions

Older versions of loom used `clap_complete` and required an `eval` line in your shell RC file that ran a subprocess on every shell startup. The new system writes a static script to disk and only calls `loom` at actual tab-completion time, which means faster shell startup and completions that work even before `loom` is in your `PATH`.

To check whether you need to migrate:

```bash
loom completions --migrate
```

This scans for two things:

1. **`eval` lines** in RC files (`.bashrc`, `.zshrc`, etc.) like `eval "$(loom completions zsh)"` — these should be removed
2. **Stale completion files** containing old `clap_complete` markers — these need to be regenerated

If issues are found, follow the printed instructions. Typically: remove the `eval` line from your RC file, then run `loom completions --install` to write the new file-based completion script.

### What's Completed

- Commands and subcommands (`loom stage <TAB>` shows all stage subcommands)
- Flags (`loom run --<TAB>` shows available flags)
- Stage IDs with smart filtering (`loom stage complete <TAB>` shows only executing stages)
- Plan files, session IDs, knowledge files (including aliases like `deps`, `tech`)
- Model names, trigger types, and more

## License

MIT
