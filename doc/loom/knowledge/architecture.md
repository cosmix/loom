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
    terminal/               # NativeBackend — host OS terminal spawning
      native/               # Host OS terminal spawning (11+ emulators)
    monitor/                # Session health, heartbeat, failure tracking
    liveness.rs             # LivenessService — session liveness probe
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

### NativeBackend (orchestrator/terminal/)

Concrete type for spawning Claude Code in terminal windows.

- **NativeBackend** (`orchestrator/terminal/native/`) — spawns Claude Code in a host terminal. Supports 11+ emulators via `TerminalEmulator` enum. PID tracking via wrapper scripts writing to `.work/pids/`.

**LivenessService** (`orchestrator/liveness.rs`) — replaces scattered `kill -0` checks. Delegates to `NativeBackend::is_session_alive()` for session liveness probes.

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
- **Self-update**: minisign signature verification for binaries; `agents.zip`, `skills.zip`, and `CLAUDE.md.template` ARE SHA256-verified against the release checksums asset (self-update refuses to install an asset with no checksum entry). Real gap: the verifier fetches an asset literally named `checksums.txt` but the release workflow publishes `SHA256SUMS.txt` — an asset-name mismatch, not a missing-verification gap (see [concerns.md](concerns.md))
- **Shell escaping**: escape_shell_single_quote(), escape_applescript_string() in emulator.rs
- **permission_mode field** (`SandboxConfig` / `StageSandboxConfig`): Resolves as stage > plan > stage-type default. Default by stage type: ALL four stage types → `auto` (Knowledge, KnowledgeDistill, Standard, IntegrationVerify) — loom stages run autonomously with no human to answer prompts, so the agent auto-accepts actions its heuristics deem safe; the sandbox deny/allow rules are the safety boundary. Override to a stricter mode (`accept-edits`, `plan`) at plan or stage level if needed.

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
2. **pre-compact.sh** -- Two-phase block-then-allow pattern. Phase 1 blocks compaction (exit 2), creates handoff. Phase 2 allows compaction (exits 0). No longer creates a recovery marker file.
3. **session-end.sh** -- Uses glob `*-${LOOM_STAGE_ID}.md` for stage file lookup (handles depth prefixes)
4. **Signals** -- cache.rs append_common_footer() adds compaction recovery instructions to ALL signal types
5. **session-start.sh** -- On SessionStart with `.source == "compact"` or `"resume"`, emits hookSpecificOutput additionalContext re-anchor pointer so the agent finds its signal file after compaction

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

## Claude Code Worktree Isolation Disabled in Generated Settings

Loom owns the per-stage git worktree, so it disables Claude Code's *own* worktree
isolation (`worktree.bgIsolation`) in every settings file it generates. Claude
Code's default (`"worktree"`) blocks Edit/Write in the checkout until
`EnterWorktree`, which would push subagents into nested worktrees on top of loom's
— leaving stray branches and tangled checkouts. Loom emits `"none"` so subagents
edit the loom worktree directly (Claude Code v2.1.143+; older versions ignore it).

Two write sites, both targeting `settings.local.json` (never the committed
`settings.json`, to avoid imposing on non-loom teammates):

- **Worktree stage sessions** — `sandbox/settings.rs:generate_settings_json()`
  emits a top-level `"worktree": { "bgIsolation": "none" }` block. Survives the
  `merge_existing_permissions()` step, which only touches `permissions.*`.
- **Main-repo sessions** (knowledge stages, interactive) —
  `fs/permissions/settings.rs:ensure_loom_hooks_local()` sets it idempotently
  alongside the agent-teams env var.

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

**LivenessService injection:** `Monitor::set_liveness(liveness: LivenessService)` is called by the orchestrator after `NativeBackend` construction. Until set, session-alive checks fall back to legacy host-PID probe (`kill -0`).

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

## Remote Control Module (loom/src/remote_control.rs)

Claude Code's `--remote-control` flag lets the loom orchestrator drive Claude sessions programmatically. It exits non-zero when prerequisites are unmet, so it must be gated by a preflight check before use.

**Key types:**

- `RemoteControlMode` (`auto` | `off`) — operator-facing switch persisted in `.work/config.toml [remote_control]`.
- `RemoteControlConfig` — the persisted config struct (single `mode` field).
- `RemoteControlStatus` (`Enabled` | `Disabled { reason }`) — preflight result.

**Key functions:**

