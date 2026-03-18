---
name: loom-usage
description: |
  Meta-orchestration skill for Claude driving loom. Covers the full loom lifecycle
  from plan initialization through execution, monitoring, debugging, and recovery.
  Use this when Claude itself needs to operate loom — running plans, interpreting
  status, recovering from failures, managing state, and coordinating multi-stage
  execution.

  USE WHEN: Claude needs to run loom commands, debug failed stages, interpret
  loom status output, recover from crashes, manage worktrees, or orchestrate
  multi-stage execution.

  DO NOT USE: For writing loom plans (use /loom-plan-writer), for designing
  wiring checks (use /loom-wiring-test), for before/after verification pairs
  (use /loom-before-after), or for dead code detection config (use /loom-dead-code-check).
triggers:
  - "loom run"
  - "loom init"
  - "loom status"
  - "loom stage"
  - "loom usage"
  - "run loom"
  - "execute plan"
  - "loom orchestration"
  - "loom debug"
  - "loom recover"
  - "stage failed"
  - "stage blocked"
  - "loom worktree"
  - "merge conflict loom"
  - "loom daemon"
  - "loom clean"
  - "loom repair"
  - "context exhausted"
  - "handoff"
  - "loom memory"
  - "loom knowledge"
  - "acceptance criteria failed"
  - "loom check"
  - "orchestrate"
  - "multi-stage"
  - "parallel stages"
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# Loom Usage — Meta-Orchestration for Claude

## Overview

This skill enables Claude to **drive loom itself** — initializing plans, running
orchestration, monitoring execution, debugging failures, and recovering from
errors. This is meta-orchestration: Claude operating loom as a tool rather than
working inside a loom worktree as an agent.

## When to Use

- Running a loom plan end-to-end
- Interpreting `loom status` output and deciding next steps
- Debugging why a stage failed, is blocked, or has merge conflicts
- Recovering from crashes, context exhaustion, or corrupted state
- Managing worktrees, sessions, and daemon lifecycle
- Understanding `.work/` directory structure and signal files

## Complementary Skills

| Skill | Use For |
| --- | --- |
| `/loom-plan-writer` | Creating new plan files |
| `/loom-wiring-test` | Designing wiring verification YAML |
| `/loom-before-after` | Before/after delta-proof verification pairs |
| `/loom-dead-code-check` | Dead code detection configuration |
| **This skill** | Everything after the plan is written |

---

## The Loom Lifecycle

```text
WRITE PLAN          VALIDATE & INIT         EXECUTE            MONITOR & DEBUG
─────────────────── ─────────────────────── ────────────────── ──────────────────
/loom-plan-writer → loom init plan.md     → loom run         → loom status
                    loom repair --fix                           loom check
                                                               loom diagnose
                                                               loom stage retry
                                          ────────────────── ──────────────────
                                          COMPLETE            CLEAN UP
                                          ────────────────── ──────────────────
                                          All stages merged → loom clean
                                          Plan → DONE-PLAN-*
```

---

## Phase 1: Validate & Initialize

### Pre-Flight Checks

Before initializing, verify the workspace is healthy:

```bash
# Detect and fix common issues (missing hooks, stale state, gitignore gaps)
loom repair --fix

# If re-running a plan, clean previous state first
loom clean --all
```

### Initialize a Plan

```bash
loom init doc/plans/PLAN-my-feature.md
```

**What happens:**

1. Parses the plan markdown, extracts YAML metadata
2. Validates all stages (IDs, dependencies, goal-backward fields)
3. Creates `.work/` directory with stage files, config.toml
4. Installs git hooks (commit guard, commit filter)
5. Builds execution DAG, checks for cycles

**Common init failures and fixes:**

