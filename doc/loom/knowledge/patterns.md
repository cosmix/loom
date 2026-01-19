# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
> This file is append-only - agents add discoveries, never delete.

## State Machine Pattern

### Stage State Machine (10 states)

```
WaitingForDeps --> Queued --> Executing --> Completed (terminal)
                     |           |
                     v           +--> Blocked --> Queued (retry)
                  Skipped        +--> NeedsHandoff --> Queued (new session)
                 (terminal)      +--> WaitingForInput --> Executing
                                 +--> MergeConflict --> Completed
                                 +--> CompletedWithFailures --> Queued
                                 +--> MergeBlocked --> Queued
```

**Critical Invariant**: Dependents transition to `Queued` only when dependencies have BOTH:

- `status == Completed` AND `merged == true`

### Session State Machine (6 states)

```
Spawning --> Running --> Completed (terminal)
                    +--> Crashed (terminal)
                    +--> ContextExhausted (terminal)
                    +--> Paused <--> Running
```

### Transition Validation

All state changes validated via `try_transition()` method before execution:

```rust
stage.status.try_transition(new_status)?  // Returns Err if invalid
```

## File-Based State Pattern

All state persisted to `.work/` directory for git-friendliness and crash recovery:

```
.work/
├── config.toml          # Active plan reference
├── stages/*.md          # Stage state (YAML frontmatter + markdown)
├── sessions/*.md        # Session tracking
├── signals/*.md         # Agent instruction files
├── handoffs/*.md        # Context handoff records
├── heartbeat/*.json     # Session heartbeats
├── checkpoints/         # Task completion records
└── learnings/           # Protected learning files
```

**Benefits:**

- Git-friendly diffing and versioning
- Human-readable state inspection
- Crash recovery via file re-read
- No in-memory state loss

## Manus Pattern (Signal Generation)

KV-cache optimized 4-section signal structure:

```markdown
1. STABLE PREFIX (never changes per agent type)
   - Worktree context, isolation boundaries
   - Execution rules, CLAUDE.md reminders
   - Hash: SHA-256 for debugging cache reuse

2. SEMI-STABLE SECTION (changes per stage, not per session)
   - Knowledge summary
   - Facts store
   - Recent learnings

3. DYNAMIC SECTION (changes per session)
   - Target (session, stage, worktree, branch)
   - Stage assignment and description
   - Dependencies, outputs, handoff

4. RECITATION SECTION (at end for attention)
   - Immediate tasks
   - Session memory (last 10 entries)
```

## Progressive Merge Pattern

Dependencies merged to main before dependent stages execute:

```
Stage A completes --> Merge A to main --> Stage B starts (uses main as base)
```

**Why:** Ensures dependent stages have clean base with all dependency work integrated.

**Base Branch Resolution:**

- No deps: Use `init_base_branch` or default branch
- All deps merged: Use merge point (main)
- Single dep not merged: Use dependency branch (legacy fallback)

## Worktree Isolation Pattern

Each stage gets isolated git worktree:

```
.worktrees/
└── {stage-id}/
    ├── .git              # Worktree git reference
    ├── .work -> ../../.work  # Symlink to shared state
    ├── .claude/          # Claude Code settings (symlinked)
    └── [project files]   # Isolated copy
```

**Branch Naming:** `loom/{stage-id}` for each worktree

**Benefits:**

- Parallel execution without conflicts
- Independent git branches
- Shared `.work/` via symlink

## Daemon IPC Pattern

Unix socket-based IPC with length-prefixed JSON:

```
Client                          Daemon
  |-- Request::SubscribeStatus -->|
  |<-- Response::Ok ---------------|
  |<-- Response::StatusUpdate -----|  (every 1 second)
  |<-- Response::StatusUpdate -----|
  |-- Request::Stop -------------->|
  |<-- Response::Ok ---------------|
```

**Message Format:** 4-byte big-endian length + JSON body (max 10MB)

## Polling Orchestration Pattern

Main loop polls every 5 seconds:

```rust
loop {
    sync_graph_with_stage_files()      // Read state from disk
    sync_queued_status_to_files()      // Write ready stages
    spawn_merge_resolution_sessions()   // Handle conflicts
    start_ready_stages()               // Spawn new sessions
    poll_monitor_for_events()          // Check health
    handle_events()                    // Process crashes/completions

    if exit_condition() { break; }
    sleep(poll_interval);
}
```

## Heartbeat Monitoring Pattern

Sessions write heartbeats for liveness detection:

```json
// .work/heartbeat/{stage-id}.json
{
  "stage_id": "feature-auth",
  "session_id": "session-abc123",
  "timestamp": "2024-01-01T00:00:00Z",
  "context_percent": 45.0,
  "last_tool": "Edit",
  "activity": "Implementing auth flow"
}
```

**Detection:**

- Timeout: 300 seconds (5 minutes) without heartbeat
- PID alive + stale heartbeat = **Hung session**
- PID dead = **Crashed session**

## Context Health Pattern

Three-tier context monitoring:

| Level  | Threshold | Action                                |
| ------ | --------- | ------------------------------------- |
| Green  | 0-60%     | Normal operation                      |
| Yellow | 60-75%    | Auto-summarize memory                 |
| Red    | 75%+      | Generate handoff, trigger new session |

## Retry with Backoff Pattern

Failed stages retry with exponential backoff:

```rust
backoff = min(base * 2^retry_count, max_backoff)
// base: 30 seconds, max: 300 seconds
```

**Retryable Failure Types:** SessionCrash, Timeout
**Non-Retryable:** User intervention required

## Learning Protection Pattern

Learning files protected from agent deletion:

```
1. snapshot_before_session()  // Save state to .snapshots/
2. [Session executes]
3. verify_after_session()     // Compare to snapshot
4. If damaged: restore_from_snapshot()
```

**Protected Marker:** `<!-- .loom-protected -->` at file start

## Append-Only Knowledge Pattern

Knowledge files never truncated:

```rust
append(file_type, content)  // Always append, never overwrite
```

**Files:** `doc/loom/knowledge/{entry-points,patterns,conventions}.md`

## Topological Depth Naming Pattern

Stage files prefixed with execution depth:

```
.work/stages/
├── 01-knowledge-bootstrap.md  # Depth 0 (no deps)
├── 02-implement-feature.md    # Depth 1 (depends on bootstrap)
├── 02-add-logging.md          # Depth 1 (parallel to feature)
└── 03-integration-verify.md   # Depth 2 (depends on all)
```

**Computation:** Iterative topological sort based on dependencies

## Factory Method Pattern (Sessions)

Different session types created via factory methods:

```rust
Session::new()                              // Regular stage session
Session::new_merge(source, target)          // Merge conflict resolution
Session::new_base_conflict(target)          // Base branch conflict
```

## Terminal Backend Abstraction

Trait-based terminal abstraction:

```rust
trait TerminalBackend {
    fn spawn_session() -> Result<Session>;
    fn spawn_merge_session() -> Result<Session>;
    fn spawn_knowledge_session() -> Result<Session>;
    fn kill_session(id) -> Result<()>;
    fn is_session_alive(pid) -> Result<bool>;
}
```

**Implementations:** Native (11 terminal emulators supported)

## PID Tracking via Wrapper Script

Avoid relying on terminal PID:

```bash
#!/bin/bash
echo $$ > ".work/pids/{stage_id}.pid"
exec claude 'prompt...'
```

Claude inherits shell PID after `exec`, enabling reliable tracking.

## Handoff V2 Schema Pattern

Structured handoff format with backward compatibility:

```yaml
---
version: 2
session_id: "session-abc123"
stage_id: "feature-auth"
context_percent: 75.0
completed_tasks:
  - description: "Implemented login"
    files: ["src/auth.rs"]
key_decisions:
  - decision: "Use JWT tokens"
    rationale: "Stateless, scalable"
---
```

**Fallback:** V1 prose markdown for legacy systems

## Acceptance Criteria Execution Pattern

Shell commands with timeout protection:

```rust
run_acceptance_with_config(stage, work_dir, config)
  -> spawn_shell_command("sh -c", command)
  -> wait_with_timeout(5 minutes)
  -> collect CriterionResult { success, stdout, stderr, duration }
```

**Context Variables:** `${WORKTREE}`, `${PROJECT_ROOT}`, `${STAGE_ID}`

## Signal Types by Execution Context

| Signal Type   | Header                    | Context   | Purpose              |
| ------------- | ------------------------- | --------- | -------------------- |
| Regular       | `# Signal:`               | Worktree  | Stage execution      |
| Merge         | `# Merge Signal:`         | Main repo | Auto-merge conflicts |
| Recovery      | `# Recovery Signal:`      | Worktree  | Crash/hung recovery  |
| Knowledge     | `# Signal:`               | Main repo | Knowledge gathering  |
| Base Conflict | `# Base Conflict Signal:` | Main repo | Multi-dep merge      |

## Execution Graph DAG Pattern

Directed acyclic graph for stage scheduling:

```rust
ExecutionGraph {
    nodes: HashMap<String, StageNode>,     // Stage data
    edges: HashMap<String, Vec<String>>,   // Reverse deps (who depends on me)
    parallel_groups: HashMap<String, Vec<String>>,
}
```

**Cycle Detection:** DFS with recursion stack tracking at build time

## TUI Architecture Pattern (ratatui-based)

Two display modes: static (default) and live (--live flag).

**Static Mode**: One-time status print to stdout, then exits.
**Live Mode**: Real-time dashboard with daemon socket subscription.

Key files: loom/src/commands/status/ui/{tui.rs, theme.rs, widgets.rs, layout.rs}
Alternative: loom/src/commands/status/render/live_mode.rs (simpler ANSI output)

### TUI Widgets Used

| Widget    | Usage                                         |
| --------- | --------------------------------------------- |
| Paragraph | Header title, footer help text, empty states  |
| Table     | Executing stages (ID, Name, Elapsed, Session) |
| List      | Pending/Completed/Blocked stage lists         |
| Gauge     | Progress bar with percentage label            |
| Block     | Borders and titles for all sections           |

### TUI Layout Structure (tui.rs:231-239)

Vertical layout with constraints:

- Header: Length(3)
- Progress bar: Length(3)
- Main content: Min(10) - split into 2 columns (50/50)
- Footer: Length(3)

Main content columns: Left (Executing 60% + Pending 40%), Right (Completed 60% + Blocked 40%)

### TUI Theme (theme.rs)

StatusColors: EXECUTING=Blue, COMPLETED=Green, BLOCKED=Red, PENDING=Gray, QUEUED=Cyan, WARNING=Yellow
Context colors: 0-60%=Green, 60-75%=Yellow, 75-100%=Red

Custom widgets (widgets.rs): progress_bar() uses Unicode blocks, status_indicator() maps status to symbols

### TUI Data Flow: Daemon to Dashboard

1. TUI connects to .work/orchestrator.sock, sends Ping/SubscribeStatus
2. Daemon broadcaster thread (broadcast.rs) collects status every 1 second
3. Status collected from .work/stages/\*.md YAML frontmatter (server/status.rs)
4. Response::StatusUpdate sent with stages_executing, pending, completed, blocked
5. TUI event loop: read daemon msgs (non-blocking), poll keyboard (100ms), render

### TUI Keyboard Handling

TUI mode (tui.rs): q or Esc to exit, sends Unsubscribe, restores terminal via Drop
Live mode (live_mode.rs): Ctrl+C handler via ctrlc crate, sends Unsubscribe, calls cleanup_terminal()

Both modes: Daemon continues running after TUI exits. Terminal state properly restored.

## Hook Configuration Pattern (fs/permissions/)

Permission management for Claude Code sessions:

**Main Repo Permissions (LOOM_PERMISSIONS):**

