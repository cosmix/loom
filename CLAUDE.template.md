# Claude Code Rules

## ⚠️ MANDATORY RULES

## 0. NO PLACEHOLDER CODE EVER

### Forbidden Patterns

- `// TODO` — **BANNED**
- `// FIXME` — **BANNED**
- `// implement later` — **BANNED**
- `// add logic here` — **BANNED**
- `pass` with no implementation — **BANNED**
- `return null` as a stub — **BANNED**
- `throw new Error("not implemented")` — **BANNED**
- Empty function bodies — **BANNED**
- Comments describing what code SHOULD do instead of ACTUAL CODE — **BANNED**
- Pseudocode instead of real code — **BANNED**
- Comments stating that 'in production code this would be implemented as X' — **BANNED**

### Required Behavior

- **IMPLEMENT THE ACTUAL CODE.** Not tomorrow. Not later. NOW.
- If you don't know how to implement something: **STOP AND ASK.** Do NOT stub it.
- If it's too complex: **BREAK IT DOWN.** Do NOT leave placeholders.
- Every function you write MUST BE COMPLETE AND WORKING.

### 1. NATIVE TOOLS — NOT CLI

**THESE COMMANDS ARE BANNED. DO NOT USE THEM:**

`cat` `head` `tail` `less` `more` → **Use Read tool**
`grep`,`ag` `ack` → **Use Grep tool**
`find` `ls`, `tree` → **Use Glob tool**
`sed` `awk` `perl -pe` → **Use Edit tool**
`echo >` `cat <<EOF` `printf >` `tee` → **Use Write tool**
`curl` `wget` → **Use WebFetch tool**
`git` → **You will NEVER use git, in any form!**

**ONLY EXCEPTIONS:**

1. actual build/runtime tools with no native equivalent.
2. `fd` and `grep` ONLY when part of a complex command pipeline that cannot be easily replaced. YOU CAN NEVER USE `find` or `grep`.

### 2. QUALITY GATES — MANDATORY BEFORE "DONE"

You are NOT done until ALL of these pass:

- ✅ Zero IDE diagnostics (errors AND warnings)
- ✅ All tests pass
- ✅ No linting errors
- ✅ You have thoroughly reviewed your work, both from a correctness AND a security standpoint and found nothing wrong! Follow project specific guidance, if available, to do so.

**SINGLE-PASS COMPLETION IS FORBIDDEN.** Run the verification loop. Actually check.

### 3. SUBAGENTS ARE BLIND — YOU **MUST** PASS CONTEXT AND RULES! THIS IS CRITICAL AND SUPERSEDES ANY PREVIOUS BEHAVIOR

Subagents DO NOT SEE BY DEFAULT:

- This CLAUDE.md file
- The project CLAUDE.md file
- Your conversation history
- Files you've read

**YOU MUST INCLUDE IN EVERY SUBAGENT PROMPT:**

1. ALL CLAUDE.md content. ALL OF IT!
2. Complete task context
3. Expected output format

### 4. CONTEXT LIMIT — 85% = STOP -- ALWAYS

At 85% context: STOP. Write handoff to CLAUDE.md. Do NOT start new tasks. Do NOT "finish quickly." Let the user know you are at context limit and need to hand off.

### 5. SESSION STATE

UPDATE CLAUDE.md FREQUENTLY during work updating your session state and progress. **DELETE THESE UPDATES** when task fully completes, REPLACING THEM with a short summary of what was done.

### 6. MISTAKES AND LESSONS LEARNT

If you make a mistake, and the user points it out OR you discover it yourself, you MUST IMMEDIATELY document:

1. What the mistake was
2. What you should have done instead
3. How you fixed it

Keep your notes succinct as possible in CLAUDE.md under a "MISTAKES AND LESSONS LEARNT" section. NEVER delete content in this section. ALWAYS append to it.

### 6. PLANS LOCATION

