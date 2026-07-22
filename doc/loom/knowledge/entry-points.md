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
| `complete`    | Hidden (dynamic completions)  | Backend for shell tab completions            |

Total: 22 visible commands + 1 hidden (complete for dynamic completions). Dispatch: `cli/dispatch.rs` match-based, two-level for nested commands.

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

- `orchestrator/terminal/mod.rs` - terminal module root; re-exports `TerminalEmulator`
- `orchestrator/terminal/native/mod.rs` - NativeBackend (spawn/kill/alive)
- `orchestrator/terminal/native/spawner.rs` - Claude Code session spawning (native)
- `orchestrator/terminal/emulator.rs` - 11 terminal emulator configs
- `orchestrator/terminal/native/detection.rs` - Auto-detect terminal
- `orchestrator/terminal/native/pid_tracking.rs` - Wrapper script, PID tracking, env vars
- `orchestrator/liveness.rs` - LivenessService: wraps NativeBackend for monitor thread; fixed_for_tests() stub for unit tests

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
- `models/stage/types.rs` - Stage (runtime model)
- `commands/init/plan_setup.rs` - create_stage_from_definition(), detect_stage_type()

## CLI Subcommand Registration Pattern

Three files to add a new subcommand:

1. `cli/types_memory.rs` - Define variant in KnowledgeCommands/MemoryCommands enum
2. `cli/dispatch.rs` - Add dispatch match arm
3. `commands/<module>/` - Implement handler

## Remote Control Module

- `loom/src/remote_control.rs` - `resolve(work_dir)` per-spawn gate, `preflight(path)`, `write_unsupported_marker(work_dir)`, `run_startup_preflight(path, work_dir)`, `RemoteControlConfig` / `RemoteControlMode` types

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

- `.work/config.toml` - Active plan reference and settings
- `.work/stages/{depth}-{stage-id}.md` - Stage state (YAML frontmatter)
- `.work/sessions/{session-id}.md` - Session tracking
- `.work/signals/{session-id}.md` - Agent instruction signals
- `doc/plans/PLAN-*.md` - Plan definition files

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
- `load_stage_definition_from_plan(work_dir, stage_id) -> Result<StageDefinition>` — reads config.toml for plan path, resolves path, parses plan, finds stage by ID. Centralized here after PLAN-anti-slop-thoroughness; was previously inlined in commands/verify.rs and re-inlined in generate.rs.

`ParsedPlan` fields: `id` (from filename stem), `name` (first H1), `source_path`, `stages: Vec<StageDefinition>`, `metadata: LoomMetadata`.

Internal modules: `extraction.rs` (YAML block extraction, plan name), `validation.rs` (YAML parse + `validate()`).

## Execution Graph Build (plan/graph/mod.rs)

- `ExecutionGraph::build(stages: Vec<StageDefinition>) -> Result<Self>` — two-pass: first creates nodes, second builds reverse-dependency edges, then calls `cycle::detect_cycles()` via DFS
- `ExecutionGraph::update_ready_status()` → returns stage IDs that became `Queued`
- Cycle detection: `cycle/mod.rs` uses recursive DFS with `visiting` / `visited` sets; returns `Err` with cycle path on detection
- `plan/graph/loader.rs` has `build_execution_graph()` that loads stage files from `.work/stages/` and calls `ExecutionGraph::build()`

## Hook System (loom/src/hooks/)

- `hooks/mod.rs` - Module root; re-exports `HookEvent`, `HooksConfig`, `generate_hooks_settings`, `setup_hooks_for_worktree`, `find_hooks_dir`
- `hooks/config.rs` - `HookEvent` enum (6 variants) + `HooksConfig` struct + `to_settings_hooks()`
- `hooks/generator.rs` - `generate_hooks_settings()` (merge session hooks into settings.json), `setup_hooks_for_worktree()`, `find_hooks_dir()`
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

