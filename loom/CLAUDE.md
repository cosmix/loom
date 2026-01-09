# Loom - Claude Code Orchestration

Loom orchestrates multiple Claude Code sessions to execute complex, multi-stage plans in parallel.

## Core Concepts

- **Plan**: A YAML-defined workflow with stages, dependencies, and acceptance criteria
- **Stage**: A unit of work that can be executed by a Claude Code session
- **Worktree**: An isolated git worktree created for each stage
- **Session**: A Claude Code instance working on a stage

## Auto-Merge Feature

When a stage completes, its worktree branch can be automatically merged to the target branch.

### Configuration

Auto-merge is **disabled by default** for safety. Enable it via:

#### 1. CLI Flag (per-run)
```bash
loom run --auto-merge
```

#### 2. Plan-Level (YAML)
```yaml
loom:
  version: 1
  auto_merge: true  # Enable for all stages
  stages:
    - id: stage-1
      name: "Setup"
```

#### 3. Per-Stage Override
```yaml
loom:
  version: 1
  auto_merge: false  # Default off
  stages:
    - id: stage-1
      name: "Safe Stage"
      auto_merge: true  # Enable just for this stage
    - id: stage-2
      name: "Review Required"
      # Uses plan default (false)
```

### Configuration Priority

When determining if auto-merge is enabled:

1. **Stage-level** `auto_merge` (highest priority)
2. **Plan-level** `auto_merge`
3. **CLI/Orchestrator** `--auto-merge` flag (lowest priority)

### Conflict Resolution

If merge conflicts occur:
1. Loom spawns a **merge resolution session**
2. The session receives a signal with conflict details
3. After resolution, cleanup happens automatically
4. If resolution fails, use `loom merge <stage>` to retry

## Merge States

In `loom status`, merge states appear as:

| Indicator | Meaning |
|-----------|---------|
| `[MERGING]` | Merge in progress |
| `[MERGED]` | Successfully merged |
| `[CONFLICT]` | Merge conflict detected |

## Commands

### Run with Auto-Merge
```bash
loom run --auto-merge
```

### Manual/Recovery Merge
```bash
loom merge <stage-id>
```
Use this to:
- Manually merge a completed stage
- Retry after a failed merge
- Restart a conflict resolution session

### Check Status
```bash
loom status
```
Shows stage status including merge state.

## Cleanup

After successful merge, loom automatically:
1. Removes the worktree directory (`.worktrees/<stage-id>/`)
2. Deletes the branch (`loom/<stage-id>`)
3. Runs `git worktree prune`

## Working in Loom Sessions

When working in a loom session:
- You're in an isolated worktree at `.worktrees/<stage-id>/`
- Your branch is `loom/<stage-id>`
- Complete work with: `loom stage complete <stage-id>`
- On completion, if auto-merge is enabled, your changes merge automatically
