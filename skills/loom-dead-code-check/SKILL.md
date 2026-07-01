---
name: loom-dead-code-check
description: Generate dead code detection configuration for loom plan verification across Rust, TypeScript, Python, Go, and JavaScript. Use when adding the dead_code_check field or dead-code acceptance criteria to a plan, or catching incomplete wiring where code exists but is never called.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - dead code
  - dead-code
  - dead_code_check
  - unused code
  - unused imports
  - unused functions
  - orphaned code
  - dead code detection
  - dead code check
  - code cleanup
  - unused variables
  - unreachable code
  - wiring verification
---

# Dead Code Detection

## Overview

Dead code — written but never called, imported, or used — is a direct signal of incomplete integration: a function nothing calls means the feature isn't wired up. In loom it serves two roles: **wiring verification** (catch implemented-but-unintegrated code) and **code quality** (cleanup). Most valuable in **integration-verify** stages as a final gate over all implementation stages.

> ⚠️ **`truths` is GONE** as a standalone field. Put dead-code checks in the first-class **`dead_code_check`** field (a goal-backward layer, run by `loom check`) or in **`acceptance`** (a build/lint command that exits non-zero on findings). A top-level `truths:` block is silently ignored and false-passes.

If dead code survives implementation it usually means: feature not wired (command unregistered, route unmounted), test code never run, refactor leftovers, or an incomplete implementation.

## The `dead_code_check` field (preferred, first-class)

`dead_code_check` is a goal-backward check evaluated by `loom check <stage-id>`. Schema:

```yaml
dead_code_check:
  command: "cargo build --message-format=short 2>&1"
  fail_patterns: ["warning: unused", "is never read", "never constructed"]
  ignore_patterns: ["generated.rs", "#[allow(dead_code)]"]
```

**Exactly how loom evaluates it** (`verify/goal_backward/dead_code.rs`):

1. Runs `command` in `working_dir`, **120 s timeout**, capturing stdout **and** stderr.
2. Scans the combined output **line by line**.
3. A line is a violation if it **contains** any `fail_pattern` (plain substring, **not regex**) AND contains **no** `ignore_pattern`.
4. Each violating line becomes one gap.

⚠ **The command's exit code is ignored** — only output text matters. So `cargo build` (exit 0 with warnings) works fine; you do NOT need `-D warnings`. This is the key difference from an `acceptance` command, which passes/fails on exit code.

⚠ `ignore_patterns` match the **whole output line**, so you can suppress by symbol name, file path, or an `#[allow(...)]` echo — whatever appears on the tool's line. Substring, so `old_helper` also ignores `old_helper_2`.

⚠ Choose `fail_patterns` that appear on the SAME line as the offending item. Tools that split a finding across lines (a header line + an indented location line) may put the symbol name on a different line than the keyword — test the real output first.

### `dead_code_check` vs `acceptance`

| | `dead_code_check` | `acceptance` |
| --- | --- | --- |
| Counts as goal-backward check | ✅ (satisfies `has_any_goal_checks`) | ❌ (separate requirement) |
| Pass/fail driver | output pattern match | command exit code |
| Surfaced by `loom check --suggest` | ✅ | ❌ |
| Needs `-D warnings` to fail | no (exit ignored) | yes |

Use `dead_code_check` when you want dead code counted as goal-backward proof; use `acceptance` when a tool already exits non-zero on findings and you want it in the build gate.

## Per-language tools and patterns

| Lang | Tool / command | fail_patterns (substrings) | Notes |
| ---- | -------------- | -------------------------- | ----- |
| Rust | `cargo build --message-format=short 2>&1` (or `cargo clippy`) | `warning: unused`, `is never read`, `never constructed`, `never used` | compiler is built in; no install. `-D dead_code` only needed for the `acceptance` (exit-code) form |
| TypeScript | `bunx ts-prune` (finds unused exports) | `used in module`, module-path lines | `--error` flag → non-zero exit (for `acceptance`). `bun add --dev ts-prune` |
| Python | `vulture src/ --min-confidence 80` | `unused function`, `unused class`, `unused import`, `unused variable`, `unreachable code` | `--min-confidence` (0-100): higher = fewer false positives. `uv add --dev vulture` |
| Go | `staticcheck ./...` | `is unused`, `U1000`, `U1001` | `go install honnef.co/go/tools/cmd/staticcheck@latest`. `U1000`=unused code, `U1001`=unused field |
| JS | `bunx unimported` (unused files + unresolved imports) | `unused file`, `unresolved import` | `bun add --dev unimported` |

⚠ **Tools ship NO deps in a fresh loom worktree** (`node_modules`, cargo tool binaries, `staticcheck`). Add install to `knowledge-bootstrap`, the sandbox `excluded_commands`, or accept the tool must be preinstalled — else the command errors and the check silently "passes" on empty output or fails opaquely. Rust's compiler-based check has no such dependency; prefer it.

### Rust example — both forms