**Settings placement:** Session hooks → `<worktree>/.claude/settings.local.json`. Global hooks (commit-filter, git-add-guard, worktree-isolation) configured via `fs/permissions.rs:configure_loom_hooks()`.

**Env vars injected via settings env block:**

- `LOOM_WORK_DIR` — path to `.work/` directory (the ONLY loom var persisted; stable per repo)

**Per-session identity (LOOM_MAIN_AGENT_PID, LOOM_STAGE_ID, LOOM_SESSION_ID):** Explicitly REMOVED from all settings env blocks (`scrub_session_identity_env` in `fs/permissions/settings.rs`). Set ONLY by the wrapper script exports so they always reflect the running session — settings env overrides process env, so persisted values from an earlier session would shadow the fresh exports (see mistakes.md 2026-07-22).

**Hooks discovery:** `find_hooks_dir()` checks `$LOOM_HOOKS_DIR` env first, then `~/.claude/hooks/loom/`. Returns `None` if not installed.

**Permissions:** Absolute paths use `//` prefix in allow entries (e.g., `Read(//home/user/.work/signals/**)`). Single `/` means project-relative — wrong for `.work/` which resolves outside the worktree due to symlink.

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

## Tool Event Log

`.work/tool-events.jsonl` — written by `hooks/post-tool-use.sh` on every tool call. Used by the monitor subsystem for stuck-session detection.

**Writer:** `hooks/post-tool-use.sh` lines 74-107 (TOOL EVENT LOGGING section). Requires `jq` to be available — the block is guarded by `command -v jq` so heartbeat writes are never blocked.

**Reader / Rust struct:** `loom/src/hooks/events.rs::ToolEvent` (line 190) — `read_tool_events(work_dir)` and `tail_tool_events(work_dir, n)`.

**ToolEvent fields:**

```rust
pub struct ToolEvent {
    pub ts: String,            // ISO 8601 timestamp
    pub tool: String,          // Tool name (e.g. "Bash", "Read")
    pub is_error: bool,
    pub session_id: String,
    pub stage_id: String,
    pub exit: Option<i32>,         // Bash tool exit code, null for others
    pub output_bytes: Option<u64>, // printf '%s' | wc -c (no trailing newline)
    pub output_head: Option<String>, // First ~200 bytes
    pub output_tail: Option<String>, // Last ~200 bytes
}
```

**Note:** `output_bytes` uses `printf '%s' | wc -c` (not `echo | wc -c`) so empty output is 0, not 1. This was a bug fixed in integration-verify — the old `echo` appended a newline making empty output record `output_bytes=1`, breaking the failure heuristic in `tool_analysis.rs:101`.

**Distinct from** `.work/hooks/events.jsonl` (HookEventLog struct in the same file) — that file logs session lifecycle hook events (SessionStart, PreCompact, SessionEnd, Stop). Both types live in `loom/src/hooks/events.rs`.

**Consumer:** `orchestrator/monitor/tool_analysis::analyze_session(work_dir, session_id)` reads the last 50 events for a session and computes `ToolAnalysis` (stuck detection). See `architecture.md § Soft Signals` for the full pipeline.

## Orchestrator Core Recovery Functions (Exact Locations)

| Function | File | Lines | Called From |
|----------|------|-------|-------------|
| `sync_graph_with_stage_files()` | `orchestrator/core/recovery.rs` | 179-567 | orchestrator.rs main loop (tick 2) |
| `sync_queued_status_to_files()` | `orchestrator/core/recovery.rs` | 569-593 | orchestrator.rs main loop (tick 3) |
| `recover_orphaned_sessions()` | `orchestrator/core/recovery.rs` | 595-791 | startup init only |
| `reconcile_and_update_graph()` | `orchestrator/core/recovery.rs` | 149-177 | orchestrator.rs (tick 1 + startup) |
| `spawn_merge_resolution_sessions()` | `orchestrator/core/merge_handler.rs` | 637-758 | orchestrator.rs (tick 4) |
| `start_ready_stages()` | `orchestrator/core/stage_executor.rs` | 64-86 | orchestrator.rs (tick 6) |

