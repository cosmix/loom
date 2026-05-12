# Entry Points

> Key files agents should read first to understand the codebase.
>
> **Related files:** [architecture.md](architecture.md) for system overview, [patterns.md](patterns.md) for design patterns.

## CLI Entry Point

- `loom/src/main.rs` - CLI entry (clap `#[derive(Parser)]`), `Commands` enum dispatch
- `loom/src/lib.rs` - Module exports (14 public modules)

## Command Dispatch (cli/types.rs)

| Command       | Entry File                    | Purpose                                      |
| ------------- | ----------------------------- | -------------------------------------------- |
| `init`        | `commands/init/execute.rs`    | Initialize `.work/` from plan                |
| `run`         | `commands/run/mod.rs`         | Start orchestrator daemon                    |
| `status`      | `commands/status.rs`          | Dashboard with stage/session info            |
| `stop`        | `commands/stop.rs`            | Shutdown daemon                              |
| `resume`      | `commands/resume.rs`          | Resume work on a stage                       |
| `sessions`    | `commands/sessions.rs`        | List/kill active sessions                    |
| `worktree`    | `commands/worktree_cmd.rs`    | List/clean/remove worktrees                  |
| `graph`       | `commands/graph/mod.rs`       | Show execution graph                         |
| `hooks`       | `commands/hooks.rs`           | Hook install/list                            |
| `stage`       | `commands/stage/`             | Stage lifecycle (15+ subcommands)            |
| `handoff`     | `commands/handoff/create.rs`  | Create handoff files                         |
| `knowledge`   | `commands/knowledge/mod.rs`   | Manage codebase knowledge                    |
| `memory`      | `commands/memory/handlers.rs` | Session memory journal                       |
| `review`      | `commands/review/mod.rs`      | Generate review docs from memories           |
| `sandbox`     | `commands/sandbox/`           | Suggest/apply sandbox config                 |
| `self-update` | `commands/self_update/mod.rs` | Update loom binary                           |
| `clean`       | `commands/clean.rs`           | Clean up resources                           |
| `repair`      | `commands/repair.rs`          | Fix workspace issues                         |
| `map`         | `commands/map.rs`             | Codebase structure analysis                  |
| `diagnose`    | `commands/diagnose.rs`        | Stage failure diagnosis                      |
| `plan verify` | `commands/plan/verify.rs`     | Validate plan file without side effects      |
| `verify`      | `commands/verify.rs`          | Goal-backward verification                   |
| `check`       | `commands/check.rs`           | Goal-backward verification (alias)           |
| `completions` | `commands/completions/mod.rs` | Shell completions (custom scripts + dynamic) |
| `container`   | `commands/container/mod.rs`   | Container image management (build/rebuild/doctor/shell/logs/list) |
| `complete`    | Hidden (dynamic completions)  | Backend for shell tab completions            |

Total: 23 visible commands + 1 hidden (complete for dynamic completions). Dispatch: `cli/dispatch.rs` match-based, two-level for nested commands.

## Orchestrator Core

- `orchestrator/core/orchestrator.rs` - Main loop (5s polling)
- `orchestrator/core/stage_executor.rs` - Worktree creation, signal gen, session spawn
- `orchestrator/core/event_handler.rs` - Dispatches StageCompleted, SessionCrashed, etc.
- `orchestrator/core/crash_handler.rs` - Failure classification, exponential backoff
- `orchestrator/core/completion_handler.rs` - Auto-merge BEFORE marking completed
- `orchestrator/core/merge_handler.rs` - Conflict detection, merge session spawning
- `orchestrator/core/persistence.rs` - Load/save state to disk

## Data Models

- `models/stage/types.rs` - Stage struct, StageStatus enum (12 states)
- `models/stage/transitions.rs` - State transition validation
- `models/stage/methods.rs` - Stage operations (try_mark_executing, try_complete, timing)
- `models/session/types.rs` - Session struct, SessionStatus enum (6 states)
- `models/failure.rs` - FailureType enum (10 variants, retryable vs non-retryable)

## Plan Parsing Pipeline