```yaml
# Goal-backward form (exit code ignored, pattern-driven):
dead_code_check:
  command: "cargo build --message-format=short 2>&1"
  fail_patterns: ["warning: unused", "is never read", "never constructed"]
  ignore_patterns: []

# OR acceptance form (exit-code driven — needs -D):
acceptance:
  - "cargo clippy -- -D warnings"        # dead_code is a subset of clippy warnings
```

### Tool config keys (suppress false positives at the source)

```text
ts-prune   .tsprunerc          {"ignore": "index.ts|types.d.ts"}
vulture    pyproject.toml       [tool.vulture] min_confidence=80  paths=["src"]  ignore_names=["setUp","tearDown","test_*"]
staticcheck .staticcheck.conf   checks = ["all", "-ST1000"]
unimported .unimportedrc.json   {"entry":["src/index.js"], "extensions":[".js",".jsx"], "ignorePatterns":["**/*.config.js"]}
```

### Working directory

Dead-code commands run in `working_dir`, where the build manifest lives (`Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`). If `Cargo.toml` is at `loom/`, set `working_dir: "loom"` — otherwise `could not find Cargo.toml`. All paths in every field are relative to `working_dir`; never `../`.

## False positives — what each tool excludes and how to suppress

Real code that looks unused. Handle via the tool's own mechanism first, `ignore_patterns` second.

| Cause | Rust | TS/JS | Python | Go |
| ----- | ---- | ----- | ------ | -- |
| Entry points | `fn main()` auto-excluded | configure `entry` in tool config | mark `__all__` | exported `main`-pkg funcs excluded |
| Test code | `#[cfg(test)]` auto-excluded | exclude test dirs in config | ignore `test_*`/`setUp`/`tearDown` | `*_test.go` handled |
| Framework magic (derives/decorators) | `#[allow(dead_code)]` on the item | tool ignore for decorators | vulture respects `__all__` | — |
| Public API in a lib | `pub` items excluded by default | ignore lib entry in `.tsprunerc` | `__all__` | exported (capitalized) excluded |
| Feature-gated / build-tagged | run with `--features=all` | enable during check | enable during check | build tags to include variants |
| Reflection / dynamic load | document + integration test | — | — | assigned-to-var for reflection |

**Golden rule:** prefer a language-native suppression (`#[allow(dead_code)]`, `__all__`, tool config) that lives WITH the code over a broad `ignore_patterns` entry that can mask real regressions later.

## Combine with wiring for a reliable signal

Dead-code detection is a strong signal, not proof. The triple check catches integration holes reliably:

```yaml
stages:
  - id: integration-verify
    stage_type: integration-verify
    working_dir: "loom"
    acceptance:
      - "cargo test"
      - 'loom new-command --help'                 # functional: feature reachable
    dead_code_check:
      command: "cargo build --message-format=short 2>&1"
      fail_patterns: ["warning: unused", "never constructed"]
    wiring:
      - source: "src/cli/dispatch.rs"
        pattern: "Commands::NewCommand"           # CONSUMER: dispatch arm, not `mod new_command`
        description: "New command dispatched in CLI"
```

Dead-code says "code is used somewhere"; wiring says "used at the RIGHT place"; the functional command says "reachable by a user." A stub that's referenced only by its own unit test passes dead-code but fails the functional check.

## YAML gotchas

- ⛔ **Never put triple backticks inside a YAML `description`** — breaks parsing.
- Quote every command; default to YAML single quotes so nothing inside is special.
- For an `acceptance` negation, `! cmd | rg -q 'warning:'` passes when the pattern is absent (`!` inverts exit code). This is shell `!`, valid in `acceptance` — but `wiring` patterns treat `!` as a literal, not negation.
- Prefer `rg` over `grep` (cross-platform, no BSD/GNU `-P` differences).

## Placement

- **integration-verify** — primary home; catches orphans from every implementation stage.
- **Per implementation stage** — optional, for fast feedback; scope to the package touched (`cargo clippy -p auth -- -D warnings`).
- **knowledge-bootstrap** — install any external tool the plan's checks depend on.

## Checklist

- [ ] Dead-code checks use `dead_code_check` (goal-backward) or `acceptance` (exit-code) — NOT a removed `truths:` block
- [ ] `fail_patterns` are substrings that appear on the SAME output line as the finding (verified against real tool output)
- [ ] `dead_code_check` command doesn't rely on exit code (it's ignored); `acceptance` form uses `-D`/`--error` so it exits non-zero
- [ ] Any external tool (ts-prune, vulture, staticcheck, unimported) is installed in the worktree or bootstrap stage
- [ ] `working_dir` points at the build manifest; all paths relative to it, no `../`
- [ ] False positives suppressed with a language-native mechanism where possible, `ignore_patterns` only as fallback
- [ ] Paired with `wiring` (consumer site) + a functional command for reliable integration proof
- [ ] No triple backticks in any `description`
