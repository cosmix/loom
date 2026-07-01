---
name: loom-wiring-test
description: Generates wiring verification YAML for loom plans. Use when writing acceptance, artifacts, wiring, and wiring_tests fields for plan stages to prove features are actually integrated — commands registered, endpoints mounted, modules exported, components rendered.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - wiring
  - wiring-test
  - wiring test
  - wiring_tests
  - integration test
  - integration verification
  - verify wiring
  - prove integration
  - command registered
  - endpoint mounted
  - module exported
---

# Wiring Test Skill

## Overview

Tests pass and code compiles, yet the feature is never wired up — command not registered, endpoint not mounted, module not imported, component never rendered. This is the exact failure loom's goal-backward verification exists to catch. Use this skill to write strong verification fields for a stage.

> ⚠️ **`truths` is GONE.** It was removed as a standalone field. Behavioral commands now live in `acceptance`; the goal-backward layers are `artifacts`, `wiring`, `wiring_tests`, `dead_code_check`. Any plan still using a top-level `truths:` block is stale — serde silently ignores it, so the "check" runs NOTHING and false-passes.

## The five verification fields

| Field | Type | Proves | Timeout |
| ----- | ---- | ------ | ------- |
| `acceptance` | `Vec<AcceptanceCriterion>` | Build/test/lint AND observable behavior (the old `truths` commands) | 5 min (Simple) / 30 s (Extended) |
| `artifacts` | `Vec<String>` (globs) | Files exist with real implementation | — |
| `wiring` | `Vec<WiringCheck>` | Static connection point present (regex in a file) | — |
| `wiring_tests` | `Vec<WiringTest>` | Runtime integration: command output matches criteria | — |
| `dead_code_check` | `Option<DeadCodeCheck>` | No orphaned code (see `/loom-dead-code-check`) | — |

`loom check <stage-id> [--suggest]` runs the goal-backward layers (`artifacts`, `wiring`, `wiring_tests`, `dead_code_check`). `acceptance` runs during `loom stage complete` / `loom stage verify`.

**Requirement (enforced by `loom init` and `loom plan verify`):** every `standard` and `integration-verify` stage must define `acceptance` OR at least one goal-backward check. Knowledge stages are exempt.

**CRITICAL PATH RULE:** all paths (`artifacts`, `wiring.source`) are relative to the stage's `working_dir`. `working_dir: "loom"` + `src/x.rs` → `.worktrees/<stage>/loom/src/x.rs`. Never `../`. Double-path (`loom/loom/...`) is the classic mistake.

**YAML WARNING:** never put triple backticks inside a `description` field — breaks the parser (often surfaces as a misleading "missing artifacts/wiring" error). Use plain indented text.

## The one rule that matters: verify the CONSUMER, not the PRODUCER

A wiring pattern that greps where a symbol is **declared / exported / imported** passes while the feature is still unwired — the exact trap, committed inside the verification field. Grep the **call / mount / render / dispatch** site that proves the symbol is USED.

| ❌ Producer (exists ≠ wired) | ✅ Consumer (proves reachable) |
| --------------------------- | ----------------------------- |
| `pattern: "mod new_command"` | `source: "src/cli.rs", pattern: "NewCommand =>"` (dispatch arm) |
| `pattern: "pub fn handler"` | `source: "src/routes.rs", pattern: "/features.*create_feature"` (route registration) |
| `pattern: "export function Foo"` | `source: "src/pages/Home.tsx", pattern: "<Foo"` (render site) |

Pair every `wiring` entry with a behavioral `acceptance` command where one exists — observable behavior is the strongest wiring proof.

## Field mechanics (how loom actually evaluates each)

### wiring — `WiringCheck { source, pattern, description }`