| Function | Purpose |
|----------|---------|
| `preflight(claude_path)` | Combines version probe + auth-eligibility heuristic |
| `claude_supports_remote_control(path)` | Version gate only (>= 2.1.51) |
| `remote_control_eligible()` | Auth heuristic: no disqualifying env var + `~/.claude/.credentials.json` present |
| `resolve(work_dir)` | Per-spawn gate: checks mode, marker, and memoized preflight |
| `run_startup_preflight(path, work_dir)` | Advisory startup warning if disabled |
| `write_unsupported_marker(work_dir)` | Writes `.work/remote_control-unsupported` |

**Resolution model (in order):**

1. `mode == off` → false (skip)
2. `.work/remote_control-unsupported` marker exists → false
3. Memoized `preflight()` (runs `claude --version` once per process) → true/false

**Fallback / fast-fail path (crash_handler.rs):**

If a native session crashes within 15 seconds of creation while `resolve()` is true, the crash handler writes `.work/remote_control-unsupported` and logs a warning. The existing retry/backoff then respawns the session; on the retry, `resolve()` returns false (marker present) so `--remote-control` is omitted.

**Config persistence:**

`fs/work_dir.rs` exposes `read_remote_control_config()` / `write_remote_control_config()` using the `[remote_control]` section of `.work/config.toml`. Pattern mirrors `read_plan_sandbox` / `write_plan_sandbox`.

**Auth disqualifying env vars (Remote Control requires claude.ai login):**

`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, `CLAUDE_CODE_USE_FOUNDRY`

## Signal Generation Pipeline (orchestrator/signals/) [DETAILED]

The signal system assembles agent prompt files in a **4-section Manus KV-cache pattern** for token efficiency.

### Call Hierarchy

```text
generate_signal_with_skills() [generate.rs]
  └─ build_signal_context()           # assembles EmbeddedContext
       └─ build_embedded_context_with_stage_and_session()
            ├─ reads handoff (V1 prose / V2 structured)
            ├─ read_plan_overview()
            ├─ KnowledgeDir::has_content()
            └─ format_memory_for_signal(last 10 entries only)
  └─ format_signal_content() [format/mod.rs]
       └─ format_signal_with_metrics()
            ├─ select stable prefix from cache.rs (by stage type)
            ├─ format_semi_stable_section() [sections.rs:15]
            ├─ format_dynamic_section() [sections.rs:382]
            ├─ format_recitation_section() [sections.rs:665]
            └─ SignalMetrics::from_sections() → SHA-256 hash first 16 hex chars