| Error | Cause | Fix |
| --- | --- | --- |
| "stage X missing truths/artifacts/wiring" | Standard stage without goal-backward verification | Add at least one of truths, artifacts, or wiring |
| "cycle detected" | Circular dependencies between stages | Remove the cycle in the dependency graph |
| "invalid stage_type" | PascalCase like "Standard" | Use lowercase: "standard", "knowledge", "integration-verify" |
| "working_dir required" | Stage missing working_dir field | Add `working_dir: "."` or appropriate subdirectory |
| "path traversal" | `../` in any path field | Use paths relative to working_dir, no `../` |
| "triple backticks in YAML" | Code fences inside description | Use plain indented text instead of fences |

### Re-Initialization

```bash
# Clean slate — removes .work/, worktrees, sessions
loom clean --all

# Then re-init
loom init doc/plans/PLAN-my-feature.md

# Or use --clean flag to do both in one step
loom init doc/plans/PLAN-my-feature.md --clean
```

---

## Phase 2: Execute

### Start Orchestration

```bash
# Default: background daemon, up to 4 parallel sessions
loom run

# With options
loom run --max-parallel 2      # Limit concurrency
loom run --manual              # Require approval before each stage
loom run --no-merge            # Skip auto-merge (manual merge later)
loom run --foreground          # Debug mode (blocks terminal)
```

**What `loom run` does:**

1. Renames plan file: `PLAN-*` → `IN_PROGRESS-PLAN-*`
2. Spawns background daemon on Unix socket (`.work/orchestrator.sock`)
3. Daemon polls every 5 seconds:
   - Loads stage files → builds execution graph
   - Finds stages with all dependencies completed+merged
   - Creates worktrees, generates signals, spawns Claude Code sessions
   - Monitors sessions for crashes, context exhaustion, completion
4. Auto-merges completed stages to main branch (progressive merge)
5. When all stages complete → renames to `DONE-PLAN-*`

### Plan File Lifecycle

| State | Filename | When |
| --- | --- | --- |
| Not started | `PLAN-feature.md` | After writing |
| Running | `IN_PROGRESS-PLAN-feature.md` | After `loom run` |
| Finished | `DONE-PLAN-feature.md` | All stages merged |

---

## Phase 3: Monitor

### Check Status

```bash
# One-time snapshot
loom status

# Live TUI dashboard (real-time updates via daemon socket)
loom status --live

# Compact single-line per stage (good for scripting)
loom status --compact

# Verbose — includes failure details
loom status --verbose
```

### Read the Execution Graph

```bash
loom graph
```

**Status icons:**

- `✓` Completed
- `●` Executing
- `▶` Ready/Queued
- `○` WaitingForDeps
- `✗` Blocked
- `⟳` NeedsHandoff
- `⚡` MergeConflict
- `?` WaitingForInput
- `⊘` Skipped
- `⚠` CompletedWithFailures
- `⊗` MergeBlocked

### Interpret Status Output

When checking status, look for these patterns:

**Healthy execution:**

- Stages flowing left-to-right through the DAG
- Context usage < 60% (green)
- Sessions running with heartbeat

**Warning signs:**

- Context usage 60-75% (yellow) — handoff may be needed soon
- Stage stuck in Executing for a long time — check session liveness
- Multiple retries on same stage — investigate root cause

**Failure indicators:**

- Stage in Blocked state — read the block reason
- MergeConflict — needs manual resolution or retry
- NeedsHandoff — context exhausted, will auto-resume
- CompletedWithFailures — acceptance passed but with warnings

---

## Phase 4: Debug & Recover

### Stage Failed or Blocked

```bash
# See detailed failure information
loom status --verbose

# Run goal-backward verification to see what's missing
loom check <stage-id>

# Get fix suggestions
loom check <stage-id> --suggest

# Spawn diagnostic Claude session
loom diagnose <stage-id>
```

### Common Failure Scenarios

#### Acceptance Criteria Failed

The stage's acceptance commands returned non-zero.

```bash
# Dry-run acceptance to see detailed results
loom stage verify <stage-id> --dry-run

# After fixing issues, retry
loom stage retry <stage-id>
```

