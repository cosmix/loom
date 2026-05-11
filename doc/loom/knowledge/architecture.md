# Architecture

> High-level component relationships, data flow, and module dependencies.
>
> **Related files:** [patterns.md](patterns.md) for design patterns, [entry-points.md](entry-points.md) for code navigation, [conventions.md](conventions.md) for coding standards.

## Project Overview

Loom is a Rust CLI (~15K lines) for orchestrating parallel Claude Code sessions across git worktrees. It enables concurrent task execution with automatic crash recovery, context handoffs, and progressive merging.

## Directory Structure

```text
loom/src/
  main.rs, lib.rs          # CLI entry (clap), module exports
  commands/                 # CLI implementations (~4K lines)
    init/, run/, stage/ (complete, merge, merge_resolver, merge_verify, ...),
    status/, merge/, memory/, knowledge/, track/, runner/
  daemon/server/            # Background daemon (~1.5K lines)
    core.rs, lifecycle.rs, protocol.rs, status.rs, client.rs, orchestrator.rs
  orchestrator/             # Core engine (~4K lines)
    core/                   # Main loop, stage executor, persistence, recovery
    terminal/               # TerminalBackend trait + dispatching
      native/               # Host OS terminal spawning (11+ emulators)
      container/            # Docker/Podman/Apple Container backend
        mod.rs              # ContainerBackend (661 lines — refactor candidate)
        fingerprint.rs      # Image fingerprint (langs + embedded resource hashes)
        image.rs            # Global image cache, per-project pin, build
        lifecycle.rs        # Container run args, mount construction
        network.rs          # Per-stage network creation + allowlist materialisation
        resources.rs        # Embedded Dockerfile.tmpl + firewall.sh access
        runtime.rs          # Docker/Podman/Apple Container runtime detection
      dispatcher.rs         # BackendDispatcher — route spawn/kill/liveness by backend
    monitor/                # Session health, heartbeat, failure tracking
    liveness.rs             # LivenessService — backend-aware session liveness probe
    signals/                # Signal generation (Manus format, cache, CRUD)
    continuation/           # Context handoff management
    progressive_merge/      # Merge orchestration + lock
    auto_merge.rs
    merge_attribution.rs    # Attribute global MERGE_HEAD to a stage; reconcile
  models/                   # Domain models (~1K lines)
    stage/ (types, transitions, methods)
    session/ (types, methods)
  plan/                     # Plan parsing (~1.5K lines)
    parser.rs, schema/ (types, validation), graph/ (DAG builder)
  fs/                       # File operations (~500 lines)
    work_dir.rs, knowledge.rs, memory.rs
  git/                      # Git operations (~800 lines)
    worktree/ (base, operations), merge/ (mod, in_progress, lock, status), branch/
  verify/                   # Acceptance + goal-backward verification (~600 lines)
    criteria/, transitions/, goal_backward/
  sandbox/                  # Claude Code sandbox config generation
    config.rs, settings.rs
  hooks/                    # Hook script definitions
  parser/frontmatter.rs     # Canonical YAML frontmatter extraction
  validation.rs             # Input validation (IDs, names)
  completions/              # Shell completion (custom scripts + dynamic engine + install)
  process/                  # PID liveness checking

.work/                      # Runtime state (gitignored)
  config.toml, stages/*.md, sessions/*.md, signals/*.md,
  handoffs/*.md, orchestrator.sock, orchestrator.pid
```

## Core Abstractions

### ExecutionGraph (plan/graph/builder.rs)

DAG of stages with dependency tracking. `get_ready()` returns stages with all deps satisfied (status == Completed AND merged == true). Cycle detection via DFS at build time.

### Stage State Machine (models/stage/)

```text
WaitingForDeps --> Queued --> Executing --> Completed --> Verified
                     |            |
                     v            +--> Blocked, NeedsHandoff, WaitingForInput,
                  Skipped              MergeConflict, CompletedWithFailures, MergeBlocked, NeedsHumanReview
```