- `plan/parser.rs` - Markdown plan parser (extracts YAML from `<!-- loom METADATA -->`)
- `plan/schema/types.rs` - LoomMetadata, StageDefinition structs
- `plan/schema/validation.rs` - Stage validation (goal-backward required for Standard only)
- `plan/graph/mod.rs` - Execution DAG with cycle detection

## Git Operations

- `git/worktree/operations.rs` - Create/remove worktrees at `.worktrees/{stage-id}/`
- `git/worktree/base.rs` - Base branch resolution for dependencies
- `git/worktree/settings.rs` - Worktree symlinks (.work, .claude/CLAUDE.md, CLAUDE.md)
- `git/merge/mod.rs` - Merge automation, conflict handling; `require_no_active_merge` guard
- `git/merge/in_progress.rs` - Single source of truth for `MERGE_HEAD` detection (handles `.git`-as-file, relative gitdirs, octopus merges)
- `git/merge/lock.rs` - File-based merge lock to serialize concurrent merges
- `git/merge/status.rs` - `check_merge_state` (Merged | Pending | Conflict | BranchMissing | Unknown)
- `git/branch.rs` - Branch creation, deletion, ancestry checks

## File System State

- `fs/work_dir.rs` - `.work/` directory management (initialize, load, main_project_root)
- `fs/stage_files.rs` - Stage file naming (`{depth}-{stage-id}.md`)
- `fs/session_files.rs` - Session file operations
- `fs/knowledge.rs` - Knowledge directory operations
- `fs/memory.rs` - Session memory operations
- `fs/verifications.rs` - Goal-backward verification results

## Daemon

- `daemon/server/core.rs` - DaemonServer struct, socket binding
- `daemon/server/lifecycle.rs` - Daemonization, accept loop, shutdown
- `daemon/protocol.rs` - IPC messages (Request/Response enums, 4-byte length-prefixed JSON)
- `daemon/server/broadcast.rs` - Status/log streaming to clients

## Monitor Subsystem

- `orchestrator/monitor/core.rs` - Coordinates detection, heartbeat, checkpoints
- `orchestrator/monitor/detection.rs` - Stage/session state change detection, budget checks
- `orchestrator/monitor/heartbeat.rs` - Hung detection (300s timeout)
- `orchestrator/monitor/context.rs` - Context health: Green (<50%), Yellow (50-64%), Red (65%+)
- `orchestrator/monitor/failure_tracking.rs` - Consecutive failure escalation

## Signal System

- `orchestrator/signals/generate.rs` - Signal file creation (generate_signal_with_skills)
- `orchestrator/signals/cache.rs` - Stable prefix generation (4 stage-type variants, SHA-256 hash)
- `orchestrator/signals/format/mod.rs` - Full signal formatting (Manus 4-section KV-cache pattern)
- `orchestrator/signals/format/sections.rs` - Section formatters (stable, semi-stable, dynamic, recitation)
- `orchestrator/signals/helpers.rs` - write_signal_file() (disk I/O)
- `orchestrator/signals/types.rs` - EmbeddedContext, DependencyStatus, SandboxSummary
- `orchestrator/signals/knowledge.rs` - generate_knowledge_signal() (knowledge stages)
- `orchestrator/signals/crud.rs` - Signal file CRUD
- `orchestrator/signals/merge.rs` - Merge conflict resolution signals
- `orchestrator/signals/recovery.rs` - Recovery signal generation

## Stage Completion (CLI)

- `commands/stage/complete.rs` - Top-level CLI completion entry; **`route_complete_for_conflicts` is the pure routing test seam** that decides Proceed vs ForceUnsafeAssumeMergedVerified vs SpawnResolver vs RevertAndSpawnResolver vs Refuse before any persistence.
- `commands/stage/merge.rs` - `loom stage merge [--resolved]`; wires `merge_verify` for ancestry check.
- `commands/stage/merge_resolver.rs` - CLI-side resolver spawn; uses `find_live_merge_session_for_stage` for single-resolver-per-stage guard.
- `commands/stage/merge_verify.rs` - `verify_or_derive_completed_commit` (read-only ancestry check shared by `--assume-merged` and `--resolved`).
- `orchestrator/merge_attribution.rs` - `attribute_main_repo_merge` and `reconcile_main_repo_active_merge` (free functions; the daemon-recovery test seam — no `Orchestrator` instance required).