**Debugging tips:**

- Read stderr even when exit code is 0 — silent failures are common
- Check that `working_dir` is correct (most common cause of "command not found")
- Verify paths are relative to working_dir (no double-paths like `loom/loom/src/`)

#### Merge Conflict

Stage completed but merge to main failed.

```bash
# Option 1: Retry the merge (if conflict was transient)
loom stage retry-merge <stage-id>

# Option 2: Manual resolution
cd .worktrees/<stage-id>
git merge main                    # Resolve conflicts
git add <resolved-files>
git commit -m "resolve merge conflict"
loom stage merge-complete <stage-id>
```

#### Context Exhausted

Agent ran out of context window. Loom auto-creates handoff.

```bash
# Check handoff was created
ls .work/handoffs/

# Resume with new session (reads handoff automatically)
loom resume <stage-id>
```

#### Session Crashed

Claude Code process died unexpectedly.

```bash
# Check crash report
ls .work/crashes/

# Recover — creates recovery signal and requeues
loom stage recover <stage-id>

# Or force retry if recovery doesn't help
loom stage retry <stage-id> --force
```

#### Stage Stuck in Executing

Session appears alive but making no progress.

```bash
# Check session liveness
loom sessions list

# Kill the stuck session
loom sessions kill --stage <stage-id>

# Reset and retry
loom stage reset <stage-id> --kill-session
loom stage retry <stage-id>
```

#### Daemon Not Running

```bash
# Check if daemon is alive
loom status  # Will show error if daemon is down

# Restart
loom run  # Safe to re-run; picks up where it left off
```

---

## Phase 5: Manual Stage Management

### Hold/Release Stages

Prevent a stage from auto-executing:

```bash
loom stage hold <stage-id>       # Prevent execution
loom stage release <stage-id>    # Allow execution
```

### Skip a Stage

Mark a stage as intentionally skipped (dependents will be blocked):

```bash
loom stage skip <stage-id> --reason "not needed for this iteration"
```

### Force Complete

When a stage is functionally done but acceptance criteria are wrong:

```bash
# Request human review (preferred)
loom stage dispute-criteria <stage-id> "criteria X is incorrect because..."

# Force complete (use sparingly)
loom stage complete <stage-id> --force-unsafe --assume-merged
```

### Pass Data Between Stages

```bash
# Set output from completed stage
loom stage output set <stage-id> api_port 8080

# Read output in dependent stage
loom stage output get <dependency-stage-id> api_port
```

---

## Memory & Knowledge Commands

### During Knowledge-Bootstrap Stages

```bash
# Initialize knowledge directory
loom knowledge init

# Run automated analysis
loom map --deep

# Check coverage
loom knowledge check

# Populate knowledge (append-only)
loom knowledge update architecture "## New Section\n\nContent here"

# Long content via heredoc
loom knowledge update patterns - <<'EOF'
## Authentication Pattern

Uses JWT with refresh token rotation.
Key files: src/auth/jwt.ts:15-80
EOF

# Record session insights
loom memory note "Found that auth uses middleware pattern"
loom memory decision "Using JWT over sessions" --context "Stateless scaling"
```

### During Implementation Stages

**ONLY use `loom memory` — NEVER `loom knowledge update`**

```bash
loom memory note "observation about code"
loom memory decision "chose approach X" --context "because Y"
loom memory question "why is Z done this way?"
loom memory change "src/foo.rs - Added bar() function"

# Review what you've recorded
loom memory list
```

### During Integration-Verify

```bash
# Read ALL stage memories for curation
loom memory show --all

# Curate valuable insights to permanent knowledge
loom knowledge update mistakes "## Lesson learned about X"
loom knowledge update patterns "## New pattern discovered"
loom knowledge update architecture "## Updated component relationships"
```

---

## The .work/ Directory

Understanding `.work/` structure is critical for debugging:

```text
.work/
├── config.toml              # Plan reference: source_path, base_branch, plan_id
├── stages/
│   └── NN-<stage-id>.md    # Stage state (YAML frontmatter + markdown body)
│                            # NN = topological depth (01, 02, etc.)
├── sessions/
│   └── <session-id>.md     # Session tracking: PID, stage, status, context %
├── signals/
│   └── <session-id>.md     # Agent task assignment (self-contained instructions)
├── handoffs/
│   └── <stage-id>-handoff-NNN.md  # Context dumps for session continuity
├── memory/
│   └── <stage-id>.md       # Stage-scoped memory journal
├── crashes/                 # Crash reports for failed sessions
├── pids/
│   └── <stage-id>.pid      # PID files for session tracking
├── wrappers/
│   └── <stage-id>-wrapper.sh  # Session launcher scripts
├── orchestrator.sock        # Unix socket for daemon IPC
├── orchestrator.pid         # Daemon PID
└── merge.lock               # Exclusive lock for progressive merges
```

**Key rules:**

- NEVER edit `.work/` files directly — use `loom` CLI commands
- Stage files use YAML frontmatter with status, timestamps, merged flag
- Signal files are self-contained — agents read ONLY their signal, not main repo
- `.work/` is gitignored and symlinked into each worktree

---

## Worktree Model

Each executing stage gets an isolated git worktree:

```text
PROJECT ROOT
├── .worktrees/
│   └── <stage-id>/              # Isolated copy of repo
│       ├── .work -> ../../.work # Symlink to shared state
│       ├── .claude/             # Worktree-specific hooks
│       ├── CLAUDE.md            # Project instructions
│       └── <project files>      # Full repo copy on loom/<stage-id> branch
├── .work/                       # Shared orchestration state
└── <main repo files>            # Main branch
```

**Path resolution:** `EXECUTION_PATH = worktree_root + working_dir`

If `working_dir: "loom"` and worktree is `.worktrees/my-stage/`, commands
execute from `.worktrees/my-stage/loom/`.

---

## Daemon Architecture

The daemon (`loom run`) is a background process that:

1. Listens on Unix socket (`.work/orchestrator.sock`)
2. Polls stage files every 5 seconds
3. Creates worktrees for ready stages
4. Spawns Claude Code sessions in terminal windows
5. Monitors session health via heartbeat files
6. Detects crashes via PID liveness checks
7. Auto-merges completed stages (progressive merge)
8. Handles retries with exponential backoff

**IPC commands:**

- `loom status` → reads from socket (or files if daemon down)
- `loom stop` → sends Stop message via socket
- `loom status --live` → subscribes to streaming updates

---

## Orchestration Decision Tree

Use this when deciding how to respond to loom state:

```text
Is the plan initialized?
├── NO → loom init doc/plans/PLAN-*.md
└── YES
    Is the daemon running?
    ├── NO → loom run
    └── YES
        Check loom status
        ├── All stages Completed → Done! loom clean (optional)
        ├── Stage Executing → Wait (or check context %)
        │   ├── Context < 60% → Healthy, wait
        │   ├── Context 60-75% → Watch closely
        │   └── Context > 75% → Expect handoff soon
        ├── Stage Blocked → Investigate
        │   ├── Read block reason from status --verbose
        │   ├── Fix underlying issue
        │   └── loom stage retry <id>
        ├── Stage MergeConflict → Resolve
        │   ├── loom stage retry-merge <id> (if transient)
        │   └── Manual resolution in worktree
        ├── Stage NeedsHandoff → Resume
        │   └── loom resume <id>
        ├── Stage CompletedWithFailures → Review
        │   ├── loom check <id> --suggest
        │   └── Fix and loom stage retry <id>
        ├── Stage WaitingForInput → Provide input
        │   └── loom stage resume <id>
        └── No stages ready, some WaitingForDeps → Wait for deps
```