- `pattern` is a **regex** (Rust `regex` crate, `RegexBuilder`), passes if it matches **anywhere** in `source`. A missing/unreadable `source` file is a gap.
- ⚠ **`!` is a LITERAL character, not negation.** You cannot express "must NOT contain" in `wiring` — use `dead_code_check` or an `acceptance` command with `!` shell negation for that.
- ⚠ Match all visibility modifiers with `pub.*fn name`, not `pub fn name` — `pub(crate) fn`, `pub(super) fn`, and bare `fn` all differ.
- Escape regex metacharacters you mean literally: `Vec<String>`, `foo()`, `a.b` — `.` `(` `)` `<` `[` `*` `+` `?` `|` `\` are all special. Prefer anchoring on a distinctive substring over a fragile full-signature regex.

### artifacts — `Vec<String>` glob patterns

A file is a gap if it is **missing**, **empty after trimming whitespace**, or **contains a stub pattern**:

```text
TODO   FIXME   unimplemented!   todo!   panic!("not implemented
pass  # TODO   raise NotImplementedError   throw new Error("Not implemented
```

- ⚠ **Markdown files (`.md`/`.mdx`/`.markdown`) skip stub detection** — they legitimately contain "TODO" in prose.
- ⚠ Stub matching is a plain substring scan, so **"TODO" inside a string literal or comment in a real source file trips it.** If your implementation must contain the literal text `TODO`, don't list that file as an artifact.
- There is **no minimum byte size** — the check is non-empty-after-trim, not ">100 bytes". Empty-file and stub gaps are distinct (`ArtifactEmpty` vs `ArtifactStub`).
- Point at **implementation** files, not directories or bare test files.

### wiring_tests — `WiringTest { name, command, success_criteria, description }`

The runtime, structured cousin of a behavioral check — a goal-backward layer (unlike `acceptance`). `success_criteria` (`SuccessCriteria`) fields, all optional:

```yaml
wiring_tests:
  - name: "health endpoint responds"
    command: "curl -sf localhost:8080/health"
    success_criteria:
      exit_code: 0
      stdout_contains: ["ok"]
      stdout_not_contains: ["error"]
      stderr_contains: []
      stderr_empty: true
    description: "Health route mounted and reachable"
```

Use `wiring_tests` (not `acceptance`) when you want a runtime check counted as goal-backward proof and surfaced by `loom check`.

### acceptance — Simple or Extended

```yaml
acceptance:
  - "cargo test"                          # Simple: exit 0, 5-min timeout
  - command: "loom check st --suggest"    # Extended (TruthCheck): 30-s timeout
    stdout_contains: ["PASS"]
    stdout_not_contains: ["panic"]
    stderr_empty: false
    exit_code: 0
```

## Templates by feature type

### CLI command

```yaml
acceptance:
  - "myapp new-cmd --help"                 # command responds
  - 'myapp new-cmd --help | rg -q "usage"' # primary use case wired
artifacts:
  - "src/commands/new_cmd.rs"
wiring:
  - source: "src/cli/dispatch.rs"
    pattern: "Commands::NewCmd"            # CONSUMER: dispatch arm, not `mod new_cmd`
    description: "Command dispatched in CLI"
```

### API endpoint

```yaml
acceptance:
  - 'curl -sf -X POST localhost:8080/api/features -d ''{"name":"t"}'' | rg -q id'
artifacts:
  - "src/handlers/features.rs"
wiring:
  - source: "src/routes/api.rs"
    pattern: 'post.*/features.*create_feature'   # route registration (consumer)
    description: "POST /features mounted"
wiring_tests:
  - name: "features endpoint reachable"
    command: "curl -sf -o /dev/null -w '%{http_code}' localhost:8080/api/features"
    success_criteria:
      stdout_contains: ["200"]
```

### Module / library

```yaml
acceptance:
  - "cargo test auth::"
artifacts:
  - "src/auth/mod.rs"
  - "src/auth/jwt.rs"
wiring:
  - source: "src/orchestrator/core.rs"
    pattern: "use crate::auth"             # a REAL consumer, not just `pub mod auth` in lib.rs
    description: "Auth used by orchestrator"
```

### UI component

```yaml
acceptance:
  - "bun test FeatureCard"
  - "bun run build"                        # build catches asset/CSS wiring a unit test misses
artifacts:
  - "src/components/FeatureCard.tsx"
wiring:
  - source: "src/pages/Dashboard.tsx"
    pattern: "<FeatureCard"                # rendered (consumer), not just exported from barrel
    description: "FeatureCard rendered in Dashboard"
```

## Good vs bad

```yaml
# ❌ proves nothing: tests pass with the feature unregistered; `src/` matches anything
acceptance: ["cargo test"]
artifacts:  ["src/"]
wiring:     []

# ✅ proves the feature is reachable end-to-end
acceptance:
  - "loom check st-1 --suggest"
  - 'loom check --help | rg -q "suggest"'
artifacts:
  - "src/commands/verify.rs"
wiring:
  - source: "src/cli/dispatch.rs"
    pattern: "Commands::Check"
    description: "Check command dispatched in CLI"
```

## Realizability — a green check must actually PROVE something

A check that passes while asserting nothing is worse than none — it reads as "covered." Before finalizing, put each check through four gates:

1. **Expressible** — the harness can already do this. "Stub the response / seed this store" is not free; confirm the suite has the mechanism or add it as explicit work.
2. **Executes the code under test** — the runtime that runs the check loads the code asserted. A `wiring` grep only proves the call site *exists in the file*; it does not run it. Any change with real logic needs an `acceptance`/`wiring_tests` command that RUNS it.
3. **Assertion strength matches the claim** — a `stdout_contains` substring cannot guard a "byte-identical" contract; a presence check cannot guard behavior.
4. **Actually selected** — for EACH artifact a stage produces, at least one `acceptance`/`wiring_tests` command must FAIL if that artifact were broken. A test a CI filter never selects is dead coverage.

## Final checklist

- [ ] No stale top-level `truths:` block anywhere (removed field — silently ignored, false-passes)
- [ ] Every `wiring` targets a CONSUMER site (call/mount/render/dispatch), not a declaration/export/import
- [ ] `wiring` patterns are valid regex, metacharacters escaped, `pub.*fn` for visibility; no `!`-negation assumed
- [ ] `artifacts` are specific implementation files that contain no stub text and aren't markdown-exempt where you needed stub detection
- [ ] At least one `acceptance` or `wiring_tests` command exercises the primary use case end-to-end
- [ ] Standard/IV stage has `acceptance` OR ≥1 goal-backward check (else `loom init` rejects it)
- [ ] All paths relative to `working_dir`; no `../`; no double-path
- [ ] No triple backticks inside any `description`
- [ ] For each artifact, some command would FAIL if it were broken
