# Architecture

> High-level component relationships, data flow, and module dependencies.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [patterns.md](patterns.md) for design patterns, [entry-points.md](entry-points.md) for code navigation, [conventions.md](conventions.md) for coding standards.

## Project Overview

Loom is a Rust CLI (~15K lines) for orchestrating parallel Claude Code sessions across git worktrees. It enables concurrent task execution with automatic crash recovery, context handoffs, and progressive merging.

## Directory Structure

```text
loom/src/
  main.rs, lib.rs          # CLI entry (clap), module exports
  commands/                 # CLI implementations (~4K lines)
    init/, run/, stage/, status/, merge/, memory/, knowledge/, track/, runner/
  daemon/server/            # Background daemon (~1.5K lines)
    core.rs, lifecycle.rs, protocol.rs, status.rs, client.rs, orchestrator.rs
  orchestrator/             # Core engine (~4K lines)
    core/                   # Main loop, stage executor, persistence, recovery
    terminal/               # TerminalBackend trait + native OS spawning
    monitor/                # Session health, heartbeat, failure tracking
    signals/                # Signal generation (Manus format, cache, CRUD)
    continuation/           # Context handoff management
    progressive_merge/      # Merge orchestration + lock
    auto_merge.rs
  models/                   # Domain models (~1K lines)
    stage/ (types, transitions, methods)
    session/ (types, methods)
  plan/                     # Plan parsing (~1.5K lines)
    parser.rs, schema/ (types, validation), graph/ (DAG builder)
  fs/                       # File operations (~500 lines)
    work_dir.rs, knowledge.rs, memory.rs
  git/                      # Git operations (~800 lines)
    worktree/ (base, operations), merge/, branch/
  verify/                   # Acceptance + goal-backward verification (~600 lines)
    criteria/, transitions/, goal_backward/
  sandbox/                  # Claude Code sandbox config generation
    config.rs, settings.rs
  hooks/                    # Hook script definitions
  parser/frontmatter.rs     # Canonical YAML frontmatter extraction
  validation.rs             # Input validation (IDs, names)
  completions/              # Shell completion (static + dynamic)
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
                  Skipped              MergeConflict, CompletedWithFailures, MergeBlocked
```