## Verification System

- `verify/criteria/runner.rs` - Acceptance criteria execution (run_acceptance) + detect_stderr_warnings()
- `verify/criteria/executor.rs` - Single criterion with timeout
- `verify/goal_backward/mod.rs` - Goal-backward verification (truths, artifacts, wiring)
- `verify/transitions/state.rs` - Atomic stage status changes
- `verify/baseline/` - Change impact detection (capture, compare)
- `verify/before_after.rs` - Before/after stage checks using TruthCheck definitions

## Terminal Backend

- `orchestrator/terminal/mod.rs` - TerminalBackend trait (spawn/kill/alive) + BackendType re-export
- `orchestrator/terminal/dispatcher.rs` - BackendDispatcher; routes spawn/kill/liveness by backend; BackendNeeds declares which backends to construct
- `orchestrator/terminal/native/spawner.rs` - Claude Code session spawning (native)
- `orchestrator/terminal/emulator.rs` - 11 terminal emulator configs
- `orchestrator/terminal/native/detection.rs` - Auto-detect terminal
- `orchestrator/terminal/native/pid_tracking.rs` - Wrapper script, PID tracking, env vars
- `orchestrator/terminal/container/mod.rs` - ContainerBackend (~975 lines — refactor candidate; split candidates: spawn_common, mount construction, env building); spawn/kill/liveness for containerised sessions
- `orchestrator/terminal/container/fingerprint.rs` - compute_fingerprint(); encodes langs + Dockerfile.tmpl + firewall.sh SHA-256
- `orchestrator/terminal/container/image.rs` - Global image cache + per-project digest pin
- `orchestrator/terminal/container/lifecycle.rs` - Container run args and mount construction (ro-base + per-stage rw overlays via build_mounts())
- `orchestrator/terminal/container/logs_capture.rs` - capture_logs() + persist_log(); wraps `<runtime> logs --tail=N`, used on crash/kill before container removal
- `orchestrator/terminal/container/network.rs` - Per-stage network create + allowlist materialisation
- `orchestrator/terminal/container/probe.rs` - Firewall enforcement smoke test; runs transient container post-build with empty allowlist, verifies egress is blocked
- `orchestrator/terminal/container/resources.rs` - Embedded Dockerfile.tmpl + firewall.sh access
- `orchestrator/terminal/container/runtime.rs` - Docker/Podman/Apple Container detection (is_apple_container checks binary + version output)
- `orchestrator/liveness.rs` - LivenessService: wraps BackendDispatcher for monitor thread; fixed_for_tests() stub for unit tests

## Handoff System

- `commands/handoff/create.rs` - CLI `loom handoff create` implementation
- `handoff/detector.rs` - Context threshold detection
- `handoff/generator/mod.rs` - Handoff file generation
- `handoff/schema.rs` - HandoffV2 structured format

## Sandbox

- `sandbox/config.rs` - MergedSandboxConfig, merge_config(), expand_paths()
- `sandbox/settings.rs` - generate_settings_json(), write_settings()

## Hooks

- `hooks/*.sh` - Shell scripts (commit-guard.sh, commit-filter.sh, etc.)
- `fs/permissions/hooks.rs` - install_loom_hooks()
- `fs/permissions/settings.rs` - ensure_loom_permissions(), create_worktree_settings()
- `fs/permissions/constants.rs` - Embedded hook scripts via include_str!()
- `orchestrator/hooks/config.rs` - HookEvent enum
- `orchestrator/hooks/generator.rs` - setup_hooks_for_worktree()

## Schema-to-Runtime Conversion

- `plan/schema/types.rs` - StageDefinition (YAML input); SandboxConfig + StageSandboxConfig with `permission_mode: Option<PermissionMode>`
- `plan/schema/execution.rs` - BackendType enum (canonical definition); PlanExecutionConfig, ProjectExecutionConfig, PlanContainerConfig, ProjectContainerConfig, NetworkConfig
- `models/stage/types.rs` - Stage (runtime model)
- `commands/init/plan_setup.rs` - create_stage_from_definition(), detect_stage_type(); validate_config() called here for backend compatibility check