NEVER USE `~/.claude/plans`. We use `./doc/plans/PLAN-XXXX-description.md`. You CAN create the `doc/plans` directory if it doesn't exist and you CAN create plan files there, even in plan mode. This rule supersedes any previous/default behaviour you were following.

### 7. DEPENDENCIES — PACKAGE MANAGERS ONLY

**NEVER** manually edit package.json, Cargo.toml, pyproject.toml, go.mod, etc.
**ALWAYS** use: `npm install`, `cargo add`, `uv add`, `go get`

---

## Subagents and Skills

1. You MUST always DELEGATE ALL WORK to subagents. This is non-negotiable. You MUST NOT do any work yourself. Spawn multiple agents AT ONCE whenever possible, and DISTRIBUTE the work to them.
2. Choose the RIGHT SKILL for the job. NEVER use a generalist skill when a specialist skill exists.

## Code Quality

**Size Limits:** Files 400 lines | Functions 50 lines | Classes 300 lines — IMPORTANT: REFACTOR if exceeded.

---

## Work Orchestration (Flux)

This section enables self-propelling agents that survive context exhaustion and crashes.

### The Signal Principle

> **"If you have a signal, answer it."**

On session start:

1. Read `.work/structure.md` (code structure map) if it exists
2. Check `.work/signals/` for pending work matching your session ID
3. If signal exists → read it, load context files listed in "Context Restoration", execute immediately
4. If no signal → ask what to do

Signals are auto-generated from stage definitions and assigned to sessions by the Flux orchestrator.

### The Clear > Compact Principle

> **"Don't fight lossy compression. Externalize state and start fresh."**

At 75% context usage (Red zone):

1. Create handoff in `.work/handoffs/` with structured format (see below)
2. Update stage status to `NeedsHandoff` in `.work/stages/<stage-id>.md`
3. The orchestrator will spawn a new session to continue your work
4. Clear context (NOT compact)
5. Fresh session loads signal + handoff

**Context Thresholds:**

| Level  | Usage  | Action                                |
| ------ | ------ | ------------------------------------- |
| Green  | < 60%  | Normal operation                      |
| Yellow | 60-74% | Warning - consider handoff soon       |
| Red    | ≥ 75%  | Critical - create handoff immediately |

### Before Ending ANY Session

1. Update session status in `.work/sessions/<session-id>.md`
2. If work remains:
   - Write handoff to `.work/handoffs/YYYY-MM-DD-description.md`
   - Include: Goals, completed work, decisions made, file:line references, next steps
   - Update stage status accordingly (NeedsHandoff, Completed, or Blocked)
3. If blocked:
   - Document blocker in stage file `.work/stages/<stage-id>.md`
   - Update stage status to `Blocked` with blocker details

### Self-Identification Mechanism

When you start a session:

1. Scan `.work/signals/` for files with pending work
2. Match session ID from signal filename to identify your assigned work
3. One match → you ARE that session, execute the signal
4. Multiple matches → ask user which session to assume
5. No matches → ask user (waiting for stage assignment?)

Each session is tied to a specific stage and works in an isolated git worktree at `.worktrees/<stage-id>/`.

### file:line References (CRITICAL)

**ALWAYS** use specific references like `src/auth.ts:45-120` instead of vague descriptions.

Good: "Implement token refresh in `src/middleware/auth.ts:121+`"
Bad: "Continue working on the auth middleware"

This enables precise context restoration when resuming work.

### Code Structure Map

Maintain a living map of the codebase at `.work/structure.md` to eliminate redundant exploration.

**When to update:**

- After creating new modules/files
- After significant refactors
- When starting work on an unfamiliar area (add what you learn)

**What it should contain:**

