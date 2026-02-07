# Entry Points

> Key files agents should read first to understand the codebase.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [architecture.md](architecture.md) for system overview, [patterns.md](patterns.md) for design patterns.

## CLI Entry Point

- `loom/src/main.rs` - CLI entry (clap `#[derive(Parser)]`), `Commands` enum dispatch
- `loom/src/lib.rs` - Module exports (14 public modules)

## Command Dispatch

| Command    | Entry File                      | Purpose                           |
| ---------- | ------------------------------- | --------------------------------- |
| `init`     | `commands/init/execute.rs`      | Initialize `.work/` from plan     |
| `run`      | `commands/run/mod.rs`           | Start orchestrator daemon         |
| `status`   | `commands/status.rs`            | Dashboard with stage/session info |
| `stop`     | `commands/stop.rs`              | Shutdown daemon                   |
| `stage`    | `commands/stage/`               | Stage lifecycle (complete, verify) |
| `merge`    | `commands/merge/execute/mod.rs` | Merge completed stages            |
| `memory`   | `commands/memory/handlers.rs`   | Session memory journal            |
| `verify`   | `commands/verify.rs`            | Goal-backward verification        |
| `map`      | `commands/map.rs`               | Codebase structure analysis       |
| `diagnose` | `commands/diagnose.rs`          | Stage failure diagnosis           |
| `attach`   | `commands/attach.rs`            | Attach to running sessions        |
| `hooks`    | `commands/hooks.rs`             | Hook install/list                 |

## Orchestrator Core

- `orchestrator/core/orchestrator.rs` - Main loop (5s polling)
- `orchestrator/core/stage_executor.rs` - Worktree creation, signal gen, session spawn
- `orchestrator/core/event_handler.rs` - Dispatches StageCompleted, SessionCrashed, etc.
- `orchestrator/core/crash_handler.rs` - Failure classification, exponential backoff
- `orchestrator/core/completion_handler.rs` - Auto-merge BEFORE marking completed
- `orchestrator/core/merge_handler.rs` - Conflict detection, merge session spawning
- `orchestrator/core/persistence.rs` - Load/save state to disk

## Data Models

- `models/stage/types.rs` - Stage struct, StageStatus enum (11 states)
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
- `git/merge.rs` - Merge automation and conflict handling
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

- `orchestrator/signals/generate.rs` - Signal file creation
- `orchestrator/signals/cache.rs` - Stable prefix generation (4 stage-type variants, SHA-256 hash)
- `orchestrator/signals/format.rs` - Full signal formatting (Manus 4-section KV-cache pattern)
- `orchestrator/signals/crud.rs` - Signal file CRUD
- `orchestrator/signals/merge.rs` - Merge conflict resolution signals
- `orchestrator/signals/recovery.rs` - Recovery signal generation

## Verification System

- `verify/criteria/runner.rs` - Acceptance criteria execution (run_acceptance)
- `verify/criteria/executor.rs` - Single criterion with timeout
- `verify/goal_backward/mod.rs` - Goal-backward verification (truths, artifacts, wiring)
- `verify/transitions/state.rs` - Atomic stage status changes
- `verify/baseline/` - Change impact detection (capture, compare)

## Terminal Backend

- `orchestrator/terminal/mod.rs` - TerminalBackend trait (spawn/kill/alive)
- `orchestrator/terminal/native/spawner.rs` - Claude Code session spawning
- `orchestrator/terminal/emulator.rs` - 11 terminal emulator configs
- `orchestrator/terminal/native/detection.rs` - Auto-detect terminal
- `orchestrator/terminal/native/pid_tracking.rs` - Wrapper script, PID tracking, env vars

## Handoff System

- `handoff/detector.rs` - Context threshold detection (Yellow=prepare, Red=generate)
- `handoff/generator/mod.rs` - Handoff file generation
- `handoff/schema.rs` - HandoffV2 structured format

## Sandbox

- `sandbox/config.rs` - MergedSandboxConfig, merge_config(), expand_paths()
- `sandbox/settings.rs` - generate_settings_json(), write_settings()

## Hooks

- `hooks/*.sh` - Shell scripts (commit-guard.sh, commit-filter.sh, learning-validator.sh)
- `fs/permissions/hooks.rs` - install_loom_hooks()
- `fs/permissions/settings.rs` - ensure_loom_permissions(), create_worktree_settings()
- `fs/permissions/constants.rs` - Embedded hook scripts, LOOM_PERMISSIONS constants
- `orchestrator/hooks/config.rs` - HookEvent enum (SessionStart, PostToolUse, etc.)
- `orchestrator/hooks/generator.rs` - setup_hooks_for_worktree()

## Schema-to-Runtime Conversion

- `plan/schema/types.rs` - StageDefinition (YAML input)
- `models/stage/types.rs` - Stage (runtime model)
- `commands/init/plan_setup.rs` - create_stage_from_definition(), detect_stage_type()

