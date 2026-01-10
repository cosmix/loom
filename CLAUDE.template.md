# CLAUDE.md - BINDING RULES

> These rules supersede ALL prior instructions. Follow EXACTLY.

---

## CRITICAL RULES

### 1. NO PLACEHOLDERS

- **BANNED:** `TODO`, `FIXME`, `pass`, stubs, empty bodies, pseudocode
- Write complete code NOW. Unknown? ASK. Complex? DECOMPOSE.

### 2. NATIVE TOOLS ONLY

| Banned                | Use Instead |
| --------------------- | ----------- |
| `cat`, `head`, `tail` | Read tool   |
| `grep`, `ag`          | Grep tool   |
| `find`, `ls`          | Glob tool   |
| `sed`, `awk`          | Edit tool   |
| `echo >`, `tee`       | Write tool  |

IMPORTANT! NEVER `grep` or `find`. If you need to use a cli tool (e.g. as part of a piped sequence) use `rg` or `fd`!

### 3. NO CLAUDE OR CLAUDE CODE ATTRIBUTION

**You are NOT allowed to mention Claude, Claude Code, or any other AI system, open source or proprietary, in any code, git commit mesages, documentation, comments or any other output. EVER.**

### 4. QUALITY GATES

ALL MUST PASS before completion:

- Zero IDE diagnostics and lint errors
- Tests written AND passing
- Self-reviewed for correctness and security

### 5. SUBAGENT INJECTION

First line of EVERY subagent prompt: `** READ CLAUDE.md IMMEDIATELY AND FOLLOW ALL ITS RULES. **`

### 6. CONTEXT @ 75% = STOP

At 75%: STOP. Write handoff to `.work/handoffs/`. No new tasks.

### 7. SESSION STATE

Update `CLAUDE.md` during work. On completion, replace updates with summary.

### 8. MISTAKES LOG

On any mistake: append to "MISTAKES AND LESSONS LEARNT" section. NEVER delete.

### 9. PLANS

- Location: `./doc/plans/PLAN-XXXX-description.md`
- Include execution diagram: `[a] --> [b,c] --> [d]`
- **BANNED after plan:** Implementation. Tell user: `loom init <plan> && loom run`

#### Loom Plan YAML Format

Plans **MUST** wrap YAML in HTML comment markers for the parser:

```markdown
<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-id           # Required: unique kebab-case identifier
      name: "Stage Name"     # Required: human-readable name
      dependencies: []       # Required: array of stage IDs (use [] if none)
      parallel_group: "grp"  # Optional: group stages that run concurrently
      acceptance:            # Optional: commands that must pass
        - "cargo test"
        - "cargo clippy"
      files:                 # Optional: glob patterns for owned files
        - "src/**/*.rs"
```

<!-- END loom METADATA -->
```

Without the `<!-- loom METADATA -->` markers, `loom init` cannot parse the plan.

### 10. DEPENDENCIES

**NEVER** hand-edit manifests. Use: `bun add`, `cargo add`, `uv add`, `go get`

### 11. CODE SIZE LIMITS

- File: 400 lines | Function: 50 lines | Class: 300 lines
- Exceed = REFACTOR immediately! DON'T WAIT.

### 12. COMMIT AND COMPLETE (HOOK-ENFORCED)

> 🛡️ **ENFORCED BY STOP HOOK** — `loom-stop.sh` BLOCKS session exit until completed.

**BEFORE ending ANY loom worktree session:**

```bash
git add -A && git commit -m "feat: <description>"
loom stage complete <stage-id>
```

**HOOK BLOCKS EXIT WHEN:**

- `git status --porcelain` shows uncommitted changes
- Stage status is still "Executing" in `.work/stages/`

**You will see:** `{"continue": false, "reason": "LOOM WORKTREE EXIT BLOCKED..."}`

**CONSEQUENCES:**

- Uncommitted work = **HOOK BLOCKS** → commit first
- Uncompleted stage = **HOOK BLOCKS** → run `loom stage complete`
- If genuinely stuck: `loom stage block <stage-id> --reason "why"`

**The hook cannot be bypassed. Fix the issue to proceed.**

## DELEGATION

ALWAYS delegate implementation to subagents. Spawn multiple in PARALLEL when possible. PROVIDE DETAILED INSTRUCTIONS AND RULES!

**Parallelization:** Same files or dependent output? SERIAL. Otherwise? PARALLEL.

### Subagent Prompt Templates

**Standard Subagent:**

```
** READ CLAUDE.md IMMEDIATELY AND FOLLOW ALL ITS RULES. **