12 variants total. Terminal states: Completed, Skipped. Transitions validated in transitions.rs. See [patterns.md -- State Machine Pattern](patterns.md#state-machine-pattern).

**Documented state-machine bypasses:** Two paths intentionally bypass `try_transition`:

1. **`--force-unsafe`** (`handle_force_unsafe_completion`) — sets `Status::Completed` from any state. Manual recovery only.
2. **Phantom-merge revert** (`reconcile_main_repo_active_merge` and `complete()`'s `RevertAndSpawnResolver` arm) — flips a `Completed + merged=true` stage back to `MergeConflict + merged=false + merge_conflict=true` when an active main-repo merge is attributed to that stage. The bypass is necessary because `Completed` is terminal; `try_transition` would refuse, but this is exactly the case the bypass is designed for. All such mutations are logged at `error` level.

### StageType Enum (plan/schema/types.rs)

- **Standard** (default) -- Regular implementation stages, require goal-backward verification
- **Knowledge** -- No worktree, commits required (directly to main), auto merged=true, exploration focus
- **IntegrationVerify** -- Second-to-last quality gate combining code review AND functional verification
- **KnowledgeDistill** -- Final stage, runs after integration-verify, curates session memories into permanent knowledge (worktree stage, sonnet default)

Signal generation has 4 stable prefix generators in cache.rs (standard, knowledge, integration-verify, knowledge-distill).

### Session Lifecycle (models/session/)

States: Spawning -> Running -> Completed | Crashed | ContextExhausted | Paused. Tracks PID, terminal window ID, context usage %, timestamps.

### TerminalBackend (orchestrator/terminal/)

Trait for spawning Claude Code in terminal windows. Two concrete implementations:

- **NativeBackend** (`orchestrator/terminal/native/`) — spawns Claude Code in a host terminal. Supports 11+ emulators via `TerminalEmulator` enum. PID tracking via wrapper scripts writing to `.work/pids/`.
- **ContainerBackend** (`orchestrator/terminal/container/`) — spawns Claude Code inside a Docker/Podman/Apple Container per stage. Host repo is bind-mounted at `/repo` (rw); hooks at `/home/loom/.claude/hooks/loom` (ro); allowlist at `/etc/loom/network/allowed_domains.txt` (ro). Liveness via `<runtime> inspect`, not `kill -0`.

**BackendDispatcher** (`orchestrator/terminal/dispatcher.rs`) — owns one or both backends and routes spawn/kill/liveness calls based on the stage's resolved `BackendType` or a session's persisted `backend` metadata.

**BackendType** (`plan/schema/execution.rs`) — `Native` (default) or `Container`. Canonical definition in plan schema, re-exported by `orchestrator/terminal/mod.rs`. Serializes as kebab-case YAML (`"native"` / `"container"`).

**LivenessService** (`orchestrator/liveness.rs`) — replaces scattered `kill -0` checks. Delegates to `BackendDispatcher::is_session_alive()` so each runtime (native host PID, container inspect) answers for its own sessions.

## Data Flow

### Plan Execution Flow

```text
1. loom init doc/plans/PLAN-foo.md [--backend native|container] [--no-build]
   --> Parse plan, create .work/, write stage files
   --> If --backend container: build/pin image, write [project_execution] to .work/config.toml

2. loom run
   --> Spawn daemon (or foreground) --> orchestrator loop
   --> BackendDispatcher constructed from plan's BackendNeeds
   --> LivenessService wraps dispatcher for monitor thread

3. Orchestrator loop (5s poll):
   Load stage files --> Build ExecutionGraph --> Find ready stages
   --> Resolve per-stage backend (stage override or project default)
   --> Create worktree + signal --> Spawn via dispatcher --> Monitor via LivenessService

4. Agent reads signal, executes, runs: loom stage complete <id>

5. Progressive merge into main branch (dependency order)
```

### IPC Protocol (daemon/server/protocol.rs)

Unix socket at `.work/orchestrator.sock`. Messages: Status, Stop, Subscribe. Length-prefixed JSON (4-byte big-endian, max 10MB). Daemon polls status every 1 second for subscribers.

## File Ownership

| Directory             | Owner Module                     | Purpose              |
| --------------------- | -------------------------------- | -------------------- |
| `.work/stages/`       | orchestrator/core/persistence.rs | Stage state          |
| `.work/sessions/`     | orchestrator/core/persistence.rs | Session state        |
| `.work/signals/`      | orchestrator/signals/            | Agent assignments    |
| `.work/handoffs/`     | orchestrator/continuation/       | Context dumps        |
| `.work/config.toml`   | commands/init/, commands/run/    | Plan reference       |
| `.worktrees/`         | git/worktree/                    | Isolated workspaces  |
| `doc/loom/knowledge/` | fs/knowledge.rs                  | Persistent learnings |

## Container Backend Topology

When `BackendType::Container` is resolved for a stage, `ContainerBackend` spawns the session inside a per-stage container with this fixed mount topology:

| Container path | Host source | Permissions |
| --- | --- | --- |
| `/repo` | host repo root | rw bind |
| `/repo/.work` | derived from `/repo` | rw (via symlink) |
| `/home/loom/.claude/hooks/loom` | `~/.claude/hooks/loom` | ro |
| `/home/loom/.claude/.credentials.json` | `~/.claude/.credentials.json` | ro (only when `forward_credentials` includes `"claude"`) |
| `/etc/loom/network/allowed_domains.txt` | `.work/network/allowed_domains.txt` | ro |

**Why `/repo`?** Git worktrees store relative symlinks (`.work -> ../../.work`). Mounting the host repo root at a fixed path preserves these symlinks and all git metadata. Stage cwd inside the container: `/repo/.worktrees/<stage-id>`. Merge/knowledge cwd: `/repo`.

**forward_credentials:** Default is `Vec::new()` (empty — no credentials forwarded). Agents must explicitly edit `.work/config.toml` to mount credentials. This is stricter than the plan spec's suggested default of `["claude"]`.

**Firewall (defense-in-depth):** Image-resident `firewall.sh` script configured with: deny IPv6 (AF_INET6), block `169.254.169.254` (cloud metadata), block `127.0.0.0/8` except `127.0.0.1`, deny `*.internal`. Allowlist is host-owned and mounted ro — the agent process inside the container cannot edit it.

**Image cache model:** Global image cache at `~/.local/share/loom/images/<fingerprint>.json`. Per-project digest pin at `.work/config.toml::[project_execution.container].image_digest`. Fingerprint encodes: detected languages + SHA-256 of `Dockerfile.tmpl` + SHA-256 of `firewall.sh`. Any change to languages or embedded resources invalidates the cache.

## Worktree Isolation (4-Layer Defense)

1. **Git layer** -- Separate worktrees at `.worktrees/<stage-id>/` with branch `loom/<stage-id>`. Symlinks: `.work` -> shared state, `.claude/CLAUDE.md` -> instructions, root `CLAUDE.md` -> project guidance.
2. **Sandbox layer** -- MergedSandboxConfig (sandbox/config.rs) generates `settings.local.json` with filesystem deny/allow, network domains, excluded commands. Knowledge writes via `loom knowledge update` CLI only.
3. **Signal layer** -- Four stage-type-specific stable prefix generators in cache.rs (standard, knowledge, integration-verify, knowledge-distill). Include isolation rules and subagent restrictions.
4. **Hook layer** -- commit-guard.sh blocks exit without commit. commit-filter.sh blocks subagent git operations via LOOM_MAIN_AGENT_PID/PPID comparison.

## Subagent Isolation

Three-layer defense: documentation (CLAUDE.md Rule 5), signal injection (cache.rs prefix), hook enforcement (commit-filter.sh). Detection: wrapper script exports LOOM_MAIN_AGENT_PID; hook compares PPID to detect subagent context.

## Layering Violations (Known Issues)

Correct dependency direction: commands/ -> orchestrator/ -> models/ (top), daemon/ / git/ / plan/ (middle), fs/ (bottom).

Known violations:

- daemon imports commands (mark_plan_done_if_all_merged) -- fix: move to fs/plan_lifecycle.rs
- orchestrator imports commands (check_merge_state) -- fix: move to git/merge/status.rs
- git/worktree imports orchestrator (hook config) -- fix: extract hooks/ as top-level
- models imports plan/schema (WiringCheck, StageType) -- fix: move types to models/

## Goal-Backward Verification (verify/goal_backward/)

Three verification layers for standard stages:

- **truths** -- Shell commands returning exit 0 (30s timeout, extended criteria: stdout_contains, stderr_empty)
- **artifacts** -- Files must exist with real implementation (stub detection: TODO, FIXME, unimplemented!, todo!)
- **wiring** -- Regex patterns verifying code connections in source files

Returns: GoalBackwardResult::Passed | GapsFound | HumanNeeded. Storage: `.work/verifications/<stage-id>.json`.

## Context Budget Enforcement

Stages define context_budget (1-100%, default 65%, max 75%). Monitor tracks Green (<50%), Yellow (50-64%), Red (65%+). BudgetExceeded event triggers auto-handoff.

## Security Model

- **ID validation**: Alphanumeric + dash/underscore, max 128 chars, no path traversal (validation.rs)
- **Acceptance criteria**: Runs arbitrary shell commands (trusted model)
- **Socket**: Mode 0o600 (owner only), max 100 connections, 10MB message limit, Unix only
- **Self-update**: minisign signature verification. Gap: non-binary release assets lack verification
- **Shell escaping**: escape_shell_single_quote(), escape_applescript_string() in emulator.rs
- **permission_mode field** (`SandboxConfig` / `StageSandboxConfig`): Resolves as stage > plan > stage-type default. `bypass-permissions` ONLY permitted with `BackendType::Container` — rejected on native to prevent host-filesystem full access. Default by stage type: Knowledge/KnowledgeDistill → `acceptEdits`; Standard/IntegrationVerify → `auto`.

## Merge Lock (progressive_merge/lock.rs)

MergeLock prevents concurrent merges via exclusive file at `.work/merge.lock`. Atomic creation, PID + timestamp. Timeout 30s, stale lock auto-cleanup at 5min. Released via Drop.

## Skills Module (loom/src/skills/)

Loads skill metadata from SKILL.md files in ~/.claude/skills/, builds inverted index of trigger keywords, matches stage descriptions. Components: types.rs (SkillMetadata, SkillMatch), matcher.rs (keyword matching, phrase=2pts, word=1pt, threshold 2.0), index.rs (SkillIndex, load_from_directory, match_skills). Up to 5 skill recommendations embedded in agent signals.

## Diagnosis Module (loom/src/diagnosis/)

Analyzes failed/blocked stages. DiagnosisContext collects crash_report, log_tail, git_status, git_diff. Generates diagnostic signal for Claude Code investigation. CLI: `loom diagnose <stage-id>`.

## Map Module (loom/src/map/)

Automated codebase analysis that populates knowledge files. Detectors: project type, dependencies, entry points, structure, conventions, concerns. Features: --deep (3-level depth + concerns), --focus (filter entry points), --overwrite. CLI: `loom map`.

## Handoff System

Fully functional handoff chain:

1. **loom handoff create** -- CLI command accepting --stage, --session, --trigger, --message flags
2. **pre-compact.sh** -- Two-phase block-then-allow pattern. Phase 1 blocks compaction (exit 2), creates handoff. Phase 2 allows compaction, creates recovery marker.
3. **session-end.sh** -- Uses glob `*-${LOOM_STAGE_ID}.md` for stage file lookup (handles depth prefixes)
4. **Signals** -- cache.rs append_common_footer() adds compaction recovery instructions to ALL signal types
5. **post-tool-use.sh** -- Detects compaction recovery marker, prints instructions, removes marker

## macOS Terminal Detection Priority

1. LOOM_TERMINAL env var (explicit override)
2. TERMINAL env var (user preference)
3. Parent process detection (walks process tree up to 10 levels via ps)
4. Cross-platform binary check (ghostty, kitty, alacritty, wezterm via which)
5. macOS native apps (/Applications/Ghostty.app, /Applications/iTerm.app, Terminal.app fallback)

Note: $TERM_PROGRAM is NOT checked.

## find_claude_path() (src/claude.rs)

Shared binary resolution: `which::which("claude")` -> `~/.claude/local/claude` -> `~/.local/bin/claude` -> `~/.cargo/bin/claude` -> `/usr/local/bin/claude` -> `/opt/homebrew/bin/claude`.

## KnowledgeDir API (fs/knowledge/dir.rs)

KnowledgeFile enum: Architecture, EntryPoints, Patterns, Conventions, Mistakes, Stack, Concerns. Core methods: new(root), exists(), initialize(), read(file), read_all(), append(file, content), generate_summary(), list_files().

## Adding New Plan Fields Checklist

1. Add to StageDefinition (plan/schema/types.rs) with serde defaults
2. Add validation in validation.rs
3. Add to Stage model (models/stage/types.rs) with serde defaults
4. Copy in create_stage_from_definition() (commands/init/plan_setup.rs)
5. If goal-check: update has_any_goal_checks() in BOTH StageDefinition and Stage
6. If verification: add verify function in verify/goal_backward/ and call from run_goal_backward_verification()
7. Check ALL test files constructing Stage directly (src/ AND tests/ directories)

## Goal-Backward Verification (verify/goal_backward/) [UPDATED]

Four verification layers for standard stages (truths removed, merged into acceptance):

- **artifacts** -- Files must exist with real implementation (stub detection: TODO, FIXME, unimplemented\!, todo\!)
- **wiring** -- Regex patterns verifying code connections in source files
- **wiring_tests** -- Runtime command-based integration verification
- **dead_code_check** -- Command + pattern detection for unused code

Acceptance criteria (verify/criteria/runner.rs) now handle both:

- **Simple** -- Plain shell command, 5min timeout, exit 0 = pass
- **Extended** -- TruthCheck struct with stdout_contains, stderr_empty, exit_code, 30s timeout

Returns: GoalBackwardResult::Passed | GapsFound | HumanNeeded. Storage: `.work/verifications/<stage-id>.json`.

Note: truths.rs module and verify_truth_checks() are retained for before_stage/after_stage verification (pre/post conditions), NOT for goal-backward.
