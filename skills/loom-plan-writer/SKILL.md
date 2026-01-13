---
name: loom-plan-writer
description: Creates execution plans optimized for loom parallel orchestration. Designs DAG-based plans with subagents within stages and concurrent worktree stages. Trigger keywords: loom plan, create plan, write plan, execution plan, orchestration plan.
allowed-tools: Read, Grep, Glob, Write, Edit
---

# Loom Plan Writer

## Overview

This skill creates execution plans optimized for loom's parallel orchestration system. Plans maximize throughput through two levels of parallelism: subagents within stages (FIRST priority), and concurrent worktree stages (SECOND priority).

## Instructions

### 1. Output Location

**MANDATORY:** Write all plans to:

```
doc/plans/PLAN-<description>.md
```

**NEVER** write to `~/.claude/plans/` or any `.claude/plans` path.

### 2. Parallelization Strategy

Maximize parallel execution at TWO levels:

```
┌─────────────────────────────────────────────────────────────────────┐
│  PARALLELIZATION PRIORITY                                           │
│                                                                     │
│  1. SUBAGENTS FIRST  - Within a stage, use parallel subagents       │
│                        for tasks with NO file overlap               │
│                                                                     │
│  2. STAGES SECOND    - Separate stages for tasks that WILL touch    │
│                        the same files (loom merges branches)        │
└─────────────────────────────────────────────────────────────────────┘
```

| Files Overlap? | Solution                              |
| -------------- | ------------------------------------- |
| NO             | Same stage, parallel subagents        |
| YES            | Separate stages, loom merges later    |

### 3. Stage Description Requirement

**EVERY stage description MUST include this line:**

```
Use parallel subagents and skills to maximize performance.
```

This ensures Claude Code instances spawn concurrent subagents for independent tasks.

### 4. Plan Structure

Every plan MUST follow this structure:

```
┌─────────────────────────────────────────────────────────────────────┐
│  MANDATORY PLAN STRUCTURE                                           │
│                                                                     │
│  FIRST:  knowledge-bootstrap    (unless knowledge already exists)   │
│  MIDDLE: implementation stages  (parallelized where possible)       │
│  LAST:   integration-verify     (ALWAYS - no exceptions)            │
└─────────────────────────────────────────────────────────────────────┘
```

Include a visual execution diagram:

```
[knowledge-bootstrap] --> [stage-a, stage-b] --> [stage-c] --> [integration-verify]
```

Stages in `[a, b]` notation run concurrently.

### 5. Loom Metadata Format

Plans contain embedded YAML wrapped in HTML comments:

````markdown
<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-id           # Required: unique kebab-case identifier
      name: "Stage Name"     # Required: human-readable display name
      description: |         # Required: full task description for agent
        What this stage must accomplish.

        Use parallel subagents and skills to maximize performance.

        Tasks:
        - Subtask 1 with requirements
        - Subtask 2 with requirements
      dependencies: []       # Required: array of stage IDs this depends on
      parallel_group: "grp"  # Optional: concurrent execution grouping
      acceptance:            # Required: verification commands
        - "cargo test"
        - "cargo clippy -- -D warnings"
      files:                 # Optional: target file globs for scope
        - "src/**/*.rs"
```

<!-- END loom METADATA -->
````

**YAML Formatting Rules:**

| Rule                     | Correct                 | Incorrect             |
| ------------------------ | ----------------------- | --------------------- |
| Code fence               | 3 backticks             | 4 backticks           |
| Nested code blocks       | NEVER in descriptions   | Breaks YAML parser    |
| Examples in descriptions | Use plain indented text | Do NOT use ``` fences |

### 6. Knowledge Bootstrap Stage (First)

Captures codebase understanding before implementation:

```yaml
- id: knowledge-bootstrap
  name: "Bootstrap Knowledge Base"
  description: |
    Explore codebase hierarchically and populate .work/knowledge/:

    Use parallel subagents and skills to maximize performance.

    Exploration order:
    1. Top-level: entry points, main modules, directory layout
    2. Module boundaries: public interfaces, internal vs external
    3. Patterns: error handling, state management, data flow
    4. Conventions: naming, file structure, testing patterns

    Use loom knowledge update commands to capture findings.
  dependencies: []
  acceptance:
    - "grep -q '## ' .work/knowledge/entry-points.md"
    - "grep -q '## ' .work/knowledge/patterns.md"
    - "grep -q '## ' .work/knowledge/conventions.md"
  files:
    - ".work/knowledge/**"
```

**Skip ONLY if:** `.work/knowledge/` already populated or user explicitly states knowledge exists.

### 7. Integration Verify Stage (Last)

