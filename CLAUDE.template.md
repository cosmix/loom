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
2. `fd` and `grep` ONLY when part of a complex command pipeline that
   cannot be easily replaced. YOU CAN NEVER USE `find` or `grep`.

### 2. QUALITY GATES — MANDATORY BEFORE "DONE"

You are NOT done until ALL of these pass:

- ✅ Zero IDE diagnostics (errors AND warnings)
- ✅ You have written extensive tests covering ALL new and changed code AND THEY ALL PASS.
- ✅ Your tests are NOT one-off, here-doc-based tests. They are real test files, added to the codebase, using the project's framework.
- ✅ No linting errors
- ✅ You have thoroughly reviewed your work, both from a correctness AND
  a security standpoint and found nothing wrong! Follow project specific
  guidance, if available, to do so.

**SINGLE-PASS COMPLETION IS FORBIDDEN.** Run the verification loop. Actually check.

### 3. SUBAGENTS ARE BLIND — PASS CONTEXT AND RULES

⚠️ CRITICAL: When delegating ANY task to a subagent, you must INJECT the following text directly into their prompt as the FIRST few lines of their instructions. Do not summarize it. Paste it verbatim:

```markdown
** READ CLAUDE.md (both the user/global and project!) IMMEDIATELY AND FOLLOW ALL ITS RULES. **
```

then add your task description etc. This ensures subagents know the rules.

### 4. CONTEXT LIMIT — 75% = STOP -- ALWAYS

At 75% context: STOP. Write handoff to CLAUDE.md. Do NOT start new tasks.
Do NOT "finish quickly." Let the user know you are at context limit and need
to hand off.

### 5. SESSION STATE

UPDATE CLAUDE.md FREQUENTLY during work updating your session state and
progress. **DELETE THESE UPDATES** when task fully completes, REPLACING THEM
with a short summary of what was done.

### 6. MISTAKES AND LESSONS LEARNT

If you make a mistake, and the user points it out OR you discover it
yourself, you MUST IMMEDIATELY document in CLAUDE.md:

1. What the mistake was
2. What you should have done instead
3. How you fixed it

Keep your notes succinct as possible under a "MISTAKES AND
LESSONS LEARNT" section. NEVER delete content in this section. ALWAYS append
to it.

### 6. PLANS — LOCATION AND WORKFLOW

**Location:** NEVER USE `~/.claude/plans`. We use
`./doc/plans/PLAN-XXXX-description.md`. You CAN create the `doc/plans`
directory if it doesn't exist and you CAN create plan files there, even in
plan mode. This rule supersedes any previous/default behaviour.

#### NO TIME ESTIMATES

Loom uses multiple parallel sessions to implement plans. Do NOT provide estimates, they are meaningless.

#### STOP IMMEDIATELY AFTER PLANNING — DO NOT IMPLEMENT

When you write a plan to `doc/plans/`, your job is **DONE**. Do NOT proceed
to implementation. EVER, EVEN IF YOU THINK THE USER 'APPROVED' THE PLAN.

**The Planning Workflow:**

1. **Research** — Explore codebase, understand patterns, identify constraints
2. **Design** — Break work into stages with dependencies and acceptance criteria
3. **Write** — Save plan to `doc/plans/PLAN-XXXX-description.md`
4. **STOP** — Present plan to user and wait for approval

**What "done" looks like in planning mode:**

```text
✅ Plan written to doc/plans/PLAN-0042-new-feature.md
✅ Stages defined with clear dependencies
✅ Acceptance criteria specified for each stage
✅ Files/scope identified per stage

Ready for your review. When approved, run:
  loom init doc/plans/PLAN-0042-new-feature.md
  loom run
```

**BANNED after writing a plan:**

- Starting implementation of any stage
- Creating files described in the plan
- Modifying existing code per the plan
- "Let me just do the first stage quickly"
- "I'll implement the simple parts now"

**If the user asks you to implement:** Remind them to use `loom init` and
`loom run` instead. The orchestrator will spawn sessions for each stage.

### 7. DEPENDENCIES — PACKAGE MANAGERS ONLY

**NEVER** manually edit package.json, Cargo.toml, pyproject.toml, go.mod, etc.
**ALWAYS** use: `bun install`, `cargo add`, `uv add`, `go get` etc.

---

## Subagents and Skills

### Core Rules

1. **DELEGATE ALL WORK** to subagents. This is non-negotiable. You MUST NOT
   do implementation work yourself. Spawn multiple agents AT ONCE whenever
   possible, and DISTRIBUTE the work to them.
2. **Choose the RIGHT SKILL** for the job. NEVER use a generalist skill when
   a specialist skill exists.
3. **ALWAYS inject rules** into subagent prompts (see Rule 3 above).