## Plan Graph Loader — Stage File Preference (Critical)

`plan/graph/loader.rs:56` — `build_graph_impl()`:

- **Lines 60-86**: Prefers `.work/stages/` over plan file. If stages_dir exists with .md files → load from `fs::load_stages_from_work_dir()` + recover sandbox from `.work/config.toml [plan_sandbox]`. Falls back to parsing plan file only if stages_dir is empty/missing.
- This means plan-file amendments are NOT automatically reflected until stages_dir is absent (i.e., fresh init). Plan-amendment stage MUST update `.work/stages/<id>.md` files in addition to the plan file.

## Plan Schema — StageDefinition Amendable Fields

`plan/schema/types.rs:306` — `StageDefinition` struct:

- Line 316: `acceptance: Vec<AcceptanceCriterion>` — amendable in v1
- Line 336: `wiring: Vec<WiringCheck>` — amendable in v1
- Line 347/352: `before_stage`/`after_stage: Vec<TruthCheck>` — deferred to v2
- Line 333: `artifacts: Vec<String>` — deferred to v2
- NOT amendable: `id`, `name`, `dependencies`, `working_dir`, `model`, `sandbox`, `execution`

## WorkDir Directory Helpers (Existing vs. Missing)

`fs/work_dir.rs:270-294` — existing helpers:

- `signals_dir()` → `.work/signals/`
- `handoffs_dir()` → `.work/handoffs/`
- `archive_dir()` → `.work/archive/`
- `stages_dir()` → `.work/stages/`
- `sessions_dir()` → `.work/sessions/`
- `crashes_dir()` → `.work/crashes/`
- `knowledge_dir()` → `.work/knowledge/`
- `ensure_dir(&self, name: &str) -> Result<PathBuf>` — create any subdir on demand

**Both helpers are now implemented:** `disputes_dir()` → `.work/disputes/` (`fs/work_dir.rs:239`) and `plan_versions_dir()` → `.work/plan_versions/` (`fs/work_dir.rs:244`)

## Sandbox Settings — ANTHROPIC_API_KEY

`sandbox/settings.rs:16-34` — `SENSITIVE_ENV_KEYS` array filters `ANTHROPIC_API_KEY` from agent sandbox environments.

- When `ANTHROPIC_API_KEY` is absent at daemon startup: adjudication disabled; disputed stages go directly to `NeedsHumanReview`

## HTTP Client Pattern — self_update/client.rs

`commands/self_update/client.rs` — `create_http_client() -> Result<Client>`:

- `Client::builder().connect_timeout(10s).timeout(120s).user_agent("loom-self-update").build()`
- `validate_response_status(&response, context)` — checks `is_success()`, returns descriptive HTTP errors
- Streaming download with size limit enforcement (buffer size 8192)
- Error propagation: `.context("Failed to ...")` pattern throughout

Adjudicator HTTP client should mirror this pattern with `user_agent("loom-adjudicator")` and longer timeout (~120s for Claude API latency).

## Admin Token Write Location

`daemon/server/lifecycle.rs:176-182`:

- Generates 32-byte (256-bit) hex token
- Writes to `<work_dir>/admin.token` (`.work/admin.token`)
- Mode 0o600 (owner-only rw)

## Daemon Capability Surface (client.rs)

`daemon/server/client.rs` — `verify_for_capability(work_dir, token, Capability) -> bool`:

- Routes to USER_TOKEN_FILE or ADMIN_TOKEN_FILE via `token_path_for()`
- Missing file → returns false (fails closed)
- Constant-time comparison via `ct_eq()`

`daemon/protocol.rs:83-97` — `Capability` enum:

- `User` — Ping, Subscribe, Unsubscribe
- `Admin` — Stop; all `--no-verify`, `--force-unsafe`, `--assume-merged` paths also gate on `Admin`