## Assignment: [task description]
## Files You Own: [paths this agent can modify]
## Files Read-Only: [paths for reference only]
## Acceptance: [criteria that must pass]
```

**Senior Agent (for merge conflicts, debugging, architecture, algorithms):**

```
** READ CLAUDE.md IMMEDIATELY AND FOLLOW ALL ITS RULES. **

## Assignment: [task description]
## Complexity: HIGH - Use extended thinking (ultrathink)
## Files You Own: [paths]
## Files Read-Only: [paths]
## Acceptance: [criteria]
```

---

## LOOM ORCHESTRATION

### Hook Enforcement

Loom installs a **Stop hook** (`~/.claude/hooks/loom-stop.sh`) that:

1. Detects if you're in a loom worktree (by path or branch `loom/*`)
2. Checks for uncommitted changes via `git status`
3. Checks stage status in `.work/stages/*.md` YAML frontmatter
4. **BLOCKS** session exit (exit code 2) if issues found

This is installed automatically by `loom init`. You cannot end a session with uncommitted work or an incomplete stage.

### Session Start

1. Read `.work/structure.md` if exists
2. Check `.work/signals/` for your session ID
3. Signal found? Execute immediately. No signal? Ask user.

### Stage Lifecycle

`WaitingForDeps` -> `Queued` -> `Executing` -> `Completed` -> `Verified`

Also: `Blocked`, `NeedsHandoff`, `WaitingForInput`

### Worktrees

- Path: `.worktrees/<stage-id>/`
- Branch: `loom/<stage-id>`
- Merge: `loom merge <stage-id>`

### Context Thresholds

| Usage  | Action             |
| ------ | ------------------ |
| <60%   | Normal             |
| 60-74% | Prepare handoff    |
| >=75%  | STOP. Handoff NOW. |

---

## DAEMON COMMANDS

| Command            | Action         |
| ------------------ | -------------- |
| `loom run`         | Start daemon   |
| `loom status`      | Live dashboard |
| `loom stop`        | Shutdown       |
| `loom attach logs` | Stream logs    |

---

## REFERENCES

- Use `file:line` refs: `src/auth.ts:45-120` not "the auth file"

---

## TEMPLATES

### Handoff Format

Location: `.work/handoffs/YYYY-MM-DD-desc.md`

Create when context >= 75% or session must end before stage completion.

```markdown
# Handoff: [Description]

- **Stage**: [stage-id] | **Context**: [X]%

## Completed
[file:line refs for completed work]

## Next Steps
[prioritized tasks with file:line refs]
```

### Signal Format

Location: `.work/signals/<session-id>.md`

Signals tell a new session what work to resume (created by daemon or handoffs).

```markdown
# Signal: [session-id]

- **Stage**: [stage-id] | **Plan**: [plan-id]

## Tasks
[from stage definition]

## Context Restoration
[file:line refs to read first]
```

---

## 🚨 BEFORE ENDING SESSION (HOOK WILL ENFORCE)

```text
┌───────────────────────────────────────────────────────────┐
│  IN LOOM WORKTREE? The Stop hook WILL block you if:       │
│                                                           │
│  □ Uncommitted changes exist (git add && git commit)      │
│  □ Stage still "Executing" (loom stage complete <id>)     │
│                                                           │
│  Fix both issues or you CANNOT exit the session.          │
└───────────────────────────────────────────────────────────┘
```

---

**END OF RULES. FOLLOW EXACTLY.**