---

## Full Workflow Example

Claude orchestrating a complete loom run:

```bash
# 1. Pre-flight
loom repair --fix

# 2. Initialize
loom init doc/plans/PLAN-add-auth.md

# 3. Start execution
loom run

# 4. Monitor (periodically)
loom status
loom graph

# 5. If a stage fails
loom status --verbose                    # See what failed
loom check failed-stage --suggest        # Get fix suggestions
loom stage retry failed-stage            # Retry after fixing

# 6. If merge conflict
loom stage retry-merge conflicted-stage  # Try auto-resolve

# 7. If context exhaustion
loom resume exhausted-stage              # Resume with handoff

# 8. When all complete
loom status                              # Verify DONE-PLAN-*
loom clean --worktrees                   # Clean up worktrees
```

---

## Recovery Playbook

### Corrupted .work/ State

```bash
loom repair --fix          # Try repair first
# If repair can't fix:
loom clean --state         # Remove .work/ only
loom init <plan-path>      # Re-initialize
loom run                   # Re-execute (completed work preserved in git)
```

### Orphaned Worktrees

```bash
loom worktree list         # See all worktrees
loom worktree remove <id>  # Remove specific worktree
loom clean --worktrees     # Remove all worktrees
```

### Stuck Daemon

```bash
loom stop                  # Graceful shutdown
# If that fails:
kill $(cat .work/orchestrator.pid)  # Force kill
rm .work/orchestrator.sock          # Clean socket
loom run                            # Restart
```

### All Else Fails

```bash
loom clean --all                         # Nuclear option
loom init doc/plans/PLAN-feature.md      # Fresh start
loom run                                 # Git branches preserve prior work
```

---

## Anti-Patterns

| Anti-Pattern | Why It's Wrong | Do This Instead |
| --- | --- | --- |
| Editing `.work/` files directly | Corrupts state machine | Use `loom` CLI commands |
| Using `target/debug/loom` | Version mismatch | Use `loom` from PATH |
| Running `loom run` on DONE-PLAN | Won't rename, confusing state | Clean and re-init first |
| Killing daemon with SIGKILL | Leaves orphaned worktrees | Use `loom stop` |
| Skipping `loom repair` before init | Missing hooks, stale state | Always run repair first |
| Force-completing without investigation | Masks real failures | Use `loom check --suggest` first |
| Ignoring stderr on exit 0 | Silent failures propagate | Always check stderr output |

---

## Quick Reference: Essential Commands

### Lifecycle

```bash
loom repair --fix                    # Pre-flight health check
loom init <plan>                     # Initialize from plan
loom run                             # Start daemon
loom status [--live|--verbose]       # Monitor
loom stop                            # Shutdown daemon
loom clean [--all|--worktrees]       # Cleanup
```

### Stage Management

```bash
loom stage retry <id>                # Retry failed stage
loom stage reset <id>                # Reset to queued
loom stage recover <id>              # Recovery from crash
loom stage complete <id>             # Mark done (runs acceptance)
loom stage retry-merge <id>          # Retry failed merge
loom stage merge-complete <id>       # After manual merge resolution
loom stage hold/release <id>         # Pause/unpause auto-execution
loom stage skip <id> --reason "..."  # Skip intentionally
```

### Debugging

```bash
loom status --verbose                # Detailed failure info
loom check <id> --suggest            # Goal-backward + fix suggestions
loom diagnose <id>                   # Spawn diagnostic session
loom stage verify <id> --dry-run     # Test acceptance without state change
loom graph                           # Visualize execution DAG
```

### Memory & Knowledge

```bash
loom memory note "..."               # Record insight
loom memory decision "..." --context "..."  # Record decision
loom memory list                     # Review entries
loom memory show --all               # All stage memories
loom knowledge check                 # Coverage report
loom knowledge update <file> "..."   # Append to knowledge
loom map --deep                      # Automated analysis
```