## Container Subcommand Implementations

- `commands/container/mod.rs` - ContainerCommands enum + dispatch (build, rebuild, doctor, shell, logs, list)
- `commands/container/logs.rs` - `loom container logs <stage-id>` — scans `.work/sessions/` for a container-backed session matching the stage, then execs into `<runtime> logs [-f] [--tail N] <name>`. Key helper: `resolve_session_for_stage(sessions_dir, stage_id) -> ResolvedTarget` (also unit-tested).
- `commands/container/list.rs` - `loom container list [--all] [--json]` — enumerates `.work/sessions/` for container-backed sessions and queries each runtime for live status via `inspect`. Default: running containers only; `--all` includes exited/removed. JSON output uses keys: `stage`, `container`, `runtime`, `status`, `session_id`. Key helper: `query_container_status(runtime, name)` returns `"running"`, `"exited"`, `"missing"`, or `"error: ..."` (reusable in logs.rs/shell.rs for liveness pre-flight).
- `commands/container/shell.rs` - `loom container shell <stage-id>` — resolves the running container for a stage then `exec`s `<runtime> exec -it <name> /bin/bash`. Takes a positional `<stage-id>` argument (NOT `--stage`).

## CLI Subcommand Registration Pattern

Three files to add a new subcommand:

1. `cli/types_memory.rs` - Define variant in KnowledgeCommands/MemoryCommands enum
2. `cli/dispatch.rs` - Add dispatch match arm
3. `commands/<module>/` - Implement handler

## Other Modules

- `src/claude.rs` - Shared find_claude_path() utility
- `completions/generator.rs` - Custom shell script generation (bash/zsh/fish)
- `completions/dynamic/mod.rs` - Context-aware dynamic completion engine
- `completions/dynamic/commands.rs` - Per-command completion definitions
- `completions/scripts/` - Shell-specific completion script templates
- `completions/install.rs` - Auto-install and migration for shell completions
- `commands/status/ui/tui.rs` - TUI dashboard entry (run_tui)
- `commands/self_update/mod.rs` - Installation, update, skill download
- `process/mod.rs` - PID liveness check (libc::kill(pid, 0))
- `skills/` - SkillIndex, SkillMatch, SkillMetadata (index.rs, matcher.rs, types.rs)
- `diagnosis/signal.rs` - generate_diagnosis_signal(), DiagnosisContext
- `map/analyzer.rs` - analyze_codebase(root, deep, focus)

## Key Config Files

- `.work/config.toml` - Active plan reference and settings; `[project_execution]` section holds container image digest + forward_credentials + backend default
- `.work/stages/{depth}-{stage-id}.md` - Stage state (YAML frontmatter)
- `.work/sessions/{session-id}.md` - Session tracking; `backend` field persists resolved BackendType
- `.work/signals/{session-id}.md` - Agent instruction signals
- `.work/network/allowed_domains.txt` - Host-side container network allowlist (mounted ro into container)
- `doc/plans/PLAN-*.md` - Plan definition files
- `loom/resources/Dockerfile.tmpl` - Embedded container image template (handlebars)
- `loom/resources/firewall.sh` - Embedded container firewall script (image-resident, applied at startup)
- `loom/resources/entrypoint.sh` - Container entrypoint
- `~/.local/share/loom/images/<fingerprint>.json` - Global image cache (host-wide)

## Verification System [UPDATED]

- `verify/criteria/runner.rs` - Acceptance criteria execution: handles AcceptanceCriterion::Simple (5min) and Extended (30s + output checks) + detect_stderr_warnings()
- `verify/criteria/executor.rs` - Single criterion with timeout, SIGKILL on timeout
- `verify/goal_backward/mod.rs` - Goal-backward verification (artifacts, wiring, wiring_tests, dead_code) — truths removed from goal-backward
- `verify/goal_backward/truths.rs` - verify_truth_checks() retained for before_after.rs only
- `verify/transitions/state.rs` - Atomic stage status changes
- `verify/baseline/` - Change impact detection (capture, compare)
- `verify/before_after.rs` - Before/after stage checks using TruthCheck definitions