## Other Modules

- `completions/generator.rs` - Static shell completion (clap_complete)
- `completions/dynamic/mod.rs` - Context-aware dynamic completions
- `commands/status/ui/tui.rs` - TUI dashboard entry (run_tui)
- `commands/status/ui/graph_widget.rs` - DAG visualization
- `CLAUDE.md.template` - Canonical agent rules template
- `commands/self_update/mod.rs` - Installation, update, skill download
- `process/mod.rs` - PID liveness check (libc::kill(pid, 0))
- `utils.rs` - format_elapsed(), format_elapsed_verbose()

## Key Config Files

- `.work/config.toml` - Active plan reference and settings
- `.work/stages/{depth}-{stage-id}.md` - Stage state (YAML frontmatter)
- `.work/sessions/{session-id}.md` - Session tracking
- `.work/signals/{session-id}.md` - Agent instruction signals
- `doc/plans/PLAN-*.md` - Plan definition files

## Skills Module Entry Points

- loom/src/skills/mod.rs — Module exports: SkillIndex, SkillMatch, SkillMetadata
- loom/src/skills/index.rs — SkillIndex::load_from_directory(), match_skills(), parse_skill_file()
- loom/src/skills/matcher.rs — match_skills() algorithm, normalize_text(), split_into_words()
- loom/src/skills/types.rs — SkillMetadata, SkillMatch structs

## Diagnosis Module Entry Points

- loom/src/diagnosis/mod.rs — Module re-export
- loom/src/diagnosis/signal.rs — generate_diagnosis_signal(), load_crash_report(), DiagnosisContext
- loom/src/commands/diagnose.rs — CLI command implementation (loom diagnose <stage-id>)

## Map Module Entry Points

- loom/src/map/mod.rs — Module re-export
- loom/src/map/analyzer.rs — analyze_codebase(root, deep, focus) orchestrator
- loom/src/map/detectors.rs — All detection functions (project type, deps, entry points, structure, conventions, concerns)
- loom/src/commands/map.rs — CLI command (loom map [--deep] [--focus] [--overwrite])

## Signal Generation Entry Points

- loom/src/orchestrator/signals/generate.rs:44 — generate_signal_with_skills() (standard stages)
- loom/src/orchestrator/signals/knowledge.rs:23 — generate_knowledge_signal() (knowledge stages)
- loom/src/orchestrator/signals/format/mod.rs:40 — format_signal_content() (KV-cache optimized formatting)
- loom/src/orchestrator/signals/format/sections.rs — Section formatters (stable, semi-stable, dynamic, recitation)
- loom/src/orchestrator/signals/helpers.rs:17 — write_signal_file() (disk I/O)
- loom/src/orchestrator/signals/types.rs — EmbeddedContext, DependencyStatus, SandboxSummary
- loom/src/orchestrator/core/stage_executor.rs:192 — Signal generation trigger point
- loom/src/orchestrator/core/stage_executor.rs:358 — get_dependency_status() (computes from ExecutionGraph)

## Sandbox Suggest Entry Points

- loom/src/commands/sandbox/suggest.rs — detect_project_and_suggest(), YAML output formatting
- loom/src/commands/sandbox/mod.rs — Sandbox command module
- loom/src/sandbox/config.rs — merge_config() (plan + stage config merging)
- loom/src/sandbox/settings.rs — Claude Code settings.local.json generation
- loom/src/plan/schema/types.rs — SandboxConfig, NetworkConfig, FilesystemConfig schemas

## Handoff CLI Registration (for new commands)

- `loom/src/cli/types.rs:29-238` — Commands enum (23 variants, dispatched in cli/dispatch.rs)
- `loom/src/cli/types_stage.rs` — StageCommands subcommand enum (pattern for nested commands)
- `loom/src/cli/types_memory.rs` — MemoryCommands subcommand enum
- `loom/src/cli/dispatch.rs:16-171` — match-based command dispatch, two-level for nested commands
- Pattern: define parent variant in types.rs with `#[command(subcommand)]`, child enum in types_*.rs, dispatch in dispatch.rs

## Handoff System (Added by integration-verify)

- `loom/src/commands/handoff/create.rs` - CLI command `loom handoff create` implementation
- `loom/src/commands/handoff/mod.rs` - Handoff command module
- `loom/src/cli/types.rs:313` - HandoffCommands enum definition
- `loom/src/cli/dispatch.rs:57-63` - Handoff command dispatch
- `hooks/pre-compact.sh` - Block-then-allow compaction pattern
- `hooks/session-end.sh` - Session end with stage glob lookup
- `hooks/post-tool-use.sh` - Compaction recovery detection
- `loom/src/orchestrator/signals/cache.rs` - Signal stable prefixes with compaction recovery
- `loom/src/orchestrator/signals/format/sections.rs` - Budget warnings with handoff create