## Dispute Criteria — Current Implementation

`commands/stage/dispute_criteria.rs:16` — `dispute_criteria(stage_id, reason) -> Result<()>`:

- Only accepts stages in `Executing` or `CompletedWithFailures` state
- `CompletedWithFailures` → two-step: → `Executing` → `NeedsHumanReview`
- `Executing` → direct `NeedsHumanReview`
- Stores reason in `stage.review_reason: Option<String>`
- Stage 2 replaces this with structured `DisputeRequest` RPC payload + `NeedsAdjudication` state

## Fix Attempts Counter — Current Usage

`models/stage/types.rs:254` — `fix_attempts: u32` field:

- Incremented: `commands/stage/check_acceptance.rs:110` when criteria fail
- Reset to 0: `commands/stage/human_review.rs:87` on human approve
- Default max: 3 (via `get_effective_max_fix_attempts()` in methods.rs)
- Warning printed when limit reached with hint to `loom stage dispute-criteria`

Stage 2/3 adds alongside: `dispute_count`, `evidence_rounds`, `amendments_applied` fields.

## Remote Control & Permission Mode Integration Points

> **Note:** Any references to a "container backend" or `BackendType` elsewhere in these knowledge
> files are **stale** — the container backend was removed in commits 5bcf5d8 / c2f16bb.
> All session spawning is now done exclusively through the native backend.

### 1. PermissionMode enum — import and defaults

- `loom/src/sandbox/config.rs:1-4` — `use crate::plan::schema::{..., PermissionMode, StageType, ...}` (no `BackendType` in scope anywhere in this file)
- `loom/src/sandbox/config.rs:49-54` — `default_mode_for(stage_type: StageType) -> PermissionMode`
  - ALL stage types (Knowledge, KnowledgeDistill, Standard, IntegrationVerify) → `Auto`
- `loom/src/sandbox/config.rs:60-95` — `merge_config(plan, stage, stage_type)` — precedence: stage > plan > `default_mode_for`
- `loom/src/sandbox/config.rs:102-112` — `validate_config(merged)` — rejects `BypassPermissions` unconditionally
- `loom/src/sandbox/config.rs:461-475` — `test_default_mode_for_stage_type`
- `loom/src/sandbox/config.rs:478-511` — `test_merge_config_permission_mode_precedence`

### 2. permission mode delivery — TWO mechanisms (settings file is NOT enough for `auto`)

- `loom/src/sandbox/settings.rs:15-26` — `apply_default_mode(settings, mode)` maps loom's kebab-case `PermissionMode` to Claude's camelCase wire format (`"acceptEdits"`, `"bypassPermissions"`, etc.); called at the end of `generate_settings_json()` so every generated `settings.local.json` carries a `permissions.defaultMode`.
- **⚠️ The settings file is IGNORED for `auto`.** Claude Code v2.1.142+ deliberately ignores `permissions.defaultMode: "auto"` when it comes from **project/local** settings (`.claude/settings.json` / `.claude/settings.local.json`) — a repo cannot grant itself auto mode. Only the `--permission-mode` **CLI startup flag** (or user/managed settings) is honored. So the authoritative delivery is the CLI flag emitted in `build_claude_command` (§3), NOT the settings file. The `defaultMode` in settings.local.json is still emitted (harmless; honored for non-`auto` modes) but is redundant given the flag. See mistakes.md "settings.local.json `defaultMode: auto` is silently ignored".

### 3. Claude command-build sites (NativeBackend)

All four public `spawn_*` methods funnel through the single unified `spawn()` in `loom/src/orchestrator/terminal/native/mod.rs`, which calls the shared `build_claude_command()`. Command shape: `{claude} --model {m} --effort {e} --permission-mode {mode} {prompt}[ --remote-control]`.