```

Knowledge stages use a SEPARATE path: `generate_knowledge_signal()` [knowledge.rs:23].

### Four Stable-Prefix Generators (cache.rs)

All generators are composed from shared `append_*` helpers and produce immutable KV-cached text.

| Generator | Function | Line | Stage Type |
|-----------|----------|------|------------|
| Standard | `generate_stable_prefix()` | 174 | `StageType::Standard` |
| Integration-Verify | `generate_integration_verify_stable_prefix()` | 313 | `StageType::IntegrationVerify` |
| Knowledge-Distill | `generate_knowledge_distill_stable_prefix()` | 447 | `StageType::KnowledgeDistill` |
| Knowledge | `generate_knowledge_stable_prefix()` | 527 | `StageType::Knowledge` |

**Standard prefix section order (approx lines 174-310):**

1. Worktree Context header
2. Isolation Boundaries (3 bullets)
3. `append_path_boundaries()` — ALLOWED/FORBIDDEN paths table
4. working_dir reminder
5. Execution Rules header
6. Worktree Isolation detail
7. Delegation & Efficiency (subagents + hierarchies + agent teams)
8. `append_subagent_restrictions()` — NO commit/complete/add-A rules
9. `append_completion_rules()`
10. `append_adversarial_review()` — Mini Adversarial Code Review (6 dimensions)
11. Dedicated Silent Failure Check block (Standard only; IV has its own section)
12. Stage Memory guidance
13. `append_git_staging_full()` (Standard ONLY; IV/KnowledgeDistill use `append_git_staging_rules()`)
14. `append_common_footer()`

**Integration-Verify key differences:** ZERO TOLERANCE box at top; no full git-staging box; now requires agent teams (MUST).

**Knowledge-Distill:** Mission = curate memories → knowledge; includes documentation update reminder.

**Knowledge prefix key differences:** No worktree; COMMITS REQUIRED; "Your Mission = build briefing document"; 6-step workflow; agent teams for bootstrap.

### Shared append_* Helpers (cache.rs:51-169)

| Helper | Lines | Content | Used By |
|--------|-------|---------|---------|
| `append_path_boundaries()` | 54-63 | ALLOWED/FORBIDDEN paths table | Standard, IV, KnowledgeDistill |
| `append_subagent_restrictions()` | 66-93 | NO git/loom/add-A rules; memory recording guide | Standard (233), IV (424) |
| `append_completion_rules()` | 96-102 | Acceptance, handoff, no retry rules | Standard (254), IV (433), KnowledgeDistill (515) |
| `append_isolation_boundaries_simple()` | 108-113 | 2-bullet version | IV (408), KnowledgeDistill (508) |
| `append_execution_rules_intro()` | 119-124 | "Follow CLAUDE.md" short header | IV (412), KnowledgeDistill (512), Knowledge (594) |
| `append_common_footer()` | 127-142 | Binary usage, state files, context recovery | ALL 4 prefixes |
| `append_git_staging_full()` | 145-160 | Full staging rules + danger box | Standard only |
| `append_git_staging_rules()` | 162-169 | Shorter version | IV, KnowledgeDistill |

**Adding a new helper:** Follow same `fn append_xxx(content: &mut String)` pattern. Place in the "Shared content blocks" cluster (lines 51-169). Call it explicitly from each generator where wanted — it's NOT auto-injected.

### Semi-Stable Section (format/sections.rs:15-378)

Changes per **stage type**, not per session. Key sub-sections:

- **Knowledge reference box** (lines 22-32): `loom knowledge show` commands if knowledge exists
- **Stage-type-aware reminder box** (lines 35-140): Knowledge/IV/KnowledgeDistill → "KNOWLEDGE UPDATES REQUIRED"; Standard → "SESSION MEMORY REQUIRED"
- **Knowledge management section** (lines 142-290): If knowledge empty → 4-step exploration order; if present → "Extend as you work"
- **Delegation Choices** (lines 319-345): Subagents vs. Hierarchy vs. Agent Teams decision
- **Ultracode License** (lines 347-362): Gated on `embedded_context.ultracode`
- **Sandbox Restrictions** (lines 365-368): Sandbox summary if present
- **Skill Recommendations** (lines 370-374): Skill index matches

### Dynamic Section (format/sections.rs:382-661)

Per-session content. Includes Target (session/stage/plan IDs, working_dir, execution path), Plan Overview, Assignment, Dependency Status + Outputs, Handoff Content, Acceptance Criteria, Goal-Backward Verification (artifacts, wiring, wiring_tests, dead_code).

### Recitation Section (format/sections.rs:665-765)

End of signal for maximum attention. Includes: Compaction Imminent warning (≥75% usage), Context Budget Warning, Immediate Tasks, Stage Memory (with PROMINENT WARNING if empty).

### EmbeddedContext Struct (types.rs:24-50)

Single container flowing through all 4 sections:

```rust
pub struct EmbeddedContext {
    pub handoff_content: Option<String>,      // V1 prose handoff
    pub parsed_handoff: Option<HandoffV2>,    // V2 structured handoff
    pub plan_overview: Option<String>,
    pub knowledge_has_content: bool,
    pub memory_content: Option<String>,       // Last 10 entries
    pub skill_recommendations: Vec<SkillMatch>,
    pub context_budget: Option<f32>,
    pub context_usage: Option<f32>,
    pub sandbox_summary: Option<SandboxSummary>,
    pub cross_stage_summary: Option<String>,  // IV/KnowledgeDistill only
    pub wiring_checklist: Option<String>,     // IV/KnowledgeDistill only
    pub ultracode: bool,
}
```

### Caching

SHA-256 of stable prefix text → first 16 hex chars → `SignalMetrics::stable_prefix_hash`. Cache invalidated whenever the stable prefix Rust code changes. Semi-stable, dynamic, recitation sections are always regenerated.

## before_stage / after_stage / code_review Schema Fields — Execution Status

**Status as of 2026-06-15 (verified against stage_executor.rs:219-256, plan/schema/types.rs:261, and orchestrator/signals/generate.rs):**

| Field | Schema Type | Stored on Stage | Executed | Where |
|-------|-------------|-----------------|----------|-------|
| `before_stage` | `Vec<TruthCheck>` | ✅ Yes (plan_setup.rs:280) | ✅ Yes | stage_executor.rs:220-256 (pre-spawn) |
| `after_stage` | `Vec<TruthCheck>` | ✅ Yes (plan_setup.rs:281) | ✅ Yes | commands/stage/complete.rs:847-866 |
| `code_review` | `Option<CodeReviewConfig>` | ❌ NOT on Stage struct | ✅ Partial | signals/generate.rs reads from plan for IV signal |

**`before_stage` execution (stage_executor.rs:219-256):**

- Runs after worktree creation, BEFORE session spawn
- Calls `crate::verify::before_after::run_before_stage_checks(&stage.before_stage, &check_dir)`
- On failure gaps: stage → `Blocked` (FailureType::TestFailure), session NOT spawned
- On errors (infrastructure): prints warning, continues anyway (advisory)
- TruthCheck timeout: 30 seconds (hardcoded in truths.rs:13)

**`after_stage` execution (commands/stage/complete.rs:847-866):**

- Runs during `loom stage complete`, AFTER acceptance criteria pass
- On failure: stage stays Executing, no merge, agent must fix and re-run

**`code_review` — WIRED FOR SIGNAL GENERATION ONLY (as of PLAN-anti-slop-thoroughness):**

- Parsed by serde at schema level (`plan/schema/types.rs:261`)
- NOT copied in `create_stage_from_definition()` — Stage struct has NO `code_review` field
- `load_code_review_for_stage(stage_id, plan_path)` in `orchestrator/signals/generate.rs` reads it directly from the plan file via `parse_plan()` — used ONLY for IntegrationVerify signal generation
- `render_review_dimensions()` emits a `## Review Dimensions` checkbox section in IV signals, honoring `require_all`
- Still NOT consumed during acceptance, completion, or goal-backward verification
- `plan/schema/mod.rs` re-exports `CodeReviewConfig` for use in generate.rs