- Read/Write(.work/\*\*) - state directory access
- Read/Write(../../.work/\*\*) - worktree parent traversal
- Read(.claude/**) and Read(~/.claude/**) - CLAUDE.md access
- Bash(loom:\*) - loom CLI prefix matching

**Hook Installation Flow:**

1. `ensure_loom_permissions()` called during `loom init`
2. Hook scripts embedded via `include_str!()` in constants.rs
3. Scripts installed to `~/.claude/hooks/loom/` directory
4. JSON config added to `.claude/settings.local.json`

**Worktree Trust Pattern:**

- `add_worktrees_to_global_trust()` modifies `~/.claude.json`
- Adds `.worktrees/` path to `trustedDirectories` array
- Prevents "trust this folder?" prompt when spawning sessions

**Permission Sync Pattern:**

- After worktree session, sync permissions back to main repo
- Filter out worktree-specific paths (`../../`, `.worktrees/`)
- Exclusive file locking via fs2 crate during merge
- Atomic write to prevent corruption

## Status Collection Pattern (daemon/server/status.rs)

How daemon collects stage status:

1. Read `.work/stages/*.md` files
2. Parse YAML frontmatter via `extract_yaml_frontmatter()`
3. Map status strings to `StageStatus` enum
4. Detect worktree status for executing stages
5. Package into `Response::StatusUpdate`

**Worktree Status Detection:**

- Check if `.worktrees/{stage-id}/` exists
- Check for merge conflicts via `git diff --name-only --diff-filter=U`
- Check for MERGE_HEAD (merge in progress)
- Check if branch manually merged via `is_branch_merged()`
- Return: Active, Conflict, Merging, Merged, or None

## TUI Unified Stage Pattern

Live status dashboard merges all stage categories:

```rust
unified_stages() -> Vec<UnifiedStage> {
    compute_levels()  // DAG depth via recursive dependencies
    collect all stages from executing/pending/completed/blocked
    sort by level, then by id
}
```

**Level Computation:**

- Stages with no deps = level 0
- Otherwise: max(dependency levels) + 1
- Cycle detection via visiting set

**Table Display:**

- Icon + Level + ID + Status + Merged + Elapsed
- Elapsed: live timer for executing, final duration for completed

## Signal KV-Cache Optimization Pattern (Manus)

Four-section signal structure for optimal LLM KV-cache reuse:

| Section       | Stability   | Changes When      | Content                             |
| ------------- | ----------- | ----------------- | ----------------------------------- |
| Stable Prefix | Never       | Agent type change | Worktree isolation, execution rules |
| Semi-Stable   | Per-stage   | Stage changes     | Knowledge, facts, learnings         |
| Dynamic       | Per-session | Each session      | Target, assignment, handoff         |
| Recitation    | Per-session | Each session      | Immediate tasks, session memory     |

**Implementation:** `orchestrator/signals/cache.rs` (stable prefix), `format.rs` (sections)
**Purpose:** Maximize LLM attention on dynamic/recitation content while caching stable prefix

## Six Signal Types Pattern

| Type            | File                    | Use Case                               |
| --------------- | ----------------------- | -------------------------------------- |
| Regular Stage   | generate.rs:20-54       | Normal worktree execution              |
| Knowledge Stage | knowledge.rs:23-56      | Main repo exploration (no commits)     |
| Recovery        | recovery.rs:40-100      | Crash/hung/context recovery            |
| Merge           | merge.rs:14-45          | Auto-merge conflict resolution         |
| Merge Conflict  | merge_conflict.rs:28-53 | Progressive merge failures             |
| Base Conflict   | base_conflict.rs:21-52  | Multi-dependency base branch conflicts |

## Embedded Context Pattern

Self-contained signals via `EmbeddedContext` struct (`signals/types.rs:6-29`):

- Agents NEVER read from main repo - signal file is single source of truth
- Context built at signal generation time with stage-specific data
- Includes: handoff, plan overview, facts, knowledge, task state, learnings, memory

## Skill Trigger Pattern

Skills activated via three mechanisms in SKILL.md frontmatter:

1. **`triggers:` array** - YAML list (auth skill uses 40+ keywords)
2. **`trigger-keywords:` string** - Comma-separated list (testing skill)
3. **Inline in description** - Keywords embedded in description text

**Loading:** Claude Code matches user intent against skill triggers

## Hook Event Pipeline Pattern

Seven hook events with specific integration points:

```
PreToolUse (Bash) --> prefer-modern-tools.sh --> Block with guidance (exit 2)
PreToolUse (AskUser) --> ask-user-pre.sh --> Mark WaitingForInput
PostToolUse --> post-tool-use.sh --> Update heartbeat
PostToolUse (AskUser) --> ask-user-post.sh --> Resume stage
PreCompact --> pre-compact.sh --> Trigger handoff
Stop --> commit-guard.sh --> Block exit without commit
Stop --> learning-validator.sh --> Block exit on damaged learnings
SessionEnd --> session-end.sh --> Cleanup
SubagentStop --> subagent-stop.sh --> Extract learnings
```

## Install Distribution Pattern

**Local install (`./install.sh`):**

1. Read CLAUDE.md.template from repo
2. Prepend timestamp header
3. Write to ~/.claude/CLAUDE.md
4. Copy hooks/ to ~/.claude/hooks/loom/

**Remote install (curl | bash):**

1. Download from GitHub releases
2. Same process with remote files

**Self-update (`loom self-update`):**

1. Download binary, skills.zip, agents.zip from releases
2. Extract skills to ~/.claude/skills/
3. Update ~/.claude/CLAUDE.md from CLAUDE.md.template

## Claude Code Hook Input Pattern (CRITICAL - 2026-01-18)

**Discovery:** Hooks receive data via stdin JSON, NOT environment variables.

**Wrong (all hooks currently do this):**

```bash
if [[ "${TOOL_NAME:-}" != "Bash" ]]; then exit 0; fi
COMMAND="${TOOL_INPUT:-}"
```

**Correct pattern:**

```bash
INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty')
COMMAND=$(echo "$INPUT_JSON" | jq -r '.tool_input.command // empty')
```

**Stdin JSON structure:**

```json
{
  "hook_event_name": "PreToolUse",
  "tool_name": "Bash",
  "tool_input": { "command": "...", "description": "..." },
  "session_id": "...",
  "cwd": "..."
}
```

**Why timeout:** `cat` blocks forever if stdin kept open. Use `timeout 1 cat`.

**Known bug:** Claude Code #9567 - env vars always empty.

## PreToolUse Response Pattern (2026-01-18)

**Simple (exit code):** Exit 0 = allow, Exit 2 = block (stderr shown to Claude)

**Advanced (JSON output with exit 0):**

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow|deny|ask",
    "permissionDecisionReason": "message",
    "updatedInput": { "command": "corrected-command" }
  }
}
```

**Auto-correct pattern (best UX):** Use `permissionDecision: "allow"` with `updatedInput` to fix commands without blocking. Example: strip Co-Authored-By from git commits automatically.

**Block with guidance pattern:** Exit 2 with helpful stderr message. Used when syntax differs too much for auto-correct (e.g., find→fd). Claude receives the guidance and rewrites the command.

## Hook Update Status (2026-01-18)

**FIXED - Read stdin JSON or drain stdin correctly:**

| Hook                   | Event                       | Behavior                                    | Notes                    |
| ---------------------- | --------------------------- | ------------------------------------------- | ------------------------ |
| commit-filter.sh       | PreToolUse:Bash             | Auto-corrects Co-Authored-By out of commits | Uses updatedInput JSON   |
| prefer-modern-tools.sh | PreToolUse:Bash             | Blocks grep/find with guidance to use rg/fd | Exit 2 + stderr guidance |
| post-tool-use.sh       | PostToolUse:\*              | Reads tool_name from stdin for heartbeat    | Silent operation         |
| ask-user-pre.sh        | PreToolUse:AskUserQuestion  | Drains stdin, marks stage WaitingForInput   | Uses LOOM\_\* env vars   |
| ask-user-post.sh       | PostToolUse:AskUserQuestion | Drains stdin, resumes stage                 | Uses LOOM\_\* env vars   |
| session-start.sh       | SessionStart:\*             | Drains stdin, initial heartbeat             | Uses LOOM\_\* env vars   |
| session-end.sh         | SessionEnd:\*               | Drains stdin, cleanup/handoff               | Uses LOOM\_\* env vars   |
| pre-compact.sh         | PreCompact:\*               | Drains stdin, triggers handoff              | Uses LOOM\_\* env vars   |
| skill-trigger.sh       | UserPromptSubmit:\*         | Reads stdin for prompt matching             | Suggests skills          |

**NOT YET UPDATED - May still use env vars:**

| Hook                  | Event Type      | Notes                      |
| --------------------- | --------------- | -------------------------- |
| learning-validator.sh | Stop:\*         | Validates learnings        |
| commit-guard.sh       | Stop:\*         | Blocks exit without commit |
| subagent-stop.sh      | SubagentStop:\* | Extracts learnings         |

**Note:** Stop/SubagentStop hooks may have different input formats. Need to verify before updating.

**Configuration fix (2026-01-18):** Fixed SessionStart to use proper `SessionStart:*` event instead of `PreToolUse:Bash` (was incorrectly running before every Bash command). Added missing `SessionEnd:*` hook configuration.

## Memory/Knowledge Consolidation (2026-01-19)

Three agent knowledge systems in loom:

| System    | Location                  | Scope                |
| --------- | ------------------------- | -------------------- |
| Facts     | .work/facts.toml          | Cross-stage KV pairs |
| Memory    | .work/memory/{session}.md | Session journal      |
| Knowledge | doc/loom/knowledge/       | Permanent curation   |

### Signal Integration Points

EmbeddedContext (types.rs:9-30) holds embedded content:

- facts_content: Table formatted for stage
- memory_content: Last 10 entries
- knowledge_summary: Compact from doc/loom/knowledge/

format.rs section placement:

- Semi-stable (88-186): knowledge, facts, skills
- Recitation (345-355): memory at end for attention

### Consolidation Rationale

Facts system redundant with knowledge:

- Both store key-value information
- Knowledge is persistent project-level
- Facts add complexity without unique value

Approach: Remove facts, add memory promote

- loom memory promote <type> <target>
- Promotes session memory to knowledge files
- Decisions -> patterns, Notes -> entry-points

## Stage Completion Flow

1. Load stage from .work/stages/
2. Route knowledge stages to no-merge path
3. Run acceptance criteria (unless --no-verify)
4. Sync worktree permissions to main
5. Cleanup terminal/session resources
6. Run task verifications if task_state exists
7. Progressive merge into merge point
8. Mark Completed, trigger dependents

## Knowledge Stage Completion

- No worktree (main repo context)
- Auto-sets merged=true (no git merge)
- Uses stage.working_dir for acceptance
- Skips merge attempt entirely

## Acceptance Criteria Execution

1. Build CriteriaContext for variable expansion
2. Expand setup commands, prefix to each criterion
3. Execute each command sequentially with timeout
4. sh -c (Unix) / cmd /C (Windows) execution
5. Return AcceptanceResult (AllPassed/Failed)

## State Transitions (models/stage/transitions.rs)

Terminal: Completed, Skipped (no outgoing transitions)
WaitingForDeps -> Queued | Skipped
Queued -> Executing | Skipped | Blocked
Executing -> Completed | Blocked | NeedsHandoff | WaitingForInput | MergeConflict | CompletedWithFailures | MergeBlocked
CompletedWithFailures -> Queued | Executing (retry)

## Module Splitting Patterns

### File-Per-Module Pattern
- Always use `mod foo;` (separate file per submodule)
- Never use inline block declarations `mod foo { }`
- Each submodule has its own .rs file alongside mod.rs

### Module Re-Export Pattern (mod.rs)
Step 1: Declare submodules at top
Step 2: Re-export public items via `pub use submod::{Item1, Item2};`
- Never use `pub mod` for re-exports
- Never use wildcard imports - always explicit

### Module Test Declaration
- Separate test file: `#[cfg(test)] mod tests;` in mod.rs
- The tests.rs file contains `#[cfg(test)] mod tests { ... }`
- Only compiled during test builds