- **`--permission-mode {mode}`** is resolved inside `spawn()` via `merge_config(read_plan_sandbox(work_dir), stage.sandbox, stage.stage_type).permission_mode` → `.as_settings_value()`. Reads the SAME `[plan_sandbox]` snapshot that `OrchestratorConfig.sandbox_config` loads from, so the CLI flag and the generated settings file never disagree. `validate_config` rejects `bypass-permissions`, so it never reaches the flag.
- Model/effort policy: `spawn_session` / `spawn_knowledge_session` use `stage.effective_model()` + `stage.effective_reasoning_effort()`; `spawn_merge_session` / `spawn_base_conflict_session` hardcode `opus` / `xhigh`.

All four call `pid_tracking::create_wrapper_script()` before `spawn_in_terminal()`.

### 4. Wrapper script — PID tracking template

`loom/src/orchestrator/terminal/native/pid_tracking.rs`:

- `create_wrapper_script()` — lines 250–368
- Wrapper `format\!()` template — lines 321–350
- Key template lines:
  - `:332` — `export LOOM_MAIN_AGENT_PID=$$` (set to shell's own PID before exec)
  - `:337` — `echo $$ > {pid_file}` (writes PID to `.work/pids/{stage_id}.pid`)
  - `:340` — `exec {claude_cmd}` (replaces shell with claude process)
- Template also exports: `LOOM_SESSION_ID`, `LOOM_STAGE_ID`, `LOOM_WORK_DIR`, `LOOM_WORKTREE_PATH`, `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`

### 5. Centralized .work/config.toml API

`loom/src/fs/work_dir.rs` (block starting at line 328):

| Function | Lines | Notes |
|----------|-------|-------|
| `read_config(work_dir)` | 356–366 | Returns `toml_edit::DocumentMut` (preserves comments) |
| `write_config(work_dir, doc)` | 370–381 | Writes doc back; caller must hold any lock |
| `read_section<T>()` (private) | 383–402 | Reads a named `[section]` as typed `T: DeserializeOwned` |
| `write_section<T>()` (private) | 404–429 | Writes typed value into named section, preserving rest |
| `read_plan_sandbox(work_dir)` | 435–437 | Public: reads `[plan_sandbox]` → `Option<SandboxConfig>` |
| `write_plan_sandbox(work_dir, sandbox)` | 440–442 | Public: writes `[plan_sandbox]` |

**Pattern to mirror for new sections:** add a `const SECTION_NAME: &str = "my_section"` at the top, then two public functions calling `read_section` / `write_section`.

### 6. Plan init — where config.toml sections are written

`loom/src/commands/init/plan_setup.rs`:

- `initialize_with_plan()` — lines 27–214
- `[plan]` table written via `toml_edit` at lines 139–157 (`work_dir::read_config` + `doc.insert("plan", ...)` + `work_dir::write_config`)
- `[plan_sandbox]` snapshot written at lines 160–162 (`work_dir::write_plan_sandbox`)

### 7. Run command entry points

- `loom/src/commands/run/mod.rs:31-84` — `execute_background()` — daemonizes orchestrator; calls `DaemonServer::with_config(...).start()`
- `loom/src/commands/run/foreground.rs:17-36` — `execute()` — public entry for `--foreground`; marks plan in-progress then calls `execute_foreground()`
- `loom/src/commands/run/foreground.rs:39-end` — `execute_foreground()` (private) — builds `OrchestratorConfig` (includes `sandbox_config: plan_sandbox` from `build_execution_graph`)

### 8. Crash handler — failure classification and retry

`loom/src/orchestrator/core/crash_handler.rs:15-137` — `handle_session_crashed()`:

- Line 49 — `classify_failure(&reason)` (from `orchestrator/retry.rs`)
- Line 65 — `should_auto_retry(&failure_type, stage.retry_count, max)` (default max = 3)
- Lines 90–110 — best-effort permission sync from crashed session's worktree
- Line 114 — `stage.try_mark_blocked()` → saves stage → marks graph `Blocked`

### 9. Claude binary resolution

`loom/src/claude.rs:10-33` — `find_claude_path() -> Result<PathBuf>`:

1. `which::which("claude")` (uses current PATH)
2. `~/.claude/local/claude` (official Claude Code install location)
3. `~/.local/bin/claude`
4. `~/.cargo/bin/claude`
5. `/usr/local/bin/claude`
6. `/opt/homebrew/bin/claude`

Used by all four NativeBackend spawn methods before building the claude command string.

## Signal Generation — Key Files and Line References

| File | Purpose | Key Lines |
|------|---------|-----------|
| `orchestrator/signals/generate.rs` | Entry point: `generate_signal_with_skills()`, `build_signal_context()`, `build_embedded_context_with_stage_and_session()` | 137-536 |
| `orchestrator/signals/cache.rs` | 4 stable-prefix generators + 8 `append_*` helpers + SignalMetrics SHA-256 | helpers:51-169, standard:174-310, IV:313-444, KnowledgeDistill:447-524, Knowledge:527-633 |
| `orchestrator/signals/format/mod.rs` | `format_signal_with_metrics()` — selects stable prefix by stage type, assembles 4 sections | 62-78 |
| `orchestrator/signals/format/sections.rs` | Semi-stable (15-378), Dynamic (382-661), Recitation (665-765) | see per-section notes |
| `orchestrator/signals/types.rs` | `EmbeddedContext` struct (24-50), `DependencyStatus`, `SandboxSummary` | 24-50 |
| `orchestrator/signals/knowledge.rs` | Knowledge-stage signal path: `generate_knowledge_signal()`, `format_knowledge_signal_content()` | 23-135 |
| `orchestrator/signals/recovery.rs` | Recovery signal: recovery context header, last known state, recovery actions | — |
| `orchestrator/signals/recovery_format.rs` | `format_recovery_signal()` if exists as separate file | — |
| `orchestrator/signals/helpers.rs` | `write_signal_file()` disk I/O | 17+ |
| `orchestrator/signals/crud.rs` | Signal file CRUD | — |

**Insertion point for new shared helper:** `cache.rs` lines 51-169 (the "Shared content blocks" cluster). Call it from each of the 4 generator functions.

## Hook Scripts — What Each Does

| Script | Hook Type | Key Behavior |
|--------|-----------|-------------|
| `session-start.sh` | SessionStart | Writes initial heartbeat; captures stdin and parses `.source` field; on `source == "compact"` or `"resume"` emits `hookSpecificOutput.additionalContext` JSON re-anchor pointer |
| `post-tool-use.sh` | PostToolUse | Updates heartbeat; logs to `.work/tool-events.jsonl`; no longer checks compaction-recovery markers (removed) |
| `pre-compact.sh` | PreCompact | Block-then-allow: first call exits 2 (blocks) + creates pending flag + calls `loom handoff`; second call exits 0 (allows); does NOT create a recovery marker file |
| `session-end.sh` | SessionEnd | Creates handoff if stage not completed |
| `learning-validator.sh` | Stop | Advisory check for session memory usage |
| `commit-guard.sh` | Stop (global) | Blocks exit if uncommitted changes or stage still Executing |
| `prefer-modern-tools.sh` | PreToolUse:Bash | Emits `hookSpecificOutput.additionalContext` JSON warning to use `rg`/`fd` instead |
| `commit-filter.sh` | PreToolUse:Bash | Blocks subagent git commits via LOOM_MAIN_AGENT_PID process tree check; blocks Claude attribution |
| `git-add-guard.sh` | PreToolUse:Bash | Blocks `git add -A`, `git add .`, `git add .work` |
| `worktree-isolation.sh` | PreToolUse:Bash/Edit/Write | Blocks cross-worktree ops and path traversal |
| `worktree-file-guard.sh` | PreToolUse:Read/Glob/Grep | Blocks file tool paths outside worktree |
| `skill-trigger.sh` | UserPromptSubmit | Scores keywords, emits skill suggestions as `hookSpecificOutput.additionalContext` |
| `ask-user-pre.sh` | PreToolUse:AskUserQuestion | Marks stage WaitingForInput |
| `ask-user-post.sh` | PostToolUse:AskUserQuestion | Resumes stage |
| `_common.sh` | Utility | `strip_embedded_content()` (prevents false-positive matches in commit messages), `loom_current_worktree()` (worktree detection by directory, NOT just env var) |

**Worktree detection gotcha:** `_common.sh:loom_current_worktree()` checks TWO conditions — current directory contains `.worktrees/` AND `LOOM_WORKTREE_PATH` points into `.worktrees/` with the directory existing. LOOM_STAGE_ID alone is insufficient (it leaks into plain sessions from prior runs).

## TruthCheck / before_stage / after_stage / code_review

| Location | Purpose |
|----------|---------|
| `models/stage/types.rs:280-303` | `TruthCheck` struct: `command`, `stdout_contains`, `stdout_not_contains`, `stderr_empty`, `exit_code`, `description` |
| `plan/schema/types.rs:100-261` | `StageDefinition`: `before_stage: Vec<TruthCheck>` (221), `after_stage: Vec<TruthCheck>` (226), `code_review: Option<CodeReviewConfig>` (261) |
| `plan/schema/types.rs:100-111` | `CodeReviewConfig`: `dimensions: Vec<String>`, `require_all: bool` |
| `commands/init/plan_setup.rs:280-281` | Copies before_stage + after_stage to Stage; does NOT copy code_review |
| `orchestrator/core/stage_executor.rs:219-256` | Executes before_stage checks BEFORE session spawn; failure → stage Blocked |
| `commands/stage/complete.rs:847-866` | Executes after_stage checks AFTER acceptance criteria; failure → stage stays Executing |
| `verify/before_after.rs` | `run_before_stage_checks()` + `run_after_stage_checks()` — both delegate to `verify_truth_checks()` |
| `verify/goal_backward/truths.rs:16-134` | `verify_truth_checks(checks, working_dir)` → `Vec<VerificationGap>`, 30s timeout per check |

## `loom pressure` — Plan Pressure-Testing Files

- `loom/src/commands/pressure/mod.rs` — the driver. Key fns: `resolve_plan_path` (raw→`doc/plans/` fallback, `is_file()` check, repo-relative `invocation`), `codex_report_path` (`codex-<basename>` sibling), `codex_log_path`/`claude_marker_path` (per-pid temp paths), `plan_steps` (ordered pipeline; `Step::{DeleteReport, Pressure{claude,codex}, Address}` — the `Pressure` variant is the parallel Claude+Codex pair), `claude_args`/`codex_args` (single-source argv builders; `claude_args` injects `completion_instruction(marker)` via `--append-system-prompt`), `render_dry_run`, `classify_exit`/`classify_code`, `run_claude_foreground` (foreground TTY + marker-watch + SIGTERM→SIGKILL, returns `ClaudeOutcome`), `spawn_codex_background`/`wait_codex` (background codex → log + spinner), `should_stop`/`claude_should_stop`, `execute`. Unit tests in `loom/src/commands/pressure/tests.rs`.
- `loom/src/codex.rs` — `find_codex_path()` codex binary resolver (exported via lib.rs).
- `loom/src/cli/types.rs:195` — `Commands::Pressure { plan, rounds (default 2, ≥1), dry_run }`; dispatched at `loom/src/cli/dispatch.rs:178`.
- `commands/{pressure,address,distill}.md` — vendored Claude slash commands (source for `~/.claude/commands/`).
- `codex/skills/pressure/SKILL.md` — vendored Codex pressure skill (source for `~/.codex/skills/pressure/`).
- `install.sh` — `install_commands()` (~line 336) and `install_codex_skill()` (~line 356), called only in the LOCAL (non-curl-pipe) branch of `main()` (~line 619).