## Hook System — Session-Start Behavior and hookSpecificOutput Pattern

### session-start.sh Behavior (Updated 2026-06-15)

- Captures stdin into a variable (not drained) using cross-platform gtimeout/timeout/cat, 1s timeout
- Validates LOOM_STAGE_ID, LOOM_SESSION_ID, LOOM_WORK_DIR — silently exits if missing
- Writes initial heartbeat: `.work/heartbeat/<LOOM_STAGE_ID>.json`
- Logs SessionStart event to `.work/hooks/events.jsonl`
- **Parses `.source` field from stdin JSON**: when `.source == "compact"` or `"resume"`, emits `hookSpecificOutput.additionalContext` JSON with a re-anchor pointer (signal file path), redirecting the agent back to its signal after context compaction or resume
- Stdin must be captured (not drained with `>/dev/null`) so the source field can be parsed — same pattern as `post-tool-use.sh`

**Compaction recovery flow (current):**

```text
pre-compact.sh phase 1 → blocks compaction + creates handoff
pre-compact.sh phase 2 → allows compaction
Claude Code emits SessionStart with source="compact"
session-start.sh → parses source → emits hookSpecificOutput additionalContext re-anchor
```

### hookSpecificOutput JSON Pattern

Used by hooks to inject context into Claude's next turn:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "<EventType>",
    "additionalContext": "<string content>"
  }
}
```

**Examples:**

- `prefer-modern-tools.sh` (lines 100-101): PreToolUse warning about grep usage
- `skill-trigger.sh` (lines 286-291): UserPromptSubmit skill suggestions
- `session-start.sh`: SessionStart re-anchor pointer on compact/resume source

**Why JSON over plain text:** Claude Code has reliability issues with plain-text stdout from certain hook types (see issue claude-code#13912); JSON additionalContext is more reliable for context injection.

**Construction:** Always use `jq -nc --arg ctx "..."  '{hookSpecificOutput: {hookEventName: "...", additionalContext: $ctx}}'` — never manually escape JSON strings.

### LOOM_* Env Vars Available to All Hooks

Set by wrapper script (pid_tracking.rs:463-479) before `exec claude`:

| Variable | Purpose |
|----------|---------|
| `LOOM_SESSION_ID` | Current session ID |
| `LOOM_STAGE_ID` | Current stage ID |
| `LOOM_WORK_DIR` | Absolute path to `.work/` |
| `LOOM_MAIN_AGENT_PID` | Process PID (set dynamically, NOT in settings.json) |
| `LOOM_WORKTREE_PATH` | Absolute worktree path (worktree sessions only) |
| `LOOM_MERGE_SESSION=1` | Set for merge resolution sessions only |

**LOOM_MAIN_AGENT_PID gotcha:** Must NOT be in settings.json env block (generator.rs explicitly removes it). Must be set by wrapper script as `export LOOM_MAIN_AGENT_PID=$$` so it reflects the actual Claude process PID. Stale value → commit-filter.sh misidentifies main agent as subagent.

### Hook Embedding (constants.rs)

All 15 hooks embedded via `include_str!()` at compile time. `install_loom_hooks()` writes them to `~/.claude/hooks/loom/` with mode 0o755. Hooks are NOT read from disk by loom at runtime.

## Shared append_* Helpers (cache.rs:51-169)

### Shared append_* Helpers (cache.rs:51-~180)

| Helper | Lines | Content | Used By |
|--------|-------|---------|---------|
| `append_path_boundaries()` | 54-63 | ALLOWED/FORBIDDEN paths table | Standard, IV, KnowledgeDistill |
| `append_subagent_restrictions()` | 66-93 | NO git/loom/add-A rules; memory recording guide | Standard (233), IV (424) |
| `append_completion_rules()` | 96-102 | Acceptance, handoff, no retry rules | Standard (254), IV (433), KnowledgeDistill (515) |
| `append_isolation_boundaries_simple()` | 108-113 | 2-bullet version | IV (408), KnowledgeDistill (508) |
| `append_execution_rules_intro()` | 119-124 | "Follow CLAUDE.md" short header | IV (412), KnowledgeDistill (512), Knowledge (594) |
| `append_common_footer()` | 127-142 | Binary usage, state files, context recovery | ALL 4 prefixes |
| `append_git_staging_full()` | 145-160 | Full staging rules + danger box | Standard only |
| `append_git_staging_rules()` | 162-169 | Shorter version | IV, KnowledgeDistill |
| `append_anti_slop_guidance()` | ~171+ | ZERO TOLERANCE anti-slop rules box | ALL 4 prefixes (after exec-rules intro, before Delegation) |
| `append_adversarial_review()` | ~104-122 | Mini adversarial code review — 6 dimensions (quality/architecture·SOLID, idiomatic, security, wiring, dead code, DRY across whole codebase) + a closing "tests actually exercise the change" check | Standard (replaces old "Self-Review" block), IV (after Mission). **Code-producing prefixes ONLY** — NOT knowledge or knowledge-distill (both emit only markdown). NOTE: silent-failure detection is NOT in this helper — Standard has its own dedicated block right after the call; IV has its own `SILENT FAILURE DETECTION` section |

**Adding a new helper:** Follow same `fn append_xxx(content: &mut String)` pattern. Place in the "Shared content blocks" cluster (lines 51-~180). Call it explicitly from each generator where wanted — it's NOT auto-injected.

**Per-stage code review:** The mandatory mini adversarial code review lives in `append_adversarial_review()` (`pub(crate)`) and is injected into the two code-producing stable prefixes (Standard, IntegrationVerify). It supersedes the older standard-prefix "Self-Review Before Completion" block. Documentation stages (Knowledge, KnowledgeDistill) deliberately omit it — they produce only markdown, so there is no code to review; the cache tests negative-assert its absence there.

**Stable prefix selection — single source of truth:** `cache::stable_prefix_for(stage_type)` is the ONE place that maps stage type → prefix generator (explicit 4-arm match). Both the regular path (`format/mod.rs::format_signal_with_metrics`) and the recovery path (`recovery_format.rs`) call it, so they can never drift.

**Resume-path coverage (important):** The review (and all execution guidance) must reach a stage no matter which signal spawns it. Three paths: (1) regular spawn + automatic crash retry → `format_signal_with_metrics()` → `stable_prefix_for()`; (2) continuation/handoff → `generate_signal()` → same path; (3) **manual recovery** (`loom stage recover`, `loom stage retry`) → `recovery_format.rs::format_recovery_signal()`. The recovery signal is built outside the KV-cache path; it now embeds the FULL stable prefix via `stable_prefix_for(stage.stage_type)` (replacing its old hand-rolled "## Worktree Context" stub), so a resumed stage gets the same rules — review, subagent restrictions, git-staging, anti-slop, completion — as a fresh spawn, correctly gated by stage type (Knowledge/KnowledgeDistill prefixes carry no review). Tests: `recovery.rs::test_generate_recovery_signal` (Standard → review + subagent restrictions + execution rules present) and `test_recovery_signal_omits_review_for_documentation_stage` (KnowledgeDistill → no review).

## load_stage_definition_from_plan — Centralized Plan Lookup

Centralized in `plan/parser/mod.rs` (re-exported via `plan/mod.rs`). Previously lived in `commands/verify.rs`.

**Signature:** `load_stage_definition_from_plan(work_dir, stage_id) -> Result<StageDefinition>`

Reads `.work/config.toml` for plan path, calls `resolve_source_path()`, calls `parse_plan()`, finds stage by ID. Used by:

- `commands/stage/complete.rs` — after_stage execution
- `commands/stage/verify.rs` — goal-backward verification
- `orchestrator/signals/generate.rs` — code_review lookup for IV signal generation

**Why plan/ layer:** both commands/ and orchestrator/ already depend on plan/; moving here eliminated a code_review re-inline in generate.rs without adding any new dependency edge. orchestrator/ -> commands/ would have been a layering violation.

## `loom pressure` Command (Plan Pressure-Testing Driver)

`loom pressure <plan> [--rounds N=2] [--dry-run]` (loom/src/commands/pressure/mod.rs) is a standalone, **synchronous foreground** driver that hardens a plan by combining two external agents. It is a second execution model distinct from the daemon/worktree orchestrator: it runs in the user's repo — NOT a worktree, NOT a background daemon, NOT a terminal-spawn.

Per round (default 2): delete the codex report → run **Claude `/pressure` (foreground) and Codex `$pressure` (background) CONCURRENTLY** → once both finish, run Claude `/address <plan>` (folds Codex's written review back into the plan). One final report deletion after all rounds. Because the two pressure-tests run in parallel, Codex reviews the *pre-edit* plan while Claude edits it — a more independent perspective; `/address` reconciles both afterward.

**Billing/TTY constraint (load-bearing):** Claude Code enters its non-interactive `-p` path — which can bill against pay-per-token API credits instead of the claude.ai subscription — whenever **stdout is not a TTY** (piped/redirected), even without `-p` (confirmed in `claude --help`). So Claude's stdout MUST stay the real terminal: `/pressure` and `/address` run in the **foreground** (interactive, subscription-billed, visible), and CANNOT be captured/backgrounded. Codex — which has separate auth and floods stdout with a verbose event stream — is the one backgrounded, with stdout+stderr captured to a temp log (`$TMPDIR/loom-pressure-codex-<pid>.log`); its tail is printed on non-clean exit.

**Auto-exit without `-p` (mirrors the daemon):** interactive Claude never exits on its own after a slash command, and EOF on stdin makes the REPL quit *before* the work finishes. So the driver replicates how the daemon ends a session (`event_handler.rs` → `NativeBackend::kill_session` → SIGTERM once the stage completes): it injects a completion instruction via `--append-system-prompt` telling the agent to `touch <marker>` as its FINAL action, polls for that marker file, then SIGTERMs (escalating to SIGKILL after a grace period) the now-idle foreground session. If the marker never appears the user can still exit manually (graceful fallback = old behavior). Codex is non-interactive and exits on its own.

Children run with `current_dir(repo_root)` (resolved via `git rev-parse --show-toplevel`), so the plan argument handed to them is **repo-relative** (e.g. `doc/plans/PLAN-foo.md`), never cwd-relative. Claude argv: `--permission-mode auto --model opus --append-system-prompt <marker-instruction> <slash>` with `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` and stdin/stdout/stderr inherited. Codex argv: `exec --sandbox workspace-write -C <repo_root> <skill>` with stdin `/dev/null` and stdout/stderr → the log file. NOTE: Codex has been observed printing a non-fatal `worker transport error / authorization required` warning at startup even while logged in and continuing to work — it is codex-side, not a loom bug; the captured log now keeps it off the terminal.

Supporting pieces:

- `loom/src/codex.rs` — `find_codex_path()` binary resolver, mirrors `claude::find_claude_path` (which::which, then candidate install paths favoring ~/.bun/bin; spawned children may not inherit PATH so resolve eagerly).
- Vendored agent assets (installed LOCALLY by install.sh): `commands/{pressure,address,distill}.md` → `~/.claude/commands/`; `codex/skills/pressure/SKILL.md` → `~/.codex/skills/pressure/`.
- Wiring: `Commands::Pressure` in cli/types.rs:195, dispatched in cli/dispatch.rs:178; `pressure` registered in dynamic completions with `--rounds`/`--dry-run`.