Verifies all work integrates correctly after merges:

```yaml
- id: integration-verify
  name: "Integration Verification"
  description: |
    Final integration verification - runs AFTER all feature stages complete.

    Use parallel subagents and skills to maximize performance.

    Tasks:
    1. Run full test suite (all tests, not just affected)
    2. Run linting with warnings as errors
    3. Verify build succeeds
    4. Check for unintended regressions
    5. Verify all acceptance criteria from previous stages still pass
  dependencies: ["stage-a", "stage-b", "stage-c"]  # ALL feature stages
  acceptance:
    - "cargo test"
    - "cargo clippy -- -D warnings"
    - "cargo build"
  files: []  # Verification only - no file modifications
```

**Why integration-verify is mandatory:**

| Reason                  | Explanation                                        |
| ----------------------- | -------------------------------------------------- |
| Isolated worktrees      | Feature stages test locally, not globally          |
| Merge conflicts         | Individual tests pass but merged code may conflict |
| Cross-stage regressions | Stage A change may break Stage B functionality     |
| Single verification     | One authoritative pass/fail for entire plan        |

### 8. After Writing Plan

1. Write plan to `doc/plans/PLAN-<name>.md`
2. **STOP** - Do NOT implement
3. Tell user:
   > Plan written to `doc/plans/PLAN-<name>.md`. Please review and run:
   > `loom init doc/plans/PLAN-<name>.md && loom run`
4. Wait for user feedback

**The plan file IS your deliverable.** Never proceed to implementation.

## Best Practices

1. **Subagents First**: Always maximize parallelism within stages before creating separate stages
2. **Explicit Dependencies**: Never create unnecessary sequential dependencies
3. **Clear File Scopes**: Define `files:` arrays to make overlap analysis explicit
4. **Actionable Descriptions**: Each description should be a complete task specification
5. **Testable Acceptance**: Every acceptance criterion must be a runnable command
6. **Bookend Compliance**: Always include knowledge-bootstrap first and integration-verify last

## Examples

### Example 1: Parallel Stages (No File Overlap)

```yaml
# Good - stages can run concurrently
stages:
  - id: add-auth
    dependencies: ["knowledge-bootstrap"]
    files: ["src/auth/**"]
  - id: add-logging
    dependencies: ["knowledge-bootstrap"]
    files: ["src/logging/**"]
  - id: integration-verify
    dependencies: ["add-auth", "add-logging"]
```

### Example 2: Sequential Stages (Same Files)

```yaml
# Both touch src/api/handler.rs - must be sequential
stages:
  - id: add-auth-to-handler
    dependencies: ["knowledge-bootstrap"]
    files: ["src/api/handler.rs"]
  - id: add-logging-to-handler
    dependencies: ["add-auth-to-handler"]  # Sequential
    files: ["src/api/handler.rs"]
  - id: integration-verify
    dependencies: ["add-logging-to-handler"]
```

### Example 3: Complete Plan Template

```markdown
# Plan: [Title]

## Overview

[2-3 sentence description]

## Execution Diagram

[knowledge-bootstrap] --> [stage-a, stage-b] --> [integration-verify]

<!-- loom METADATA -->

` ` `yaml
loom:
  version: 1
  stages:
    - id: knowledge-bootstrap
      name: "Bootstrap Knowledge Base"
      description: |
        Explore codebase and populate .work/knowledge/.

        Use parallel subagents and skills to maximize performance.

        Tasks:
        - Identify entry points and main modules
        - Document patterns and conventions
      dependencies: []
      acceptance:
        - "grep -q '## ' .work/knowledge/entry-points.md"
      files:
        - ".work/knowledge/**"

    - id: stage-a
      name: "Feature A"
      description: |
        Implement feature A.

        Use parallel subagents and skills to maximize performance.

        Tasks:
        - Task 1
        - Task 2
      dependencies: ["knowledge-bootstrap"]
      acceptance:
        - "cargo test"
      files:
        - "src/feature_a/**"

    - id: stage-b
      name: "Feature B"
      description: |
        Implement feature B.

        Use parallel subagents and skills to maximize performance.

        Tasks:
        - Task 1
        - Task 2
      dependencies: ["knowledge-bootstrap"]
      acceptance:
        - "cargo test"
      files:
        - "src/feature_b/**"

    - id: integration-verify
      name: "Integration Verification"
      description: |
        Final verification after all stages complete.

        Use parallel subagents and skills to maximize performance.

        Tasks:
        - Full test suite
        - Linting
        - Build verification
      dependencies: ["stage-a", "stage-b"]
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
        - "cargo build"
      files: []
` ` `

<!-- END loom METADATA -->
```