### Parallelization Patterns

**PARALLEL: Similar changes across many files**

When making the same type of change to multiple files (e.g., adding a field,
updating imports, renaming a pattern):

1. Divide files into disjoint sets
2. Spawn one subagent per set
3. Each file belongs to EXACTLY ONE subagent

```text
Task: Add `created_at` field to 12 model files

Subagent 1: user.rs, profile.rs, session.rs, token.rs
Subagent 2: order.rs, payment.rs, invoice.rs, refund.rs
Subagent 3: product.rs, category.rs, inventory.rs, review.rs
```

**PARALLEL: Features in different modules**

When implementing independent features that touch different parts of the
codebase:

```text
Task: Add logging, caching, and rate limiting

Subagent 1: Logging → src/logging/*, tests/logging/*
Subagent 2: Caching → src/cache/*, tests/cache/*
Subagent 3: Rate limiting → src/ratelimit/*, tests/ratelimit/*
```

**PARALLEL: Independent acceptance criteria**

When verifying multiple unrelated checks:

```text
Subagent 1: Run unit tests
Subagent 2: Run integration tests
Subagent 3: Run linter
Subagent 4: Run type checker
```

### Serialization Patterns

**SERIAL: Shared file conflicts**

When two or more subagents would likely modify the SAME file, serialize:

```text
❌ BAD: Both touch src/lib.rs
  Subagent 1: Add module A → modifies src/lib.rs
  Subagent 2: Add module B → modifies src/lib.rs

✅ GOOD: Serialize
  Subagent 1: Add module A → modifies src/lib.rs
  (wait for completion)
  Subagent 2: Add module B → modifies src/lib.rs
```

**SERIAL: Dependency chains**

When output of one task is input to another:

```text
❌ BAD: Parallel with dependency
  Subagent 1: Create User struct
  Subagent 2: Implement UserService (needs User struct)

✅ GOOD: Serialize
  Subagent 1: Create User struct
  (wait for completion)
  Subagent 2: Implement UserService
```

**SERIAL: Shared state modifications**

When tasks modify shared configuration, schemas, or central registries:

```text
❌ BAD: Both modify Cargo.toml
  Subagent 1: Add serde dependency
  Subagent 2: Add tokio dependency

✅ GOOD: Single subagent or serialize
  Subagent 1: Add serde AND tokio dependencies
```

### Decision Flowchart

```text
Will subagents touch the same files?
├─ YES → SERIALIZE (or combine into one subagent)
└─ NO → Does task B depend on task A's output?
        ├─ YES → SERIALIZE
        └─ NO → PARALLELIZE
```

### Subagent Prompt Template

```text
** READ CLAUDE.md IMMEDIATELY AND FOLLOW ALL ITS RULES. **

## Your Assignment
[Describe the specific task]

## Files You Own (ONLY modify these)
- path/to/file1.rs
- path/to/file2.rs

## Files You May Read (but NOT modify)
- path/to/shared/types.rs

## Acceptance Criteria
- [ ] Specific criterion 1
- [ ] Specific criterion 2

## Constraints
- Do NOT modify files outside your ownership list
- Do NOT add dependencies without asking
```

## Code Quality

**Size Limits:** Files 400 lines | Functions 50 lines | Classes 300 lines —
IMPORTANT: REFACTOR if exceeded.

---

## Work Orchestration (loom)

This section enables self-propelling agents that survive context exhaustion
and crashes.

### Creating loom Plans

Plans define parallelizable work with dependencies. All stages, metadata, and
the execution graph are contained in a **single markdown file**.

**Location:** `doc/plans/PLAN-XXXX-description.md`

#### Plan File Structure

````markdown
# PLAN: [Descriptive Title]

## Overview

[What this plan accomplishes - 2-3 sentences]

## Current State

[Analysis of existing code, problems to solve]

## Proposed Changes

[Detailed description of the implementation approach]

## Stages

### Stage 1: [Name]

[Detailed description of what this stage accomplishes]

**Files:** `src/path/*.ext`

**Acceptance Criteria:**

- [ ] Tests pass
- [ ] No lint errors

### Stage 2: [Name]

[Description - depends on Stage 1]

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1-short-id
      name: "Stage 1 Name"
      description: "What this stage does"
      dependencies: []
      acceptance:
        - "pytest tests/test_stage1.py"
        - "ruff check src/"
      files:
        - "src/module/*.py"

    - id: stage-2-short-id
      name: "Stage 2 Name"
      description: "Builds on stage 1"
      dependencies: ["stage-1-short-id"]
      parallel_group: "core"
      acceptance:
        - "pytest tests/"
      files:
        - "src/other/*.py"