11 variants total. Terminal states: Completed, Skipped. Transitions validated in transitions.rs. See [patterns.md -- State Machine Pattern](patterns.md#state-machine-pattern) for full diagram.

### StageType Enum (plan/schema/types.rs)

- **Standard** (default) -- Regular implementation stages, require goal-backward verification
- **Knowledge** -- No worktree, no commits, exploration only, auto merged=true
- **IntegrationVerify** -- Final verification, exempt from goal-backward checks
- **CodeReview** -- Security/quality review, exempt from goal-backward checks

### Session Lifecycle (models/session/)

States: Spawning -> Running -> Completed | Crashed | ContextExhausted | Paused. Tracks PID, terminal window ID, context usage %, timestamps.

### TerminalBackend (orchestrator/terminal/)

Trait for spawning Claude Code in terminal windows. NativeBackend supports 11+ emulators (kitty, alacritty, gnome-terminal, etc.) via `TerminalEmulator` enum. PID tracking via wrapper scripts that write to `.work/pids/`.

## Data Flow

### Plan Execution Flow

```text
1. loom init doc/plans/PLAN-foo.md
   --> Parse plan, create .work/, write stage files

2. loom run
   --> Spawn daemon (or foreground) --> orchestrator loop

3. Orchestrator loop (5s poll):
   Load stage files --> Build ExecutionGraph --> Find ready stages
   --> Create worktree + signal --> Spawn terminal --> Monitor sessions

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

3. **Signal layer** -- Four stage-type-specific stable prefix generators in cache.rs (standard, knowledge, code-review, integration-verify). Include isolation rules and subagent restrictions.

4. **Hook layer** -- commit-guard.sh blocks exit without commit. commit-filter.sh blocks subagent git operations via LOOM_MAIN_AGENT_PID/PPID comparison. See [patterns.md -- Hook Patterns](patterns.md#hook-patterns).

## Subagent Isolation

Three-layer defense: documentation (CLAUDE.md Rule 5), signal injection (cache.rs prefix), hook enforcement (commit-filter.sh). Detection: wrapper script exports LOOM_MAIN_AGENT_PID; hook compares PPID to detect subagent context.

## Layering Violations (Known Issues)

Correct dependency direction: commands/ -> orchestrator/ -> models/ (top), daemon/ / git/ / plan/ (middle), fs/ (bottom). Lower layers must not import higher.

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

Stages define context_budget (1-100%, default 65%, max 75%). Monitor tracks Green (<50%), Yellow (50-64%), Red (65%+). BudgetExceeded event triggers auto-handoff. See [patterns.md -- Context Health Pattern](patterns.md#context-health-pattern).

## Security Model

- **ID validation**: Alphanumeric + dash/underscore, max 128 chars, no path traversal (validation.rs)
- **Acceptance criteria**: Runs arbitrary shell commands (trusted model)
- **Socket**: Mode 0o600 (owner only), max 100 connections, 10MB message limit, Unix only
- **Self-update**: minisign signature verification. Gap: non-binary release assets lack verification
- **Shell escaping**: escape_shell_single_quote(), escape_applescript_string() in emulator.rs

## Merge Lock (progressive_merge/lock.rs)

MergeLock prevents concurrent merges via exclusive file at `.work/merge.lock`. Atomic creation, PID + timestamp. Timeout 30s, stale lock auto-cleanup at 5min. Released via Drop.

## Skills Module (loom/src/skills/)

Provides automated skill recommendation for agent signals. Loads skill metadata from SKILL.md files in ~/.claude/skills/, builds inverted index of trigger keywords, and matches stage descriptions against triggers.

Components:

- types.rs: SkillMetadata (name, description, triggers), SkillMatch (name, description, score, matched_triggers)
- matcher.rs: Keyword matching algorithm. Normalizes text (lowercase, underscore/hyphen → space). Phrase matches = 2 points, word matches = 1 point. Returns top N results above configurable threshold.
- index.rs: SkillIndex - main API. load_from_directory() reads ~/.claude/skills/*/SKILL.md files, parses YAML frontmatter, builds trigger_map HashMap. match_skills() enforces score threshold of 2.0.

Integration: Exported via lib.rs (pub mod skills). Used by signal generation (generate_signal_with_skills in orchestrator/signals/generate.rs) to embed up to 5 skill recommendations in agent signals.

## Diagnosis Module (loom/src/diagnosis/)

Analyzes failed/blocked stages and generates diagnostic signals for investigation.

Components:

- signal.rs: DiagnosisContext struct (stage, crash_report, log_tail, git_status, git_diff). generate_diagnosis_signal() creates .work/signals/{session-id}.md with failure evidence. load_crash_report() reads crash reports for a stage.

Philosophy: loom collects evidence (crash reports, git state, logs), Claude Code performs analysis. Non-destructive investigation before recovery/reset.

CLI: loom diagnose <stage-id> → commands/diagnose.rs

## Map Module (loom/src/map/)

Automated codebase analysis that populates knowledge files.

Components:

- analyzer.rs: Orchestrates all detectors, returns AnalysisResult (architecture, stack, conventions, concerns)
- detectors.rs: detect_project_type() (Rust/Node/Go/Python/Ruby), analyze_dependencies() (parses Cargo.toml/package.json), find_entry_points() (main.rs/index.ts/main.py), analyze_structure() (directory tree depth 2-3), detect_conventions() (formatters, linters, tsconfig), find_concerns() (TODO/FIXME counts, .env/.secrets)

Features: --deep (3-level depth + concern scanning), --focus <area> (filter entry points), --overwrite (replace vs append). Skips .git, .work, .worktrees, node_modules, target, .venv, **pycache**.

CLI: loom map [--deep] [--focus <area>] [--overwrite] → commands/map.rs → writes to doc/loom/knowledge/ via KnowledgeDir

## StageType Enum Update (2026-02-07)

StageType now has 3 variants (CodeReview was removed and consolidated into IntegrationVerify):

- **Standard** (default) -- Regular implementation stages, require goal-backward verification
- **Knowledge** -- No worktree, no commits, exploration only, auto merged=true
- **IntegrationVerify** -- Final quality gate combining code review AND functional verification

Signal generation has 3 prefix generators in cache.rs (standard, knowledge, integration-verify). The integration-verify prefix includes code review guidance (security-engineer, senior-software-engineer review agents).

## Handoff System Issues (2026-02-07)

Three critical bugs in the handoff chain cause complete handoff failure:

1. **Missing CLI command**: `loom handoff create` does not exist. Both hooks/pre-compact.sh:60 and hooks/session-end.sh:64 call it, fail silently, no handoff is ever created.

2. **pre-compact.sh allows compaction on failure**: Even when handoff creation fails, exits 0. Agent's context is destroyed without any record.

3. **session-end.sh stage lookup broken**: Line 54 uses exact path `stages/${LOOM_STAGE_ID}.md` but stage files have depth prefixes (e.g., `01-stage-id.md`). The status check always fails.

### Handoff Generation API

`generate_handoff(_session, stage, content, work_dir)` in handoff/generator/mod.rs:

- `_session: &Session` — UNUSED (underscore prefix), accepts minimal Session
- `stage: &Stage` — needs stage.id and stage.description
- `content: HandoffContent` — builder pattern with with_*() methods
- `work_dir: &Path` — path to `.work/` directory
- Returns: `Result<PathBuf>` — path to written handoff file

### Signal System - No Recovery Text

3 stable prefix generators in cache.rs (standard, knowledge, integration-verify). None contain compaction recovery instructions. Budget warning in format_recitation_section() triggers at 80%+ but only shows promote/complete instructions.

## StageDefinition → Stage Field Propagation (commands/init/plan_setup.rs:190-253)

`create_stage_from_definition(stage_def, plan_id) -> Stage` copies ALL verification fields:

Direct copies: id, name, description, dependencies, parallel_group, acceptance, setup, files, auto_merge, context_budget, truths, artifacts, wiring, truth_checks, wiring_tests, dead_code_check, sandbox, execution_mode.

Special handling: working_dir wrapped in Some(), stage_type via detect_stage_type(), plan_id from parameter.

Stage-only fields (not from StageDefinition): status, worktree, session, held, retry_count, merged, merge_conflict, verification_status, timestamps, etc.

### Adding New Fields Checklist

To add a new field from plan YAML to stage:

1. Add to StageDefinition (plan/schema/types.rs) with serde defaults
2. Add validation in validation.rs validate()
3. Add to Stage model (models/stage/types.rs) with serde defaults
4. Copy in create_stage_from_definition() (commands/init/plan_setup.rs)
5. If goal-check: update has_any_goal_checks() in BOTH StageDefinition and Stage methods
6. If verification: add verify function in verify/goal_backward/ and call from run_goal_backward_verification()

### StageDefinition Verification Fields (plan/schema/types.rs:212-267)

- truths: Vec<String> — simple shell commands
- artifacts: Vec<String> — file glob patterns
- wiring: Vec<WiringCheck> — source + regex pattern + description
- truth_checks: Vec<TruthCheck> — enhanced with stdout_contains, exit_code, stderr_empty
- wiring_tests: Vec<WiringTest> — named command tests with SuccessCriteria
- dead_code_check: Option<DeadCodeCheck> — command + fail_patterns + ignore_patterns

### TruthCheck Struct (plan/schema/types.rs:292-310)

Fields: command (String), stdout_contains (Vec<String>), stdout_not_contains (Vec<String>), stderr_empty (Option<bool>), exit_code (Option<i32>), description (Option<String>). All optional fields use serde(default, skip_serializing_if).

## Handoff System (Updated 2026-03-05)

The handoff system is FULLY FUNCTIONAL. Previously documented bugs have been fixed:

1. **loom handoff create EXISTS**: CLI at cli/types.rs:298-317, handler at commands/handoff/create.rs:14-120, dispatched at cli/dispatch.rs:62. Accepts --stage, --session, --trigger, --message flags.

2. **pre-compact.sh uses two-phase block-then-allow**: Phase 1 blocks compaction (exit 2), creates handoff, agent dumps context. Phase 2 allows compaction (exit 0), creates recovery marker at .work/compaction-recovery/{SESSION_ID}.

3. **session-end.sh handles depth prefixes**: Line 54 uses glob `*-${LOOM_STAGE_ID}.md` with fallback to exact match.

4. **Signals include recovery text**: cache.rs:96-101 append_common_footer() adds compaction recovery instructions to ALL signal types.

5. **Post-tool-use.sh detects compaction recovery**: Checks .work/compaction-recovery/{SESSION_ID} marker, prints recovery instructions, removes marker.

### Stage State Count Correction

StageStatus has 12 variants (not 11): WaitingForDeps, Queued, Executing, Completed, Blocked, NeedsHandoff, WaitingForInput, Skipped, MergeConflict, CompletedWithFailures, MergeBlocked, NeedsHumanReview. Terminal: Completed, Skipped.

### macOS Terminal Detection Priority

1. LOOM_TERMINAL env var (explicit override)
2. TERMINAL env var (user preference)
3. Parent process detection (walks process tree up to 10 levels via ps)
4. Cross-platform binary check (ghostty, kitty, alacritty, wezterm via which)
5. macOS native apps (/Applications/Ghostty.app, /Applications/iTerm.app, Terminal.app fallback)

Note: $TERM_PROGRAM is NOT checked. Ghostty detection only checks /Applications path.

## find_claude_path() Function (orchestrator/terminal/native/mod.rs)

Binary resolution strategy:

1. `which::which("claude")` — PATH lookup
2. `~/.claude/local/claude` — Official Claude Code install (priority fallback)
3. `~/.local/bin/claude` — Linux user-local
4. `~/.cargo/bin/claude` — Cargo installations
5. `/usr/local/bin/claude` — Standard UNIX
6. `/opt/homebrew/bin/claude` — Homebrew macOS

Dependencies: `which` crate, `dirs` crate (home dir), `anyhow` (error handling). Returns `Result<PathBuf>`.

Planned extraction: Move to `crate::claude::find_claude_path()` as shared module to avoid duplication between terminal spawner and bootstrap command.

## KnowledgeDir API (fs/knowledge/dir.rs)

Core API for knowledge file management:

- `KnowledgeDir::new(project_root)` — Constructor from project root path
- `exists() -> bool` — Check if knowledge directory exists
- `has_content() -> bool` — Check if files have content
- `initialize() -> Result<()>` — Create directory with template files (idempotent)
- `file_path(KnowledgeFile) -> PathBuf` — Resolve file path
- `read(KnowledgeFile) -> Result<String>` — Read single file
- `read_all() -> Result<Vec<(KnowledgeFile, String)>>` — Read all files
- `append(KnowledgeFile, content) -> Result<()>` — Append content (append-only design)
- `generate_summary() -> Result<String>` — Summary of all knowledge
- `list_files() -> Result<Vec<(KnowledgeFile, PathBuf)>>` — List file paths

KnowledgeFile enum: Architecture, EntryPoints, Patterns, Conventions, Mistakes, Stack, Concerns. Has `filename()`, `from_filename()`, `all()` methods.
