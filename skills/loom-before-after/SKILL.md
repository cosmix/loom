---
name: loom-before-after
description: Generates before/after verification pairs for loom plans. Proves a stage actually changed system behavior by capturing state before and after implementation. Use for delta-proof verification of new commands, endpoints, modules, or bug fixes.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - before after
  - before-after
  - before_stage
  - after_stage
  - delta proof
  - prove change
  - prove new
  - verify delta
  - state transition
  - before implementation
  - after implementation
---

# Before/After Verification

## Overview

A **delta-proof** proves a stage *caused* a change, not merely that the end state is valid. Capture what is true BEFORE implementation and what must be true AFTER; the pair distinguishes "my stage made it work" from "it already worked" (and "my fix resolved it" from "already fixed"). Without it, a green `acceptance` can pass on code that was already correct — proving nothing.

> ⚠️ **`truths` is GONE** as a standalone field. Behavioral "after state" commands now go in **`acceptance`** (Simple string, or Extended object for output matching). The explicit, automated delta-proof mechanism is the **`before_stage`** / **`after_stage`** fields — unchanged, still `Vec<TruthCheck>`. A top-level `truths:` block is silently ignored and false-passes.

## The delta pattern

| Case | BEFORE (pre-condition) | AFTER (post-condition) |
| ---- | ---------------------- | ---------------------- |
| New feature | reproducer FAILS (feature absent) | reproducer SUCCEEDS |
| Bug fix (inverted!) | reproducer SUCCEEDS (bug present) | reproducer FAILS (bug gone) |
| Behavior change | old behavior observed | new behavior observed |

For a bug fix the direction inverts: the reproducer "passing" means the bug is still there. `before_stage` with `exit_code: 1` on the reproducer, `after_stage` with `exit_code: 0`.

## `before_stage` / `after_stage` — the automated delta-proof (how loom runs them)

Both are `Vec<TruthCheck>`; the plan author writes them. Exact lifecycle:

- **`before_stage`** runs **after worktree creation, BEFORE the session spawns**. On a failed check → stage goes **Blocked**, session is **not** spawned (you asserted a pre-condition that didn't hold). Infrastructure errors are advisory (warn + continue). 30-s timeout per check.
- **`after_stage`** runs during **`loom stage complete`, AFTER `acceptance` passes**. On failure → stage **stays Executing**; the agent must fix and re-complete. 30-s timeout.

`TruthCheck` fields: `command` (required), `exit_code` (default 0), `stdout_contains`, `stdout_not_contains`, `stderr_empty`, `description`.

```yaml
before_stage:
  - command: "cargo test test_feature"
    exit_code: 1
    description: "Feature test fails before implementation"
after_stage:
  - command: "cargo test test_feature"
    exit_code: 0
    stdout_contains: ["test result: ok"]
    description: "Feature test passes after implementation"
```

⚠ **`before_stage` runs in a FRESH worktree before any agent work** — the reproducer must already exist in the base branch (a committed failing test, an existing endpoint), not one the stage is about to write. If the test file doesn't exist yet, `before_stage` errors (advisory) instead of proving the delta; prefer a command that exercises current behavior (`curl`, `--help`) over a not-yet-written test.

⚠ A negative `exit_code` assertion is brittle: `exit_code: 1` fails if the command dies with 2, 127 (not found), or 130 (interrupt). Pair with `stdout_contains`/`stdout_not_contains` to assert the RIGHT failure, not just any.

## When to use (and not)

Use delta-proof for: new features, bug fixes, behavior changes, new endpoints/commands, refactors that change behavior. **Skip** for: simple file existence (use `artifacts`), knowledge-only stages, pure quality gates (lint/format). A `description`-level BEFORE/AFTER note plus `artifacts` is enough for "add a config file" — reserve `before_stage`/`after_stage` for behavioral deltas.

## Capturing the AFTER state (`acceptance` + `wiring` + `artifacts`)

Beyond the automated pair, the standing verification of the after-state uses the normal fields:

- **`acceptance`** — the behavioral command that proves the feature works at runtime (the old `truths` commands live here now).
- **`wiring`** — proves the feature is CONNECTED. Target the consumer (call/mount/render/dispatch), never a declaration/export.
- **`artifacts`** — files exist with real implementation (non-empty, no stub text).

## Scenario templates

### New CLI command

```yaml
- id: add-check-command
  name: "Add loom check command"
  stage_type: standard
  working_dir: "loom"
  description: |
    Implement `loom check <stage-id>`.
    DELTA: BEFORE `loom check --help` fails (unregistered); AFTER it succeeds.
  acceptance:
    - "loom check --help"
    - "loom check nonexistent 2>&1 | rg -q 'not found'"
  wiring:
    - source: "src/cli/dispatch.rs"
      pattern: "Commands::Check"            # consumer: dispatch arm
      description: "Check command dispatched"
  artifacts:
    - "src/commands/verify.rs"
```

### New API endpoint

```yaml
- id: add-status-endpoint
  stage_type: standard
  working_dir: "."
  description: |
    GET /api/status returning health JSON.
    DELTA: BEFORE returns 404; AFTER returns 200 + JSON.
  before_stage:
    - command: "curl -sf localhost:8080/api/status"
      exit_code: 1
      description: "Endpoint absent before stage"
  after_stage:
    - command: "curl -sf localhost:8080/api/status"
      exit_code: 0
      stdout_contains: ["healthy"]
      description: "Endpoint returns health JSON"
  acceptance:
    - "curl -sf localhost:8080/api/status | jq -e '.healthy'"
  wiring:
    - source: "src/routes/mod.rs"
      pattern: "/api/status"
      description: "Status route registered"
  artifacts:
    - "src/handlers/status.rs"
```

### New module

```yaml
- id: add-retry-module
  stage_type: standard
  working_dir: "loom"
  description: |
    Create retry module (exponential backoff).
    DELTA: BEFORE `use crate::retry::RetryPolicy` won't compile; AFTER it does and is USED.
  acceptance:
    - "cargo test --lib retry"
  wiring:
    - source: "src/orchestrator/core/orchestrator.rs"
      pattern: "use crate::retry"           # a real consumer, not just `pub mod retry` in lib.rs
      description: "Retry used by orchestrator"
  artifacts:
    - "src/retry.rs"
```

### Bug fix (inverted delta)

```yaml
- id: fix-empty-plan-crash
  stage_type: standard
  working_dir: "loom"
  description: |
    Fix panic when initializing an empty plan.
    DELTA (inverted): BEFORE empty plan panics; AFTER it errors gracefully.
    The reproducer test PASSES only after the fix (it catches the graceful error).
  after_stage:
    - command: "cargo test test_empty_plan_no_crash"
      exit_code: 0
      description: "Empty plan handled without panic"
  acceptance:
    - "cargo test --lib plan::parser"
  wiring:
    - source: "src/plan/parser.rs"
      pattern: "is_empty"                    # guard added
      description: "Empty stage-list guard present"
  artifacts:
    - "tests/empty_plan_tests.rs"
```

## Pitfalls

1. **Testing too broadly.** `acceptance: ["cargo test"]` for "add register endpoint" proves nothing specific — assert the endpoint responds: `curl -sf -X POST .../api/register -d '{"email":"t@e.com"}' | jq -e '.user_id'`.
2. **Confusing test setup with the delta.** Before/after is about what YOUR STAGE changes ("feature X absent → works"), not test fixtures ("seed data → clean up").
3. **Bug-fix direction reversed.** BEFORE = bug present (reproducer succeeds/panics), AFTER = bug gone. Not the other way.
4. **Over-proving trivial additions.** A new config file → `artifacts: ["config.toml"]`, not a before/after pair.
5. **`before_stage` referencing not-yet-written code.** It runs in a fresh worktree before the agent works — the reproducer must already exist.
6. **Weak negative assertions.** `exit_code: 1` alone matches any failure; add `stdout_contains` to pin the intended one.

## Paths

All paths (`acceptance` cwd, `wiring.source`, `artifacts`) are relative to `working_dir`. `working_dir: "loom"` → `cargo test` runs in `loom/`, `src/x.rs` → `loom/src/x.rs`. Never `../`; never double-path (`loom/loom/...`).

## Checklist

- [ ] Delta is documented in the `description` (BEFORE / AFTER) for human reviewers
- [ ] Behavioral after-state commands are in `acceptance` (or `after_stage`), NOT a removed `truths:` block
- [ ] For a bug fix, the direction is inverted (before = bug present)
- [ ] `before_stage` reproducer exists in the base branch (not written by this stage)
- [ ] Negative `exit_code` assertions are pinned with `stdout_contains`/`stdout_not_contains`
- [ ] `wiring` targets a consumer site; `artifacts` are real implementation files
- [ ] All paths relative to `working_dir`; no `../`; no triple backticks in any `description`
