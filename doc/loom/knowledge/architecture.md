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

When `BackendType::Container` is resolved for a stage, `ContainerBackend` spawns the session inside a per-stage container using a **ro-base + per-stage rw overlay** mount topology. `/repo` is mounted read-only as the base layer; explicit rw mounts shadow only the paths a given stage legitimately needs:

| Container path | Host source | Permissions | Who gets this |
| --- | --- | --- | --- |
| `/repo` | host repo root | ro (base) | All sessions |
| `/repo/.worktrees/<stage-id>` | derived from `/repo` | rw overlay | Standard / IntegrationVerify sessions |
| `/repo/doc/loom/knowledge` | derived from `/repo` | rw overlay | Knowledge / KnowledgeDistill sessions |
| `/repo` (full, replaces ro base) | host repo root | rw | Merge / BaseConflict sessions only |
| `/repo/.work/sessions` | derived from `/repo` | rw overlay | All sessions |
| `/repo/.work/memory` | derived from `/repo` | rw overlay | All sessions |
| `/repo/.work/handoffs` | derived from `/repo` | rw overlay | All sessions |
| `/repo/.work/crashes` | derived from `/repo` | rw overlay | All sessions |
| `/repo/.work/wrappers` | derived from `/repo` | rw overlay | All sessions |
| `/repo/.work/pids` | derived from `/repo` | rw overlay | All sessions |
| `/repo/.worktrees/<id>/settings.local.json` | derived from worktree | ro overlay | All sessions |
| `/home/loom/.claude/hooks/loom` | `~/.claude/hooks/loom` | ro | All sessions |
| `/home/loom/.claude/.credentials.json` | `~/.claude/.credentials.json` | ro | Only when `forward_credentials` includes `"claude"` |
| `/etc/loom/network/allowed_domains.txt` | `.work/network/allowed_domains.txt` | ro | All sessions |