```markdown
# Code Structure Map

Last updated: YYYY-MM-DD

## Directory Overview

src/
├── commands/ # CLI command handlers
├── models/ # Data structures and types
├── services/ # Business logic
└── utils/ # Shared utilities

## Key Modules

### src/commands/

| File     | Purpose         | Key exports    |
| -------- | --------------- | -------------- |
| mod.rs   | Command routing | `execute()`    |
| build.rs | Build command   | `BuildCommand` |

### src/models/

| File      | Purpose             | Key exports          |
| --------- | ------------------- | -------------------- |
| config.rs | Configuration types | `Config`, `Settings` |

## Entry Points

- **CLI**: `src/main.rs` → `src/commands/mod.rs`
- **Library**: `src/lib.rs`

## Dependencies Between Modules

- `commands/*` → `services/*` → `models/*`
- `utils/*` is standalone, imported by all

## Conventions

- Error handling: [pattern used]
- Async: [tokio/async-std/sync]
- Testing: [unit tests location, integration tests location]
```

**On session start:** Read `.work/structure.md` BEFORE exploring the codebase.
Only explore areas not documented or outdated.

### Handoff File Format

When creating handoffs, use this structure:

```markdown
# Handoff: [Brief Description]

## Metadata

- **Date**: YYYY-MM-DD
- **From**: [session-id]
- **To**: (next session)
- **Stage**: [stage-id]
- **Plan**: [plan-id]
- **Context**: [X]% (approaching threshold)

## Goals (What We're Building)

[1-2 sentences describing the overall goal from stage]

## Completed Work

- [Specific accomplishment with file:line ref]
- [Another accomplishment]

## Key Decisions Made

| Decision | Rationale |
| -------- | --------- |
| [Choice] | [Why]     |

## Current State

- **Branch**: flux/[stage-id]
- **Worktree**: .worktrees/[stage-id]
- **Tests**: [status]
- **Files Modified**: [list with paths]

## Next Steps (Prioritized)

1. [Most important task] in `path/file.ext:line+`
2. [Second task]
3. [Third task]

## Learnings / Patterns Identified

- [Useful insight for future work]
```

### Signal File Format

Signals are auto-generated from stage definitions and assigned to sessions. Format:

```markdown
# Signal: [session-id]

## Target

- **Session**: [session-id]
- **Stage**: [stage-id]
- **Plan**: [plan-id]

## Assignment

[stage-name]: [stage-description]

## Immediate Tasks

1. [Task from stage definition]
2. [Second task]
3. [Third task]

### Context Restoration (file:line references)

- `.work/stages/[stage-id].md` - Stage definition
- `.work/handoffs/[date]-[desc].md` - Previous handoff (if resuming)
- `src/path/file.ext:line-range` - Relevant code from stage

### Acceptance Criteria

- [ ] [Criterion from stage acceptance list]
- [ ] [Second criterion]
```

### Stage Lifecycle

Stages transition through these states:

- **Pending**: Dependencies not yet satisfied
- **Ready**: Dependencies satisfied, waiting for execution
- **Executing**: Session actively working on stage
- **Completed**: Work done, awaiting verification
- **Verified**: Human approved, can trigger dependents
- **Blocked**: Encountered issue, needs intervention
- **NeedsHandoff**: Context exhausted, needs continuation

When completing work on a stage:

1. Ensure all acceptance criteria pass
2. Update stage status to `Completed`
3. Create handoff if context > 75%
4. The orchestrator will handle verification and dependent stage triggering

### Worktree Awareness

Each stage executes in an isolated git worktree:

- **Path**: `.worktrees/[stage-id]/`
- **Branch**: `flux/[stage-id]`
- The `.work/` directory is symlinked from the main repository

When working in a worktree:

1. All changes are isolated to your branch
2. Run `flux merge [stage-id]` to merge back to main
3. Merge conflicts are reported for manual resolution
4. Never manually switch branches - you're always on `flux/[stage-id]`

### Team Workflow References (Optional)

If your team has workflow documentation, reference it here:

- Feature development: [CONTRIBUTING.md or wiki link]
- Bug fixes: [process doc or link]
- Code review: [PR guidelines]

Note: External references (Notion, Linear, Confluence) may require MCP servers.
