# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [architecture.md](architecture.md) for system overview, [conventions.md](conventions.md) for coding standards.

## Table of Contents

- [State Machine Pattern](#state-machine-pattern) - Stage/Session state machines
- [File-Based State Pattern](#file-based-state-pattern) - .work/ directory persistence
- [Signal Generation Patterns](#signal-generation-patterns) - Manus KV-cache optimization
- [Progressive Merge Pattern](#progressive-merge-pattern) - Dependency-ordered merging
- [Worktree Isolation Pattern](#worktree-isolation-pattern) - Git worktree isolation
- [Daemon IPC Pattern](#daemon-ipc-pattern) - Unix socket communication
- [Polling Orchestration Pattern](#polling-orchestration-pattern) - Main loop design
- [Heartbeat Monitoring Pattern](#heartbeat-monitoring-pattern) - Session liveness
- [Context Health Pattern](#context-health-pattern) - Context usage tiers
- [Retry with Backoff Pattern](#retry-with-backoff-pattern) - Exponential backoff
- [Terminal Spawning Patterns](#terminal-spawning-patterns) - Cross-platform terminals
- [Hook Patterns](#hook-patterns) - Claude Code hook integration
- [TUI Patterns](#tui-patterns) - Terminal UI with ratatui
- [Memory/Knowledge Consolidation](#memoryknowledge-consolidation) - Agent knowledge systems
- [Stage Completion Patterns](#stage-completion-patterns) - Completion workflows
- [Error Handling Framework](#error-handling-framework) - anyhow patterns
- [Security Validation Patterns](#security-validation-patterns-2026-01-29) - Input validation, shell escaping

---

## State Machine Pattern

### Stage State Machine (10 states)

```text
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

```text
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

> **Directory structure:** See [architecture.md § Directory Structure](architecture.md#directory-structure) for complete file layout.

All state persisted to `.work/` directory for git-friendliness and crash recovery.

**Key directories:** config.toml (plan reference), stages/_.md (stage state), sessions/_.md (session tracking), signals/_.md (agent instructions), handoffs/_.md (context records).

**Benefits:**

- Git-friendly diffing and versioning
- Human-readable state inspection
- Crash recovery via file re-read
- No in-memory state loss

## Signal Generation Patterns

### Manus Pattern (KV-Cache Optimization)

Four-section structure optimizes LLM KV-cache reuse:

| Section       | Stability   | Changes When      | Content                             |
| ------------- | ----------- | ----------------- | ----------------------------------- |
| Stable Prefix | Never       | Agent type change | Worktree isolation, execution rules |
| Semi-Stable   | Per-stage   | Stage changes     | Knowledge, facts, learnings         |
| Dynamic       | Per-session | Each session      | Target, assignment, handoff         |
| Recitation    | Per-session | Each session      | Immediate tasks, session memory     |

**Implementation:** `orchestrator/signals/cache.rs` (stable prefix), `format.rs` (sections)

### Six Signal Types

| Type            | File                    | Use Case                               |
| --------------- | ----------------------- | -------------------------------------- |
| Regular Stage   | generate.rs:20-54       | Normal worktree execution              |
| Knowledge Stage | knowledge.rs:23-56      | Main repo exploration (no commits)     |
| Recovery        | recovery.rs:40-100      | Crash/hung/context recovery            |
| Merge           | merge.rs:14-45          | Auto-merge conflict resolution         |
| Merge Conflict  | merge_conflict.rs:28-53 | Progressive merge failures             |
| Base Conflict   | base_conflict.rs:21-52  | Multi-dependency base branch conflicts |

**Signal headers by execution context:**

| Signal Type   | Header                    | Context   | Purpose              |
| ------------- | ------------------------- | --------- | -------------------- |
| Regular       | `# Signal:`               | Worktree  | Stage execution      |
| Merge         | `# Merge Signal:`         | Main repo | Auto-merge conflicts |
| Recovery      | `# Recovery Signal:`      | Worktree  | Crash/hung recovery  |
| Knowledge     | `# Signal:`               | Main repo | Knowledge gathering  |
| Base Conflict | `# Base Conflict Signal:` | Main repo | Multi-dep merge      |

### Embedded Context Pattern

Self-contained signals via `EmbeddedContext` struct (`signals/types.rs:6-29`):

- Agents NEVER read from main repo - signal file is single source of truth
- Context built at signal generation time with stage-specific data
- Includes: handoff, plan overview, facts, knowledge, task state, learnings, memory

## Progressive Merge Pattern

Dependencies merged to main before dependent stages execute:

```text
Stage A completes --> Merge A to main --> Stage B starts (uses main as base)
```

**Why:** Ensures dependent stages have clean base with all dependency work integrated.

**Base Branch Resolution:**

- No deps: Use `init_base_branch` or default branch
- All deps merged: Use merge point (main)
- Single dep not merged: Use dependency branch (legacy fallback)

## Worktree Isolation Pattern

> **Full details:** See [architecture.md § Worktree Isolation](architecture.md#worktree-isolation) for 4-layer defense (Git, Sandbox, Signal, Hooks).

Each stage gets isolated git worktree at `.worktrees/{stage-id}/` with branch `loom/{stage-id}`.

**Symlinks:** `.work` → shared state, `.claude/CLAUDE.md` → instructions, `CLAUDE.md` → project guidance.

**Benefits:** Parallel execution without conflicts, independent git branches, shared `.work/` via symlink.

## Daemon IPC Pattern

Unix socket-based IPC with length-prefixed JSON:

```text
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

> **Threshold constants:** See [conventions.md § Context Thresholds](conventions.md#context-thresholds) for exact values.

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

```text
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

```text
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

## Terminal Spawning Patterns

### Backend Abstraction

Trait-based terminal abstraction (`orchestrator/terminal/`):

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

### Emulator Pattern

To add new terminal, modify `TerminalEmulator` enum in `emulator.rs`:

1. Add variant (e.g., TerminalApp, ITerm2)
2. Implement `binary()` - return binary name string
3. Implement `from_binary()` - parse binary name to variant
4. Implement `build_command()` - configure Command with terminal-specific args

**CLI argument variations:**

- kitty: `--title, --directory, [cmd args]`
- alacritty: `--title, --working-directory, -e [cmd]`
- gnome-terminal: `--title, --working-directory, -- [cmd]`
- xterm: `-title, -e [cmd with cd prefix]`

### Detection Pattern

Priority order (`detection.rs:17-63`):

1. `TERMINAL` env var (user preference)
2. `gsettings/dconf` (GNOME/Cosmic DE settings)
3. `xdg-terminal-exec` (emerging standard)
4. Fallback list: kitty, alacritty, foot, wezterm, gnome-terminal

**macOS additions:**

1. Check `TERM_PROGRAM` env var (Terminal.app, iTerm)
2. `defaults read com.apple.LaunchServices`
3. macOS fallback: Terminal.app, iTerm2

### PID Tracking via Wrapper Script

Avoid relying on terminal PID (`pid_tracking.rs:242-332`):

```bash
#!/bin/bash
echo $$ > ".work/pids/{stage_id}.pid"
exec claude 'prompt...'
```

Claude inherits shell PID after `exec`, enabling reliable tracking.

**Current Linux-only:** Uses `/proc` scanning for fallback discovery
**macOS alternatives:** `pgrep -f claude`, `ps aux`, `lsof +D /path/to/worktree`

### Window Operations Pattern

Linux implementation uses `wmctrl/xdotool` (`window_ops.rs`):

- `close_window_by_title()`: wmctrl -c or xdotool windowclose
- `window_exists_by_title()`: wmctrl -l or xdotool search --name

**macOS AppleScript equivalents via osascript:**

- Terminal.app: `tell app Terminal to close window where name contains`
- iTerm2: `tell app iTerm2 to close window`
- Escape double quotes: `replace('"', '\\"')` before embedding
- Window search: `whose name contains` for partial matching

### Session Spawn Flow

1. `NativeBackend::spawn_*_session()` builds title, prompt
2. Creates wrapper script via `pid_tracking::create_wrapper_script()`
3. Calls `spawner::spawn_in_terminal()` with emulator, title, workdir, cmd
4. `spawn_in_terminal()` runs `emulator.build_command()` and spawns
5. Reaper thread (spawner.rs:17-26) prevents zombies via `child.wait()`
6. PID discovery follows via file read or fallback scan

### Session Kill Strategy

Layered approach (`native/mod.rs:344-384`):

1. Close window by title (loom-{stage_id})
2. Fallback: SIGTERM to stored PID
3. Clean up tracking files after either method

### Session Liveness Check

Layered checking (`native/mod.rs:386-428`):

1. Check PID file (most current)
2. Verify PID is alive via `kill -0`
3. Fallback to `session.pid`
4. Final fallback: window existence by title

### macOS Platform Layer

macOS terminal support uses AppleScript via osascript binary.

**Detection priority:** iTerm2 (/Applications/iTerm.app) > cross-platform (kitty, alacritty, wezterm) > Terminal.app (fallback)

**Terminal.app Command Pattern:**

```applescript
osascript -e 'tell application "Terminal"
do script "cd /path && command" in front window
set name of front window to "title"
end tell'
```

**iTerm2 Command Pattern:**

```applescript
osascript -e 'tell application "iTerm2"
set newWindow to (create window with default profile command "cmd")
end tell'
```

### Terminal State Restoration

`utils.rs:22-35` cleanup_terminal() clears line, shows cursor, resets attributes.

Panic hook (lines 41-50) installs terminal cleanup before panic using Once for single installation.

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

## TUI Patterns

### Architecture (ratatui-based)

Two display modes: static (default) and live (--live flag).

**Static Mode**: One-time status print to stdout, then exits.
**Live Mode**: Real-time dashboard with daemon socket subscription.

Key files: `loom/src/commands/status/ui/{tui.rs, theme.rs, widgets.rs, layout.rs}`
Alternative: `loom/src/commands/status/render/live_mode.rs` (simpler ANSI output)

### Widgets Used

| Widget    | Usage                                         |
| --------- | --------------------------------------------- |
| Paragraph | Header title, footer help text, empty states  |
| Table     | Executing stages (ID, Name, Elapsed, Session) |
| List      | Pending/Completed/Blocked stage lists         |
| Gauge     | Progress bar with percentage label            |
| Block     | Borders and titles for all sections           |

### Layout Structure (tui.rs:231-239)

Vertical layout with constraints:

- Header: Length(3)
- Progress bar: Length(3)
- Main content: Min(10) - split into 2 columns (50/50)
- Footer: Length(3)

Main content columns: Left (Executing 60% + Pending 40%), Right (Completed 60% + Blocked 40%)

### Theme (theme.rs)

**Status Colors:** EXECUTING=Blue, COMPLETED=Green, BLOCKED=Red, PENDING=Gray, QUEUED=Cyan, WARNING=Yellow
**Context Colors:** 0-60%=Green, 60-75%=Yellow, 75-100%=Red

Custom widgets (widgets.rs): `progress_bar()` uses Unicode blocks, `status_indicator()` maps status to symbols

### Data Flow: Daemon to Dashboard

1. TUI connects to `.work/orchestrator.sock`, sends Ping/SubscribeStatus
2. Daemon broadcaster thread (`broadcast.rs`) collects status every 1 second
3. Status collected from `.work/stages/*.md` YAML frontmatter (`server/status.rs`)
4. `Response::StatusUpdate` sent with stages_executing, pending, completed, blocked
5. TUI event loop: read daemon msgs (non-blocking), poll keyboard (100ms), render

### Unified Stage Pattern

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

### Keyboard Handling

**TUI mode (tui.rs):** q or Esc to exit, sends Unsubscribe, restores terminal via Drop
**Live mode (live_mode.rs):** Ctrl+C handler via ctrlc crate, sends Unsubscribe, calls cleanup_terminal()

Both modes: Daemon continues running after TUI exits. Terminal state properly restored.

## Hook Patterns

### Configuration (fs/permissions/)

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

### Event Pipeline

Seven hook events with specific integration points:

```text
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

### Input Pattern (CRITICAL)

Hooks receive data via stdin JSON, NOT environment variables.

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

**Correct pattern:**

```bash
INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty')
COMMAND=$(echo "$INPUT_JSON" | jq -r '.tool_input.command // empty')
```

**Why timeout:** `cat` blocks forever if stdin kept open. Use `timeout 1 cat`.

### Response Patterns

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

### Current Hook Status

**Stdin-aware hooks:**

| Hook                   | Event                       | Behavior                                    |
| ---------------------- | --------------------------- | ------------------------------------------- |
| commit-filter.sh       | PreToolUse:Bash             | Auto-corrects Co-Authored-By out of commits |
| prefer-modern-tools.sh | PreToolUse:Bash             | Blocks grep/find with guidance to use rg/fd |
| post-tool-use.sh       | PostToolUse:\*              | Reads tool_name from stdin for heartbeat    |
| ask-user-pre.sh        | PreToolUse:AskUserQuestion  | Drains stdin, marks stage WaitingForInput   |
| ask-user-post.sh       | PostToolUse:AskUserQuestion | Drains stdin, resumes stage                 |
| session-start.sh       | SessionStart:\*             | Drains stdin, initial heartbeat             |
| session-end.sh         | SessionEnd:\*               | Drains stdin, cleanup/handoff               |
| pre-compact.sh         | PreCompact:\*               | Drains stdin, triggers handoff              |
| skill-trigger.sh       | UserPromptSubmit:\*         | Reads stdin for prompt matching             |

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

## Skill Trigger Pattern

Skills activated via three mechanisms in SKILL.md frontmatter:

1. **`triggers:` array** - YAML list (auth skill uses 40+ keywords)
2. **`trigger-keywords:` string** - Comma-separated list (testing skill)
3. **Inline in description** - Keywords embedded in description text

**Loading:** Claude Code matches user intent against skill triggers

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

## Memory/Knowledge Consolidation

Three agent knowledge systems in loom:

| System    | Location                  | Scope                |
| --------- | ------------------------- | -------------------- |
| Facts     | .work/facts.toml          | Cross-stage KV pairs |
| Memory    | .work/memory/{session}.md | Session journal      |
| Knowledge | doc/loom/knowledge/       | Permanent curation   |

**Signal Integration:** EmbeddedContext (types.rs:9-30) holds embedded content included in signals:

- `facts_content`: Table formatted for stage
- `memory_content`: Last 10 entries
- `knowledge_summary`: Compact from doc/loom/knowledge/

**Section placement in format.rs:**

- Semi-stable (88-186): knowledge, facts, skills
- Recitation (345-355): memory at end for attention

**Memory promotion:** `loom memory promote <type> <target>` promotes session memory to knowledge files (Decisions → patterns, Notes → entry-points)

## Stage Completion Patterns

**Regular stages:**

1. Load stage from .work/stages/, run acceptance criteria (unless --no-verify)
2. Sync worktree permissions to main, cleanup terminal/session resources
3. Run task verifications if task_state exists
4. Progressive merge into merge point, mark Completed, trigger dependents

**Knowledge stages:**

- No worktree (main repo context), auto-sets merged=true (no git merge)
- Uses stage.working_dir for acceptance, skips merge attempt entirely

## Acceptance Criteria Execution

Shell commands with timeout protection (`verify/acceptance_runner.rs`):

```rust
run_acceptance_with_config(stage, work_dir, config)
  -> spawn_shell_command("sh -c", command)
  -> wait_with_timeout(5 minutes)
  -> collect CriterionResult { success, stdout, stderr, duration }
```

**Context Variables:** `${WORKTREE}`, `${PROJECT_ROOT}`, `${STAGE_ID}`

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

## Shell Completion Architecture

Two-tier system: static + dynamic completions.

Static (generator.rs): clap_complete generates scripts user sources in shell rc.
Dynamic (dynamic/mod.rs): Shell calls hidden `loom complete` for context-aware values.

### Static Completion Pattern

Shell enum (Bash, Zsh, Fish) with FromStr for parsing.
generate_completions() uses clap_complete::generate() with shell-specific Generator impl.
User runs: eval "$(loom completions bash)" in .bashrc

### Dynamic Completion Pattern

CompletionContext struct holds: cwd, shell, cmdline, current_word, prev_word.
complete_dynamic() matches prev_word to determine completion type:

- init -> plan files
- verify/merge/resume -> stage IDs
- kill (in sessions) -> session IDs
- complete/block/reset/waiting (in stage) -> stage IDs

### Completion Integration with fs Module

Stage ID extraction: Uses fs::stage_files::extract_stage_id() to strip depth prefix.
Example: 01-knowledge-bootstrap.md -> knowledge-bootstrap

File scanning: Standard fs::read_dir() with extension filtering (.md only).

## Timing Persistence Pattern (Stage Model)

- started_at set on FIRST Executing transition (preserved across retries)
- completed_at set when stage reaches terminal state
- duration_secs computed: (completed_at - started_at).num_seconds()
- Persisted in .work/stages/\*.md YAML frontmatter

## Orchestrator Completion Detection Pattern

- is_complete(): ALL stages Completed OR Skipped
- all_stages_terminal(): includes Blocked, MergeConflict states
- Normal exit: graph.is_complete() OR (failed + no sessions + no ready)

## Status Broadcast Pattern

- Daemon polls .work/stages/\*.md every 1 second
- Response::StatusUpdate sent to all subscribed clients
- Four stage categories: executing, pending, completed, blocked
- LiveStatus.unified_stages() merges and deduplicates

## Memory Enforcement (Defense in Depth)

Memory recording enforced through multiple touchpoints:

1. **Signal recitation** (format.rs:333-361): ALWAYS shows memory section
2. **Stable prefix** (cache.rs:122-126): "Session Memory (MANDATORY)" section
3. **CLAUDE.md.template**: CRITICAL box + checklist with MEMORY at top
4. **loom-plan-writer skill**: MEMORY RECORDING block in stage templates
5. **commit-guard hook**: Soft reminder when no entries found
6. **Wrapper script** (pid_tracking.rs:316-318): Sets LOOM_SESSION_ID env var

Pattern: Defense in depth - agents see memory prompts at session start,
resume, completion checklist, and exit hook.

## Directory Hierarchy Pattern (Three-Level Model)

Loom uses a three-level directory model for stage execution:

| Level        | Path                   | Purpose                          |
| ------------ | ---------------------- | -------------------------------- |
| Project Root | /path/to/project       | Main repo where doc/plans/ lives |
| Worktree     | .worktrees/<stage-id>/ | Isolated copy mirroring project  |
| working_dir  | YAML field             | Subdirectory within worktree     |

### Path Resolution Formula

EXECUTION_PATH = worktree_root + working_dir

Resolution logic (acceptance_runner.rs:16-45):

1. working_dir = "." → Use worktree root directly
2. working_dir = "loom" → Join: .worktrees/<stage>/loom/
3. Subdirectory missing → Fall back to worktree root with warning

### working_dir Common Mistakes

| Mistake                                           | Fix                                            |
| ------------------------------------------------- | ---------------------------------------------- |
| cargo test with wrong working_dir                 | Set working_dir to dir with Cargo.toml         |
| Paths like loom/src/file.rs when working_dir=loom | Use src/file.rs (relative to working_dir)      |
| ./target/debug/app from project root              | Use ./loom/target/debug/app OR set working_dir |

### Signal vs Stage Context

Signals show worktree path (format.rs:209) but NOT working_dir.
Stage YAML contains working_dir field (types.rs:64-66).
Agents must check stage definition for working_dir context.

## Error Handling Framework

Uses anyhow::Result<T> (main.rs:3, Cargo.toml:12-13)
Context patterns: .context(), .with_context(|| format!())
Validation: bail!() for explicit errors

Key examples:

- orchestrator.rs:96 - context on backend creation
- git/merge.rs:59-64 - with_context with format!
- git/worktree/operations.rs:46 - bail! for validation

## Graceful Error Degradation

Non-critical path patterns:

- orchestrator.rs:120-142 - Skill loading with warning fallback
- orchestrator.rs:221-241 - if let Ok() for stage loading
- process/mod.rs:30-36 - unwrap_or(false) for liveness

Critical: Zero unwrap()/expect() in main code. Assertions only for invariants.

## Daemon Socket Security

- Mode 0o600 (owner only) - daemon/server/lifecycle.rs:123
- Max 100 connections - daemon/server/core.rs:10
- 10 MB message limit - daemon/protocol.rs:175-177
- Unix socket (no network exposure)

## Self-Update Security

Uses minisign verification - commands/self_update/signature.rs:9-37
Public key: RWTHfjV12CKdjuXF6DPYXsOoneV6zG4nt4Qd1DFe7JzSIXTXKfRJPHjJ
Limits: 50MB binary, 4KB signature
Install: temp file → backup → atomic rename → rollback on failure

## Input Validation

Whitelist: alphanumeric + dash/underscore - validation.rs:55-81
MAX_ID_LENGTH: 128 chars
Reserved names blocked (., .., CON, PRN, etc.)
safe_filename() strips traversal - validation.rs:179-185

## Goal-Backward Verification Pattern

Problem: Tests passing ≠ Feature working. Code compiles but never wired up.

Solution: Three verification layers in stage definitions:

1. truths - Shell commands that return 0 if behavior works
2. artifacts - Files must exist with real implementation
3. wiring - Grep patterns verify connections

Example YAML usage:
truths: - 'curl -f <http://localhost/health>'
artifacts: - 'src/auth/\*.rs'
wiring: - source: src/main.rs
pattern: 'use auth::'
description: Auth module imported

## Knowledge Commands Independence

Knowledge commands work without .work/ directory.

WorkDir::main_project_root() works when .work doesn't exist - returns current directory.

## Phantom Merge Verification Gap

Merge handler sets merged=true after git merge reports success WITHOUT verifying commit is in target branch history.

**Root cause files:**

- merge_handler.rs:167-202 - try_auto_merge sets merged immediately
- merge_status.rs:58-61 - check_merge_state short-circuits on merged flag

**Affected files for fix:**

- loom/src/git/merge.rs - Add verify_merge_succeeded() using is_ancestor_of()
- loom/src/orchestrator/core/merge_handler.rs - Verify before setting merged=true
- loom/src/commands/status/merge_status.rs - Remove short-circuit, always verify ancestry

## Agent Anti-Pattern: Binary Usage

Agents use target/debug/loom instead of loom from PATH. Causes version mismatch and state corruption.

**Currently missing from:**

- CLAUDE.md.template - no explicit prohibition
- cache.rs signal generation - no binary usage warning

## Agent Anti-Pattern: Direct State Editing

Agents edit .work/stages/\*.md files directly to set merged=true instead of using loom CLI.

**Currently missing from:**

- CLAUDE.md.template - no explicit prohibition against editing .work/
- cache.rs signal generation - no state file editing warning

## StageType Enum (plan/schema/types.rs:5-18)

Three variants with kebab-case YAML serialization:

- Standard (default) - Regular implementation stages
- Knowledge - Knowledge-gathering stages (no worktree)
- IntegrationVerify - Final verification stages

YAML: stage_type: knowledge | standard | integration-verify

## Stage Type Detection (models/stage/methods.rs:351-363)

is_knowledge_stage() returns true if:

1. stage_type == StageType::Knowledge, OR
2. ID contains 'knowledge' (case-insensitive), OR
3. Name contains 'knowledge' (case-insensitive)

## Knowledge vs Standard Stage Execution

Knowledge stages (stage_executor.rs:78-86):

- No worktree (runs in main repo)
- No commits/merges required (auto merged=true)
- Mark Executing immediately, then start_knowledge_stage()
- PID tracked as knowledge-{stage_id}

Standard stages:

- Create worktree first (.worktrees/{id}/)
- Require commits and progressive merge
- Resolve base branch from dependencies before Executing

## Signal Generation by Stage Type

Knowledge signals (signals/knowledge.rs):

- generate_knowledge_signal(session, stage, repo_root, deps, work_dir)
- No worktree path, no git history
- Type marker: 'Knowledge (no worktree)'

Regular signals (signals/generate.rs):

- generate_signal_with_skills(session, stage, worktree, deps, ...)
- Includes worktree isolation warnings
- Git history and commit requirements

## Stage Type Validation (plan/schema/validation.rs:232-246)

Goal-backward checks (truths/artifacts/wiring) required ONLY for Standard stages.
Knowledge and IntegrationVerify stages are EXEMPT.

Validation warns if plan has no knowledge stage (lines 260-283).

## Git Hook System

### Hook Infrastructure

Location: fs/permissions/hooks.rs, hooks/\*.sh (embedded via include_str!)
Installed to: ~/.claude/hooks/loom/

Key hooks:

- commit-guard.sh (Stop): Blocks exit if uncommitted changes or stage incomplete
- prefer-modern-tools.sh (PreToolUse): Blocks grep/find, suggests Grep/Glob tools
- commit-filter.sh (PreToolUse): Blocks commits with Claude/Anthropic attribution AND subagent commits

### Subagent Commit Prevention (CRITICAL)

**Problem:** Subagents spawned via Task tool were committing partial work or calling `loom stage complete`, causing LOST WORK.

**Solution:** Three-layer defense:

1. **Documentation** (CLAUDE.md.template Rule 5): Subagent restrictions in every Task prompt
2. **Signal injection** (cache.rs): Subagent restrictions in stable prefix
3. **Hook enforcement** (commit-filter.sh): Blocks git commit and loom stage complete from subagents

**Detection mechanism:**

- Wrapper script (pid_tracking.rs) exports `LOOM_MAIN_AGENT_PID=$$`
- Main agent: `$PPID == $LOOM_MAIN_AGENT_PID` (hook's parent is main agent)
- Subagent: `$PPID != $LOOM_MAIN_AGENT_PID` (hook's parent is subagent, not main)
- Hook compares $PPID to $LOOM_MAIN_AGENT_PID to detect subagent context

**Blocked operations for subagents:**

- `git commit` (any form)
- `git add -A` or `git add .` (bulk staging)
- `loom stage complete` (stage completion)

**Allowed for subagents:**

- Writing code to assigned files
- Running tests
- Reporting results back to main agent
- Reading files

Config env vars: LOOM_STAGE_ID, LOOM_SESSION_ID, LOOM_WORK_DIR, LOOM_MAIN_AGENT_PID

## Git Hooks (continued)

### Session Lifecycle Hooks

- session-start.sh: Initializes heartbeat at .work/heartbeat/{stage}.json
- post-tool-use.sh: Updates heartbeat, reminds about knowledge/memory after commits
- pre-compact.sh: Triggers handoff on context exhaustion
- session-end.sh: Logs completion event

Hook events logged to: .work/hooks/events.jsonl (JSON Lines format)

### User Input Hooks

- ask-user-pre.sh: Marks stage WaitingForInput, sends desktop notification
- ask-user-post.sh: Resumes stage after user answers

### Skill System Hooks

- skill-trigger.sh (UserPromptSubmit): Suggests skills based on prompt keywords
- skill-index-builder.sh: Builds keyword index from SKILL.md files

Config in .claude/settings.json with env vars: LOOM_STAGE_ID, LOOM_SESSION_ID, LOOM_WORK_DIR, LOOM_MAIN_AGENT_PID

## Signal Cache System

> **See also:** [Signal Generation Patterns](#signal-generation-patterns) for full signal structure.

### Manus KV-Cache Pattern (orchestrator/signals/cache.rs)

SignalMetrics tracks signal size, token estimates, stable prefix hash.

Signal sections for LLM cache optimization:

1. Stable prefix (cacheable): Worktree context, isolation rules
2. Semi-stable: Plan overview, knowledge context
3. Dynamic: Assignment, acceptance criteria
4. Recitation: Context restoration hints

### Prefix Generators

- generate_stable_prefix(): For worktree agents - isolation rules, git staging warnings
- generate_knowledge_stable_prefix(): For main repo knowledge stages - exploration focus

Hash computed via SHA-256 (first 16 hex chars) for cache debugging.
Stable prefixes are deterministic across invocations to maximize KV-cache hits.

## Security Validation Patterns (2026-01-29)

### Input Validation at Boundaries

CRITICAL: All IDs used in path construction MUST be validated with validate_id() from crate::validation.

Files requiring validation:

- `fs/stage_loading.rs` - validate stage_id after YAML parse
- `fs/verifications.rs` - validate stage_id in store/load/delete
- `fs/stage_files.rs` - defensive check in find_stage_file
- `diagnosis/signal.rs` - validate session_id before path use

### Shell Command Security

When building shell commands for terminal spawning:

- NEVER concatenate user input directly into command strings
- Use argument arrays instead of shell string interpolation
- Single quotes in content need escaping (MateTerminal)
- Working directories should be validated paths, not string concatenation (XTerm)

### Release Asset Security

Current state: Only binary files are signature-verified via minisign.

Gap: Non-binary release assets lack verification:

- CLAUDE.md.template
- agents.zip
- skills.zip

Recommended: Add SHA256 checksum verification for all release assets.

### Environment Variable Expansion

When expanding ${VAR} in strings, use positional/indexed replacement:

- WRONG: content.replace(var_pattern, value) - affects ALL occurrences
- RIGHT: Replace only at matched position to handle overlapping names ($FOO vs $FOOBAR)

### Race Condition Prevention

Lock acquisition patterns should use atomic operations:

- File locks: Use flock/fcntl with proper error handling
- Sequence numbers: Use atomic increment or lock file

## Shell String Escaping Pattern

When constructing shell commands with untrusted input (paths, stage IDs, commands):

- **Single-quoted strings**: Escape single quotes with pattern `'\''` (end quote, escaped quote, start quote)
- **Double-quoted strings**: Escape backslashes and double quotes
- **AppleScript strings**: Escape backslashes and double quotes with backslash prefix

Location: `orchestrator/terminal/emulator.rs` - escape_applescript_string(), escape_shell_single_quote()

## Promoted from Memory [2026-01-29 18:44]

### Decisions

- **Fixed shell injection vulnerabilities in MateTerminal and XTerm emulator commands by adding escape_shell_single_quote() function**
  - _Rationale:_ MateTerminal used unescaped single quotes allowing command injection. XTerm concatenated workdir directly into shell command. Both now properly escape using standard shell escaping pattern.

## Promoted from Memory [2026-01-29 21:33]

### Notes

- Integration verification passed for worktree isolation enforcement: All acceptance criteria met (cargo test, clippy, build). Sandbox defaults include deny rules for path traversal. Signal generation includes Worktree Isolation section with ALLOWED and FORBIDDEN lists. Hook enforcement validates bash commands and file paths.
- Goal-backward verification truths were prose descriptions instead of shell commands. Manually verified: 1) Sandbox defaults in types.rs:155-179 include deny_read with ../../**and ../.worktrees/**, deny_write with ../../**and .work/stages/**, .work/sessions/\*\*. 2) Signal format/sections.rs includes Worktree Isolation section. 3) hooks/validators/bash.rs validates git -C, path traversal, cross-worktree access.

## Worktree Isolation Details (2026-01-29)

> **See also:** [Worktree Isolation Pattern](#worktree-isolation-pattern) above, [architecture.md § Worktree Isolation](architecture.md#worktree-isolation) for 4-layer defense.

Implementation verification:

1. Sandbox defaults (types.rs:155-179): deny ../../**, ../.worktrees/**, .work/stages/\*\*
2. Signal generation (sections.rs:319-349): ALLOWED/FORBIDDEN lists
3. Hook enforcement (worktree-isolation.sh): blocks git -C, path traversal

## Bug Fix Patterns (2026-01-29)

### Permission Sync with File Locking

Location: fs/permissions/sync.rs:120-180

Pattern for atomic file updates:

1. Open file with read/write/create/no-truncate
2. Acquire exclusive lock (fs2::FileExt::lock_exclusive)
3. Read current content from locked handle
4. Modify in memory
5. Truncate, seek to start, write to SAME locked handle
6. Lock releases on drop

CRITICAL: Writing to a new File handle bypasses the lock. Always use the locked handle.

### Daemon Graceful Shutdown

Location: daemon/server/lifecycle.rs

Shutdown sequence:

1. Client sends Request::Stop via socket
2. Server checks shutdown_flag in accept loop (100ms sleep on WouldBlock)
3. Server waits for orchestrator, log tailer, broadcaster threads
4. cleanup() removes socket, PID, completion marker
5. Drop impl ensures cleanup on panic

Timeout handling: If 5s timeout, suggest manual kill with PID file path.

### Session State Detection

Location: orchestrator/monitor/detection.rs

Detection struct tracks: last_stage_states, last_session_states, last_context_levels, reported_hung_sessions.

Event deduplication: reported_hung_sessions HashSet prevents duplicate hung events. Cleared on fresh heartbeat.

### Session Crash vs Hung Logic

- Crash: PID dead AND stage not Completed
- Hung: PID alive AND heartbeat stale > timeout
- Normal exit: PID dead BUT stage already Completed (skip crash event)

This prevents false crash reports when session exits normally after completing work.

### Init Error Rollback

Location: commands/init/execute.rs

Error levels:

- Fatal: validate_work_dir_state() - abort immediately
- Fatal: plan parsing - cleanup with remove_work_directory_on_failure(), return error
- Non-fatal: git hook install - log warning, continue

## Worktree Path Resolution Patterns

### Direct Path for .work Access

For commands that only need to access .work state files, use direct Path:

- CORRECT (complete.rs pattern): `let work_dir = Path::new(".work");`
- Follows symlinks automatically
- AVOID: `WorkDir::new(".")?.load()?` unless you need full functionality

### main_project_root() for Cross-Worktree Operations

When writing to main repo from a worktree (e.g., knowledge files):

- Use `work_dir.main_project_root()` to resolve through symlinks
- Returns the actual main repo path, not the worktree path
- See knowledge.rs for the canonical pattern

### Hook Regex Precision

When matching file/path patterns in hook scripts:

- AVOID: `.*\.work` matches .work as substring anywhere
- PREFER: Word boundary or path segment matching for precision
- Test hooks against edge cases like paths containing .workflow or myfile.work.txt

## Claude Code Settings Format (SANDBOX)

The implementation in `src/sandbox/settings.rs` generates the correct Claude Code sandbox format.

### Correct Claude Code Sandbox Format

sandbox:
  enabled: true
  autoAllowBashIfSandboxed: true
  excludedCommands: [list]
  allowUnsandboxedCommands: true  # optional, when escape hatch needed
  network:
    allowedDomains: [list]
    allowLocalBinding: true  # optional
permissions:
  deny: [list]
  allow: [list]

### Key Format Rules

1. Use `sandbox.enabled` field to control sandbox state
2. `excludedCommands` goes INSIDE sandbox object, not at root level
3. Network domains in `sandbox.network.allowedDomains`
4. `autoAllowBashIfSandboxed` enables automatic bash approval when sandbox is active
5. File permissions (Read/Write/Edit) use `permissions.deny` and `permissions.allow`
