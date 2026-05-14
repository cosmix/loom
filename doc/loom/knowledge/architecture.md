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

**BackendDispatcher** (`orchestrator/terminal/dispatcher.rs`) — routes spawn/kill/liveness calls to the appropriate backend implementation.

**LivenessService** (`orchestrator/liveness.rs`) — replaces scattered `kill -0` checks. Delegates to `BackendDispatcher::is_session_alive()` for backend-aware session liveness probes.

## Data Flow

### Plan Execution Flow

```text
1. loom init doc/plans/PLAN-foo.md
   --> Parse plan, create .work/, write stage files

2. loom run
   --> Spawn daemon (or foreground) --> orchestrator loop

3. Orchestrator loop (5s poll):
   Load stage files --> Build ExecutionGraph --> Find ready stages
   --> Create worktree + signal --> Spawn session --> Monitor via LivenessService

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
- **permission_mode field** (`SandboxConfig` / `StageSandboxConfig`): Resolves as stage > plan > stage-type default. Default by stage type: Knowledge/KnowledgeDistill → `acceptEdits`; Standard/IntegrationVerify → `auto`.

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

## Per-Worktree Gitignore for settings.local.json

After worktree creation, `.claude/settings.local.json` is appended (idempotently) to `<worktree>/.git/info/exclude`. Uses per-worktree exclude to avoid polluting the repo's `.gitignore`.

- Standard/IntegrationVerify/KnowledgeDistill: append to `<worktree>/.git/info/exclude`
- Knowledge stages: append to main repo's `.git/info/exclude` (no worktree created)
- The per-worktree exclude file lives at `<worktree>/.git/info/exclude` — NOT at `<worktree-dir>/.git/info/exclude` (the latter is a FILE pointing at the real gitdir, not a directory; the real exclude is at `<repo>/.git/worktrees/<stage-id>/info/exclude`)

## Hook System Architecture (hooks/)

The `hooks/` module provides Claude Code hooks integration for session lifecycle management. It is a **top-level module** — currently imported by `orchestrator/` and `git/worktree/`, which is a known layering violation (both should import a stable hooks interface instead).

**Layering:** `hooks/` is used by `orchestrator/core/stage_executor.rs` (worktree hook setup) and `git/worktree/settings.rs` (settings injection). The intended fix is to extract hooks as a fully independent top-level module with no reverse imports.

**Global vs session hooks distinction:**

- **Global hooks** (commit-filter.sh, git-add-guard.sh, worktree-isolation.sh, prefer-modern-tools.sh): written once by `loom init` into the main repo's `.claude/settings.local.json` via `fs/permissions.rs`. Persist across all sessions.
- **Session hooks** (session-start.sh, post-tool-use.sh, pre-compact.sh, session-end.sh, learning-validator.sh): generated fresh per-session by `hooks/generator.rs:generate_hooks_settings()`. Merged into worktree's `settings.local.json` with duplicate detection.

## Monitor Subsystem (orchestrator/monitor/)

Full file list:

- `core.rs` — `Monitor` struct, `poll()` API, stage/session loading
- `config.rs` — `MonitorConfig` (work_dir, hung_timeout, etc.)
- `detection.rs` — `Detection` struct: `detect_stage_changes()`, `detect_session_changes()`, `detect_heartbeat_events()`
- `events.rs` — `MonitorEvent` enum (stage/session/heartbeat event variants)
- `failure_tracking.rs` — Consecutive failure escalation logic
- `handlers.rs` — `Handlers` struct: handoff/crash-report generation; holds optional `LivenessService`
- `heartbeat.rs` — `HeartbeatWatcher` with 300s hung timeout
- `context.rs` — Context health thresholds: Green (<50%), Yellow (50-64%), Red (65%+)
- `tests.rs` — Unit tests

**`Monitor::poll()` flow:**

1. Load all stages from `.work/stages/*.md`
2. Load all sessions from `.work/sessions/*.md`
3. `detection.detect_stage_changes()` — file-level changes
4. `detection.detect_session_changes()` — PID liveness, status transitions
5. `detection.detect_heartbeat_events()` — hung detection via `HeartbeatWatcher`
6. Return `Vec<MonitorEvent>`

**LivenessService injection:** `Monitor::set_liveness(liveness: LivenessService)` is called by the orchestrator after `BackendDispatcher` construction. Until set, session-alive checks fall back to legacy host-PID probe (`kill -0`).

## Status Command Architecture (commands/status/)

The status command is organized as a sub-module tree:

```text
commands/status.rs          # Entry: dispatches to 3 modes + validate/doctor
commands/status/
  data.rs                   # collect_status_data() → StatusData struct
  render/                   # Pure render functions (progress, graph, merge, compact)
  ui/                       # TUI backed by daemon IPC subscription
  diagnostics.rs            # Workspace integrity checks
  display.rs                # count_files() helper
  merge_status.rs           # Merge section data
  validation.rs             # Markdown + cross-reference validation
```

**Data flow (static mode):** `collect_status_data()` loads plan name, stage list (with status/context), session list, merge state, and progress counts into a single `StatusData`. Renderers receive `StatusData` and write to `impl Write`.

**TUI mode:** `ui::run_tui(work_path)` subscribes to the daemon's Unix socket (`orchestrator.sock`) and re-renders on each update. Requires daemon running; errors with hint if not.

## Soft Signals

Soft signals are advisory per-session notices persisted to disk so that dedup survives daemon restarts. File: `.work/monitor/soft-signals.jsonl` (JSONL, append-only, no compaction).

**Schema (single variant today):**

```json
{"kind":"possibly_stuck","session_id":"s1","stage_id":"my-stage","recent_events":10,"failure_count":9,"failure_ratio":0.9,"emitted_at":"<RFC3339>","expires_at":"<RFC3339>"}
```

**Decay window:** `DECAY_WINDOW_SECS = 120` — signals expire 120 seconds after they are written. `read_active(work_dir, now)` filters out expired signals. `read_active_for_session(work_dir, now, session_id)` further filters by session.

**Detection pipeline:**

1. `post-tool-use.sh` appends rows to `.work/tool-events.jsonl` on every tool call.
2. `orchestrator/monitor/tool_analysis::analyze_session()` reads the last 50 events for a session and computes `ToolAnalysis`.
3. Stuck criteria: `recent_failure_count >= 5 (STUCK_MIN_EVENTS)` AND `failure_ratio >= 0.80 (STUCK_FAILURE_RATIO)` within a 60-second rolling window (`STUCK_WINDOW_SECS`). Failure-shaped events: `is_error == true` OR `output_bytes == Some(0)`.
4. On detection, monitor emits `MonitorEvent::PossiblyStuck`; the event handler calls `soft_signals::append(work_dir, &signal)`.
5. `daemon/server/status.rs::collect_status()` calls `soft_signals::read_active_for_session()` to derive `Stage.is_possibly_stuck` at read time (never persisted to stage files — `#[serde(skip)]`).
6. Static `loom status` reads via `commands/status/data.rs::collect_status_data()` using the same helper.

**Key files:** `orchestrator/monitor/soft_signals.rs` (schema + I/O), `orchestrator/monitor/tool_analysis.rs` (analysis), `orchestrator/monitor/detection.rs` (event emission), `daemon/server/status.rs` (status derivation).

## Orchestrator Main-Loop Tick Sequence (Exact Call Order)

Main loop at `orchestrator/core/orchestrator.rs:258-376` — 5s poll cycle (100ms chunks for shutdown responsiveness):

```text
1. reconcile_and_update_graph()        [recovery.rs:149-177]  — catch phantom merges pre-sync
2. sync_graph_with_stage_files()       [recovery.rs:179-567]  — disk → in-memory graph
3. sync_queued_status_to_files()       [recovery.rs:569-593]  — graph Queued → disk
4. spawn_merge_resolution_sessions()   [merge_handler.rs:637-758] — detect/spawn merge resolvers
5. *** INSERT: check_pending_disputes() + apply_pending_verdicts() HERE ***
6. start_ready_stages()                [stage_executor.rs:64-86]  — worktrees + sessions for Queued
7. monitor.poll() → handle_events()   [event_handler.rs:308-311]  — completion/crash events
```

Insertion point for adjudicator hooks: after step 4 (merge resolution) and BEFORE step 6 (start_ready_stages) so re-queued stages from verdicts are picked up in the same cycle.

No mpsc channels currently — entirely polling-based. The dispute adjudicator adds the first worker-thread + mpsc pattern. See patterns.md § Worker Thread + mpsc Pattern.

## Stage State Machine — NeedsAdjudication (New Variant)

Current 12-variant enum (`models/stage/types.rs`):

```text
WaitingForDeps → Queued → Executing → Completed/Skipped (terminal)
                  |           |
                  v           +→ Blocked, NeedsHandoff, WaitingForInput,
               Skipped           MergeConflict, CompletedWithFailures,
                                  MergeBlocked, NeedsHumanReview
```

Autonomous adjudication adds `NeedsAdjudication` as a new NON-TERMINAL variant:

```text
Executing → NeedsAdjudication → Queued   (accept verdict re-queues)
                              → NeedsHumanReview  (exhaust budget or disabled API key)
```

Transitions FROM `NeedsAdjudication` (to be added to `transitions.rs`):

- `Queued` — accept/reject verdict processed, stage re-queued
- `NeedsHumanReview` — dispute budget exhausted OR ANTHROPIC_API_KEY not set

## Dispute Directory Structure (New, Stage 2+)

`.work/disputes/<stage_id>/<n>/` — per-dispute directory (numbered from 1):

| File | Authority | Contents |
|------|-----------|----------|
| `request.md` | Agent-writable (via daemon RPC) | id, stage_id, criterion_index, reason, evidence_commit, failure_output, fix_attempts_at_dispute, created_at |
| `verdict.md` | Daemon-only (worker thread writes) | verdict, citations, reasoning, plan_patch, adjudicator_attempt_count, model |
| `applied.marker` | Daemon-only (zero-byte, idempotency) | — |
| `.inflight` | Daemon-only (staleness guard) | timestamp + worker ID; >10min → re-fire |

Request.md is written by the daemon handler on behalf of the agent's RPC call. Trust boundary: same pattern as `loom memory note`.

## Plan Versioning (New, Stage 3+)

`.work/plan_versions/` directory for amendment audit trail:

- `.work/plan_versions/.lock` — file lock (serializes amendments)
- `.work/plan_versions/<n>.md` — snapshot of full plan content after amendment n
- `.work/plan_versions/audit.md` — O_APPEND atomic rows (amendment log)

Plan amendment 6-step atomic flow: acquire lock → compute new content → write snapshot → append audit → atomic rename plan file → release lock. Recovery: on daemon startup scan audit.md, verify plan file matches latest snapshot.

## Plan Immutability Invariant (CURRENTLY ENFORCED; Plan-Amendment Stage Relaxes It)

Plans are loaded ONCE at daemon startup via `build_execution_graph()` → `ExecutionGraph::build()`. No reload mechanism exists. The in-memory `graph: ExecutionGraph` field in `Orchestrator` (orchestrator.rs:87) holds all state. Plan file mutations are ONLY via `try_auto_merge()` (stage file changes, not plan structure). The `plan-amendment` stage deliberately relaxes this invariant — ONLY `acceptance`/`wiring` arrays on a single stage are amendable; DAG topology, dependencies, IDs are never changed.