## Shared Hook Utility

- `hooks/_common.sh` - Source guard + `strip_embedded_content()` — sourced by all PreToolUse hooks. MUST be installed alongside hooks (in `~/.claude/hooks/loom/`). Registered in `constants.rs` as `HOOK_COMMON`.

## Plan Validation Functions (plan/schema/validation.rs)

Key public functions for `loom plan verify` to call:

| Function                                            | Return                             | Severity                                       |
| --------------------------------------------------- | ---------------------------------- | ---------------------------------------------- |
| `validate(&metadata)`                               | `Result<(), Vec<ValidationError>>` | Fatal — called by `parse_plan()` automatically |
| `validate_structural_preflight(&stages, repo_root)` | `Vec<String>`                      | Advisory warnings                              |
| `check_knowledge_recommendations(&stages)`          | `Vec<String>`                      | Advisory suggestions                           |
| `check_sandbox_recommendations(&metadata)`          | `Vec<String>`                      | Advisory suggestions                           |

`validate()` runs inside `parse_and_validate()` → called by `parse_plan_content()` → called by `parse_plan()`. Any new command that calls `parse_plan()` automatically gets fatal validation for free.

## Plan Parser Module (plan/parser/mod.rs)

**Note:** `plan/parser` is a **subdirectory**, not a single file. Entry point is `plan/parser/mod.rs`.

- `parse_plan(path: &Path) -> Result<ParsedPlan>` — reads file + validates
- `parse_plan_content(content: &str, source_path: &Path) -> Result<ParsedPlan>` — for tests without I/O

`ParsedPlan` fields: `id` (from filename stem), `name` (first H1), `source_path`, `stages: Vec<StageDefinition>`, `metadata: LoomMetadata`.

Internal modules: `extraction.rs` (YAML block extraction, plan name), `validation.rs` (YAML parse + `validate()`).

## Execution Graph Build (plan/graph/mod.rs)

- `ExecutionGraph::build(stages: Vec<StageDefinition>) -> Result<Self>` — two-pass: first creates nodes, second builds reverse-dependency edges, then calls `cycle::detect_cycles()` via DFS
- `ExecutionGraph::update_ready_status()` → returns stage IDs that became `Queued`
- Cycle detection: `cycle/mod.rs` uses recursive DFS with `visiting` / `visited` sets; returns `Err` with cycle path on detection
- `plan/graph/loader.rs` has `build_execution_graph()` that loads stage files from `.work/stages/` and calls `ExecutionGraph::build()`

## Hook System (loom/src/hooks/)

- `hooks/mod.rs` - Module root; re-exports `HookEvent`, `HooksConfig`, `generate_hooks_settings`, `setup_hooks_for_worktree`, `setup_container_main_session_settings`, `find_hooks_dir`
- `hooks/config.rs` - `HookEvent` enum (6 variants) + `HooksConfig` struct + `to_settings_hooks()`
- `hooks/generator.rs` - `generate_hooks_settings()` (merge session hooks into settings.json), `setup_hooks_for_worktree()`, `setup_container_main_session_settings()`, `find_hooks_dir()`
- `hooks/events.rs` - `log_hook_event()`, `read_recent_events()`, event log CRUD
- `hooks/validators/` - Validator scripts for PreToolUse hooks (commit-filter, git-add-guard, worktree-isolation, prefer-modern-tools)

**6 hook events:**
| Event | Script | Purpose |
| --- | --- | --- |
| `SessionStart` | `session-start.sh` | Initial heartbeat |
| `PostToolUse` | `post-tool-use.sh` | Heartbeat update after every tool call |
| `PreCompact` | `pre-compact.sh` | Trigger handoff before context compaction |
| `SessionEnd` | `session-end.sh` | Cleanup on normal exit |
| `Stop` | `learning-validator.sh` | Memory usage check on stop |
| `PreferModernTools` | `prefer-modern-tools.sh` | Suggest fd/rg over find/grep in Bash |