```

<!-- END loom METADATA -->
````

#### Stage Definition Schema

| Field            | Required | Description                                        |
| ---------------- | -------- | -------------------------------------------------- |
| `id`             | Yes      | Unique identifier (alphanumeric, dash, underscore) |
| `name`           | Yes      | Human-readable stage name                          |
| `description`    | No       | What this stage accomplishes                       |
| `dependencies`   | Yes      | Array of stage IDs that must complete first        |
| `parallel_group` | No       | Stages in same group can run simultaneously        |
| `acceptance`     | No       | Shell commands that must pass (exit 0)             |
| `files`          | No       | Glob patterns for files this stage modifies        |

**Note:** Use `dependencies: []` for stages with no dependencies.

#### Dependency Graph Rules

1. **No circular dependencies** - A cannot depend on B if B depends on A
2. **Stages without dependencies start immediately** - They become `Ready`
3. **Dependent stages wait** - Stay `Pending` until all deps are `Verified`
4. **Parallel groups** - Same group + satisfied deps = run together

#### Example: Parallel Execution

```yaml
loom:
  version: 1
  stages:
    # These two have no deps - run in PARALLEL
    - id: setup-database
      name: "Database Setup"
      dependencies: []
      parallel_group: "infrastructure"

    - id: setup-cache
      name: "Cache Setup"
      dependencies: []
      parallel_group: "infrastructure"

    # This waits for BOTH above to complete
    - id: integration
      name: "Integration Layer"
      dependencies: ["setup-database", "setup-cache"]
```

Execution flow:

```text
[setup-database] ──┬──► [integration]
[setup-cache]   ───┘
     └── parallel ──┘        └── sequential
```

#### Acceptance Criteria Best Practices

```yaml
acceptance:
  # Run specific tests
  - "pytest tests/test_feature.py -v"

  # Check imports work
  - "python -c 'from mymodule import NewClass'"

  # Linting
  - "ruff check src/feature/"

  # Type checking
  - "mypy src/feature/"

  # Build verification
  - "cargo build --release"
  - "cargo test --lib"
```

#### Workflow

1. **Create plan:** Write `doc/plans/PLAN-001-feature.md` with full structure
2. **Initialize:** `loom init doc/plans/PLAN-001-feature.md`
3. **Execute:** `loom run` (spawns parallel sessions for ready stages)
4. **Monitor:** `loom status` or `loom attach <stage-id>`
5. **Verify:** `loom verify <stage-id>` (runs acceptance criteria)
6. **Merge:** `loom merge <stage-id>` (merges completed stage to main)

### The Signal Principle

> **"If you have a signal, answer it."**

On session start:

1. Read `.work/structure.md` (code structure map) if it exists
2. Check `.work/signals/` for pending work matching your session ID
3. If signal exists → read it, load context files listed in "Context
   Restoration", execute immediately
4. If no signal → ask what to do

Signals are auto-generated from stage definitions and assigned to sessions
by the loom orchestrator.

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
   - Include: Goals, completed work, decisions made, file:line references,
     next steps
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

Each session is tied to a specific stage and works in an isolated git worktree
at `.worktrees/<stage-id>/`.

### file:line References (CRITICAL)

**ALWAYS** use specific references like `src/auth.ts:45-120` instead of vague descriptions.

Good: "Implement token refresh in `src/middleware/auth.ts:121+`"
Bad: "Continue working on the auth middleware"

This enables precise context restoration when resuming work.

### Code Structure Map

Maintain a living map of the codebase at `.work/structure.md` to eliminate
redundant exploration.

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

- **Branch**: loom/[stage-id]
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

### Stage Completion

When you complete work on your assigned stage:

1. Ensure all code is committed to your worktree branch
2. Run `loom stage complete <stage-id>` to mark completion
3. This runs acceptance criteria automatically
4. If criteria pass, stage is marked complete
5. If criteria fail, review errors and fix before retrying

**DO NOT wait for the Stop hook** - explicitly mark completion when done.

### Worktree Awareness

Each stage executes in an isolated git worktree:

- **Path**: `.worktrees/[stage-id]/`
- **Branch**: `loom/[stage-id]`
- The `.work/` directory is symlinked from the main repository

When working in a worktree:

1. All changes are isolated to your branch
2. Run `loom merge [stage-id]` to merge back to main
3. Merge conflicts are reported for manual resolution
4. Never manually switch branches - you're always on `loom/[stage-id]`

### Team Workflow References (Optional)

If your team has workflow documentation, reference it here:

- Feature development: [CONTRIBUTING.md or wiki link]
- Bug fixes: [process doc or link]
- Code review: [PR guidelines]

Note: External references (Notion, Linear, Confluence) may require MCP servers.
