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

### 3. QUALITY GATES

All must pass before completion:

- Zero IDE diagnostics and lint errors
- Tests written AND passing
- Self-reviewed for correctness and security

### 4. SUBAGENT INJECTION

First line of EVERY subagent prompt: `** READ CLAUDE.md IMMEDIATELY AND FOLLOW ALL ITS RULES. **`

### 5. CONTEXT @ 75% = STOP

At 75%: STOP. Write handoff to `.work/handoffs/`. No new tasks.

### 6. SESSION STATE

Update `CLAUDE.md` during work. On completion, replace updates with summary.

### 7. MISTAKES LOG

On any mistake: append to "MISTAKES AND LESSONS LEARNT" section. NEVER delete.

### 8. PLANS

- Location: `./doc/plans/PLAN-XXXX-description.md`
- Include execution diagram: `[a] --> [b,c] --> [d]`
- Loom plans: See `doc/templates/loom-plan.md` for YAML format
- **BANNED after plan:** Implementation. Tell user: `loom init <plan> && loom run`

### 9. DEPENDENCIES

**NEVER** hand-edit manifests. Use: `bun add`, `cargo add`, `uv add`, `go get`

### 10. CODE SIZE LIMITS

- File: 400 lines | Function: 50 lines | Class: 300 lines
- Exceed = REFACTOR immediately! DON'T WAIT.

### 11. COMMIT AND COMPLETE (HOOK-ENFORCED)

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

### 12. NO CLAUDE OR CLAUDE CODE ATTRIBUTION

**You are NOT allowed to mention Claude, Claude Code, or any other AI system, open source or proprietary, in any code, git commit mesages, documentation, comments or any other output. EVER.**

## DELEGATION

ALWAYS delegate implementation to subagents. Spawn multiple in PARALLEL when possible. PROVIDE DETAILED INSTRUCTIONS AND RULES!

**Parallelization:** Same files or dependent output? SERIAL. Otherwise? PARALLEL.

**Senior agents required for:** merge conflicts, debugging, architecture, algorithms.
See `doc/templates/subagent.md` for prompt templates.

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
- Templates: `doc/templates/` (handoff, signal, loom-plan, subagent)

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