**Settings placement:** Session hooks → `<worktree>/.claude/settings.local.json`. Global hooks (commit-filter, git-add-guard, worktree-isolation) configured via `fs/permissions.rs:configure_loom_hooks()` / `configure_loom_hooks_for_container()`.

**Container path difference:** Native hooks use host-absolute paths (`~/.claude/hooks/loom/<script>`); container hooks use fixed container mount path `/home/loom/.claude/hooks/loom/<script>`.

**Non-worktree container sessions** (knowledge, merge, base-conflict): settings written to `.work/container-settings/<session_id>.local.json` (NOT `.claude/settings.local.json`) to avoid corrupting the host operator's settings. The file is ro-mounted into the container at the expected location.

**Env vars injected via settings.json env block:**
- `LOOM_STAGE_ID` — current stage ID
- `LOOM_SESSION_ID` — current session ID
- `LOOM_WORK_DIR` — host path for native, `/repo/.work` for container

**LOOM_MAIN_AGENT_PID:** Explicitly REMOVED from settings.json env in `generator.rs`. Must be set dynamically by the wrapper script (`export LOOM_MAIN_AGENT_PID=$$`) so it reflects the actual Claude process PID. A stale value from a previous session would cause commit-filter.sh to misidentify the main agent as a subagent.

**Hooks discovery:** `find_hooks_dir()` checks `$LOOM_HOOKS_DIR` env first, then `~/.claude/hooks/loom/`. Returns `None` if not installed.

**Permissions:** Absolute paths use `//` prefix in allow entries (e.g., `Read(//home/user/.work/signals/**)`). Single `/` means project-relative — wrong for `.work/` which resolves outside the worktree due to symlink.

## Container Logs / Shell Commands (commands/container/logs.rs)

Both `loom container logs <stage-id>` and `loom container shell <stage-id>` share session-lookup logic:

1. Scan `.work/sessions/*.md` for sessions with matching `stage_id`, `backend: container`, and a populated `container_name`
2. Pick newest by `last_active` timestamp (multiple sessions: newest wins)
3. Verify container state via `<runtime> inspect -f '{{.State.Status}}' <name>`
4. `exec()` into `<runtime> logs` / `<runtime> exec -it` — replaces the loom process so Ctrl-C, stdout buffering, and signal handling work natively

**Key difference between logs and shell:**
- `logs`: accepts running OR exited containers (`require_running=false`) — useful post-crash
- `shell`: requires `Running` state (`require_running=true`); exited containers suggest `loom container logs <stage-id>` instead

**Container missing error:** Directs user to `.work/crashes/` for captured logs, or `loom container list` to see what's running.

**Runtime detection:** If session file has no `runtime` field, falls back to `rt::detect_runtime("auto")`.

## Status Command (commands/status/)

- `commands/status.rs` - Entry point; dispatches to 3 modes
- `commands/status/data.rs` - `collect_status_data()` — loads stages, sessions, plan into `StatusData`
- `commands/status/render/` - Renderers: `render_progress()`, `render_graph()`, `render_merge_status()`, `render_compact()`, `render_attention()`
- `commands/status/ui/` - TUI for `--live` mode (subscribes to daemon via IPC)
- `commands/status/diagnostics.rs` - `check_directory_structure()`, `check_parsing_errors()` for `loom status validate` / `doctor`
- `commands/status/display.rs` - `count_files()` helper
- `commands/status/merge_status.rs` - Merge section data
- `commands/status/validation.rs` - `validate_markdown_files()`, `validate_references()`

**3 display modes:**
| Mode | Flag | Behavior |
| --- | --- | --- |
| Static (default) | none | Snapshot: logo → plan name → daemon indicator → progress bar → stage graph → merge status |
| Compact | `--compact` | Single-line scripting output via `render_compact()` |
| Live | `--live` | TUI subscribed to daemon IPC; requires daemon running (`DaemonServer::is_running()`) |

**Verbose mode (`--verbose`):** Shows `render_attention()` — detailed failure information for blocked/failed stages.