**Mount ordering invariant:** The ro base must be the first bind-mount in the `docker|podman run` args. rw overlays on tighter subtree paths must follow. Reversing order silently defeats the ro restriction (later mounts shadow earlier ones). See [mistakes.md — Mount order inversion](mistakes.md#mount-order-inversion-silently-defeats-the-ro-base).

**Merge/BaseConflict exception:** These sessions need full write access to resolve conflicts; no ro base is used. This is intentional and documented in `build_mounts` unit tests.

**settings.local.json ro overlay:** Prevents an agent running inside the container from disabling Claude Code hooks (e.g., removing sandbox restrictions) by overwriting this file.

**Why `/repo`?** Git worktrees store relative symlinks (`.work -> ../../.work`). Mounting the host repo root at a fixed path preserves these symlinks and all git metadata. Stage cwd inside the container: `/repo/.worktrees/<stage-id>`. Merge/knowledge cwd: `/repo`.

**forward_credentials:** Default is `Vec::new()` (empty — no credentials forwarded). Agents inside the container cannot escalate this because `.work/config.toml` is covered by the ro base and is NOT in the rw overlay set. Host-side editing by the operator remains possible. Agents must request credentials via operator action. This is stricter than the plan spec's suggested default of `["claude"]`.

**Firewall (defense-in-depth):** Image-resident `firewall.sh` script configured with: deny IPv6 (AF_INET6), block `169.254.169.254` (cloud metadata), block `127.0.0.0/8` except `127.0.0.1`, deny `*.internal`. Allowlist is host-owned and mounted ro — the agent process inside the container cannot edit it.

**Firewall enforcement smoke test:** `loom init --backend container` runs a transient probe container after image build (using `--cap-drop=ALL --cap-add=NET_ADMIN --cap-add=NET_RAW` + empty allowlist) to verify the firewall actually blocks egress. Pass = request blocked. Fail = firewall is a no-op. Skip with `--allow-insecure-runtime` on rootless Podman or Apple Container runtimes where iptables enforcement is best-effort.

**Image cache model:** Global image cache at `~/.local/share/loom/images/<fingerprint>.json`. Per-project digest pin at `.work/config.toml::[project_execution.container].image_digest`. Fingerprint encodes: detected languages + SHA-256 of `Dockerfile.tmpl` + SHA-256 of `firewall.sh`. Any change to languages or embedded resources invalidates the cache.

**Log capture:** Containers run without `--rm`. On abnormal exit (`wait_until_running` failure or `kill_session`), `logs_capture::capture_logs()` captures trailing stdout+stderr from `<runtime> logs` and persists to `.work/crashes/<stage>-<ts>-<session>.container.log` before `<runtime> rm -f`. Best-effort: log capture never blocks cleanup.

> See [Container Backend — Mount-Topology Hardening Decision](#container-backend--mount-topology-hardening-decision) for full rationale.

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

## Container Backend — Mount-Topology Hardening Decision

**Context:** The initial container backend mounts `/repo` as `rw bind` (full read-write), which gives container sessions write access to `.git/`, sibling worktrees, `doc/plans/`, knowledge files, and `.work/config.toml` (where `forward_credentials` can be escalated for the next stage). The hardening plan narrows this surface area.

**Chosen approach: ro base + per-stage rw overlays**

Mount `/repo` read-only as the base layer, then stack explicit rw mounts back on top for only the paths a given stage legitimately needs:

| Mount | Type | Rationale |
| --- | --- | --- |
| `/repo` (host repo root) | ro (base) | Default deny — container cannot mutate .git/, sibling worktrees, doc/plans/, .work/config.toml |
| `/repo/.worktrees/<stage-id>` | rw | Stage's working tree — all code writes land here |
| `/repo/.work/sessions` | rw | Session log writes |
| `/repo/.work/memory` | rw | `loom memory` writes |
| `/repo/.work/handoffs` | rw | Handoff file writes |
| `/repo/.work/crashes` | rw | Crash report writes |
| `/repo/.work/wrappers` | rw | Wrapper scripts (execute-bit semantics on some runtimes require rw) |
| `/repo/.work/pids` | rw | Wrapper writes PID here |
| `/repo/doc/loom/knowledge` | rw | Knowledge stages only |
| `/repo` (replaces ro base) | rw | Merge / BaseConflict stages only — they need broad write access to resolve conflicts |
| `settings.local.json` (worktree or root) | ro overlay | Prevents agent from disabling hooks from inside the container |

**Why NOT mount only `.worktrees/<stage-id>`:**

Git worktrees use relative symlinks that require the parent tree to exist in the container:

- `.work` symlink inside the worktree points to `../../.work` (relative path — resolves to `/repo/.work` only if `/repo` is mounted)
- `.git` inside the worktree is a file (not a directory) containing `gitdir: ../../.git/worktrees/<stage-id>` — the relative gitdir requires `/repo/.git` to be accessible at the same fixed path

Mounting only `.worktrees/<stage-id>` would break both the shared-state symlink and all git operations inside the container.

**Mount ordering matters:** Later mounts shadow earlier ones on overlapping paths. The ro base must be listed first in the `docker|podman run` args, followed by rw overlays. Reversing this order (rw before ro at the same path) leaves the rw mount effective — a silent security regression.

## Container Backend — Lifetime and Log Capture Decision

**Problem with `run -d --rm`:**

The `--rm` flag causes the container runtime to automatically remove the container as soon as the entrypoint process exits. The `<runtime> logs` command (which reads stdout/stderr) is only usable while the container exists — once removed, the log history is gone forever. When `firewall.sh` or the entrypoint dies early (common on rootless Podman without slirp4netns ≥ 1.2.3, and on Apple Container's limited Linux capability emulation), the crash report gets `log_tail: None` and `log_path: None` — no actionable diagnostic.

**Chosen approach: explicit post-capture removal**

Remove `--rm` from `build_run_args`. Containers now persist after process exit, in the "exited" state. Explicit cleanup occurs:

1. **On `wait_until_running` failure** in `spawn_common`: capture logs → persist to `.work/crashes/<stage>-<ts>-<session>.container.log` → `<runtime> rm -f <name>` (best-effort, errors ignored).
2. **In `kill_session`** (called by crash handler AND by `loom sessions kill`): capture logs → persist → then proceed with existing `rm -f` cleanup.

**Invariant:** Log capture is best-effort — failure to capture must never block container removal. The `persist_log` call is wrapped in `.ok()` / error-logged-to-stderr, and the `rm -f` always runs.

## Container Backend — Naming, Topology, and Deletion Lifecycle

### Container Name Format (per SessionType)

Container names are derived in `models/session/methods.rs::derive_tracking_key()`:

| SessionType    | Container name format            |
| -------------- | -------------------------------- |
| `Stage`        | `loom-<stage-id>`                |
| `Merge`        | `loom-merge-<stage-id>`          |
| `BaseConflict` | `loom-base-conflict-<stage-id>`  |
| `Knowledge`    | `loom-knowledge-<stage-id>`      |

The name is stored in `session.tracking_key` and `session.container_name` (both fields persist to `.work/sessions/<id>.md` so a restarted daemon can still kill/inspect the container).

### Per-Stage Topology (Parallel Containers)

Each stage that resolves to `BackendType::Container` gets **its own dedicated container** for the duration of the session. Parallel stages in the DAG run as parallel containers simultaneously. There is no container sharing between stages. The number of live containers at any point equals the number of concurrently executing container-backed stages.

### Container Deletion Triggers

Containers run **without `--rm`** (explicit post-capture removal to preserve logs). Removal happens at these call sites:

| Trigger                                   | Code path                                                          | Log capture? |
| ----------------------------------------- | ------------------------------------------------------------------ | ------------ |
| Spawn failure (`wait_until_running` fails) | `ContainerBackend::spawn_common` → `capture_logs` → `rm -f`       | Yes          |
| Session killed by orchestrator/user       | `ContainerBackend::kill_session` → `capture_logs` → `rm -f`       | Yes          |
| `loom sessions kill <session>`            | `commands/sessions.rs` → `backend.kill_session` → same path above | Yes          |
| `loom stop` (daemon shutdown)             | orchestrator shutdown path → `kill_session` on all active sessions | Yes          |
| `loom clean [--sessions]`                 | `commands/clean.rs::clean_sessions` → `<runtime> ps -a --filter name=loom-` → `rm -f` each | No (bulk removal) |

**Log capture is always best-effort** — failure to capture never blocks removal.

### Session Files Persist After Container Removal

`.work/sessions/<id>.md` files are **not deleted when the container is removed**. The session file outlives its container. This is the root cause of the "stale session file" bug in `loom container logs`:

- `resolve_session_for_stage()` (`commands/container/logs.rs`) scans `.work/sessions/*.md` for a session with matching `stage_id` and a populated `container_name`.
- If a stage ran, its session file persists with `container_name` set, even after the container was removed.
- A subsequent call to `loom container logs <stage>` will find the stale session file, resolve the container name, then fail at the `<runtime> logs` call because the container no longer exists.
- The fix: `logs` should check container existence (via `<runtime> inspect`) before exec-ing, and fall back to `.work/crashes/` log files for Exited/removed containers.

### Container Session Identity Symmetry (`set_container_identity` / `clear_container_identity`)

`Session` tracks container identity via two paired methods in `models/session/methods.rs`:

| Method | Called when | Effect |
|---|---|---|
| `set_container_identity(runtime, container_name)` | Container is spawned and running | Writes `runtime` + `container_name` fields, persisted to session file |
| `clear_container_identity()` | Container is removed (`kill_session`, spawn error path) | Nils `runtime` + `container_name`, persisted to session file |

**Invariant:** Any call site that removes a container (via `rm -f`) must call `clear_container_identity()` and persist the session file before returning. Without the clear, session files permanently reference removed containers and mislead `loom container logs` / `loom container list`.

**Where enforced:** `ContainerBackend::kill_session` and the `spawn_common` error path.

### `loom container list`

New subcommand (`commands/container/list.rs`) that enumerates `.work/sessions/` and queries each runtime for live container status. Implemented as of the fix-container-backend-ux plan.

| Flag | Behavior |
|---|---|
| *(none)* | Show only running containers |
| `--all` | Show all containers including exited/removed |
| `--json` | Emit JSON Lines; keys: `stage`, `container`, `runtime`, `status`, `session_id` |

Runtime status is queried via `<runtime> inspect -f '{{.State.Status}}' <name>` — not from the session file. Returns `"running"`, `"exited"`, `"missing"`, or `"error: ..."`. This makes the command authoritative even for stale session files.

**Note on session schema keys:** The JSON output uses `stage` (not `stage_id`), `container` (not `container_name`), `status` (not `state`). Tests in `list.rs` assert these exact keys.

**Known tech debt:** `build_rows()` in `list.rs` reimplements session-loading + runtime-detection logic that partially overlaps with `load_sessions()` and `pick_container_session()` helpers in `logs.rs`. These should be consolidated into shared helpers in a future refactor. Detection: `rg 'session.runtime.as_deref'` surfaces 3 sites across `logs.rs` / `list.rs`.
