# Plan: Add `loom plan verify` Command

## Overview

Add a new top-level `loom plan` command with a `verify` subcommand that validates a plan file without any side effects. Today the only way to validate a plan is to run `loom init <path>`, which requires a git repo, creates `.work/`, writes `config.toml`, and installs hooks — far too heavy for iterative plan drafting. `loom plan verify <path>` will reuse the existing validation primitives (`parse_plan`, `validate`, `validate_structural_preflight`, `check_knowledge_recommendations`, `check_sandbox_recommendations`, `ExecutionGraph::build`) but invoke them read-only and report findings in either human or JSON format.

This unblocks tighter feedback loops for plan authors (including `/loom-plan-writer` itself, which currently has no cheap way to syntax-check what it produced) and makes plans CI-checkable.

## Goals

- Provide a side-effect-free way to validate a plan from any working directory.
- Reuse 100% of existing validation logic — this is a new caller, not new validation rules.
- Surface schema errors (fatal) and structural/knowledge/sandbox warnings (non-fatal) clearly in both human and JSON output.
- Build the execution DAG to catch cycles the schema validator misses.
- Add a `--strict` flag that promotes warnings to errors for CI/pre-commit use.
- Establish a `loom plan` namespace for future plan-related subcommands (`show`, `list`, `graph`, etc.) without committing to them now.

Non-goals: no new validation rules; no new metadata fields; no `loom plan show/list/graph` (future); no multi-path/glob support (future).

## Context Notes

- Validation logic already lives in `loom/src/plan/schema/validation.rs`. The path `parse_plan -> parse_plan_content -> parse_and_validate` collapses `Vec<ValidationError>` into one anyhow message — too lossy for our needs. We must extract YAML and call `crate::plan::schema::validate` directly so the structured `Vec<ValidationError>` is in hand. Reusable helpers (all `pub use` from `crate::plan::schema`): `validate`, `validate_structural_preflight`, `check_knowledge_recommendations`, `check_sandbox_recommendations`, plus types `LoomMetadata`, `StageDefinition`, `StageType`, `ValidationError`.
- `crate::plan::parser` re-exports `extract_plan_name` and `extract_yaml_metadata` (`parser/mod.rs:10`) — both pub, both safe to call directly.
- `ExecutionGraph::build` (`loom/src/plan/graph/mod.rs:36`) detects cycles via DFS — schema validation does NOT catch cycles (only self-deps and missing-dep references). Note: schema validation runs first, so cycle test fixtures must define enough acceptance/goal-backward fields to pass schema validation before the cycle is reachable.
- `compute_all_levels` (`loom/src/plan/graph/levels.rs:68`) returns `HashMap<String, usize>` mapping stage id → topological depth. To group by level for display, invert: collect into `BTreeMap<usize, Vec<&StageDefinition>>` and iterate sorted.
- `loom graph` (`loom/src/commands/graph/mod.rs:35`) reads from `.work/` (runtime state). Reuse `colors::stage_color` (already `pub use`) but NOT the tree display — we operate on a plan file, not stages on disk, and must not touch `.work`.
- `detect_stage_type` in `loom/src/commands/init/plan_setup.rs:208` applies heuristic name-matching when `stage_type` is the default `Standard`. **A user's `stage: knowledge-bootstrap` with default stage_type would print as "standard" if we use the raw field.** To match what `loom init` does at runtime, extract `detect_stage_type` to `plan/schema/` and reuse it.
- CLI types: `loom/src/cli/types.rs` (`Commands` enum) — currently does NOT import `PathBuf`. Nested subcommands require `#[derive(Subcommand)]` on the child enum and `#[command(subcommand)]` on the parent variant (see `Commands::Stage` and `StageCommands` in `cli/types_stage.rs` for the pattern). Dispatch in `cli/dispatch.rs`. Commands modules registered in `commands/mod.rs`.
- Dynamic shell completion definitions live in `loom/src/completions/dynamic/commands.rs`; the per-subcommand route lives in `mod.rs::complete_after_subcommand`. Generated zsh scripts (`completions/scripts/zsh.rs`) are thin wrappers around `loom complete` — testing them via `rg -q "_loom__plan__verify"` proves nothing. Verify the dynamic backend via `./target/debug/loom complete zsh "$PWD" "loom " "" "loom"` instead.
- Existing integration tests use `std::process::Command` (not `assert_cmd`, which is not in `loom/Cargo.toml`). See `loom/tests/integration/dependency_simple.rs:5` for the pattern. Match that to avoid adding a new dev-dep.
- `loom/src/plan/schema/tests/` already contains `validation_tests.rs`, `knowledge_recommendations_tests.rs`, etc. — extend there for unit tests; add binary-level tests in `loom/tests/integration/`.
- **Plan files are TRUSTED input** in this project (checked in, authored by humans/Claude under review). The new command will not sanitize ANSI escape sequences in plan names or stage names — `--no-color` only disables our own `colored` output, not arbitrary control characters embedded in plan text. This matches the existing trust model for acceptance criteria, which already execute as shell commands. Document this in the command's `long_about`.

## Execution Diagram

```mermaid
graph LR
    knowledge-bootstrap --> implement-plan-verify
    implement-plan-verify --> integration-verify
    integration-verify --> knowledge-distill
```

---

## Stages

### 1. Knowledge Bootstrap

**Purpose:** Confirm existing knowledge covers plan parsing/validation, the CLI registration pattern, and the dispatch flow. If any of those are stale, refresh them.

**Tasks:**

- Run `loom knowledge check`. If coverage ≥ 50% AND the entries listed below already exist, record `loom memory note "knowledge sufficient — skipping deep refresh"` and proceed.
- Confirm `entry-points.md` lists `commands/verify.rs`, `plan/parser/mod.rs`, `plan/schema/validation.rs`, `plan/graph/mod.rs` and the "CLI Subcommand Registration Pattern" three-file checklist.
- Confirm `patterns.md` covers the AcceptanceCriterion untagged enum, signal generation, file-based state. If something is missing for this feature's surface area, fill it.
- Record any surprises about how `loom init` currently flows soft warnings vs hard errors in memory.

**Files:** `doc/loom/knowledge/**`

**Acceptance:** `loom knowledge check --min-coverage 50` passes; the knowledge files already contain `##` sections (they do).

---

### 2. Implement `loom plan verify`

**Purpose:** Add the new command end-to-end: clap subcommand, dispatch, command implementation, dynamic completions, tests.

**Dependencies:** knowledge-bootstrap

**Tasks (single agent, no subagents — small surface area):**

_CLI plumbing:_

- In `loom/src/cli/types.rs`:
  - Add `use std::path::PathBuf;` near the top (it's not currently imported).
  - Add a new variant `Plan { #[command(subcommand)] command: PlanCommands },` to the `Commands` enum, grouped near `Check`. The `#[command(subcommand)]` attribute is REQUIRED — see `Commands::Stage`/`StageCommands` in `cli/types_stage.rs` for the working pattern.
  - Define `#[derive(Subcommand)] pub enum PlanCommands { Verify { ... } }` with fields:
    - `#[arg(value_parser = clap::value_parser!(PathBuf))] path: PathBuf` (positional, required)
    - `#[arg(long)] strict: bool`
    - `#[arg(long)] json: bool`
    - `#[arg(long)] no_color: bool`
  - The `Verify` variant must have `#[command(about = "Verify a plan file without side effects", long_about = ...)]` — the integration test asserts `--help` stdout contains `"Verify"`.
- In `loom/src/cli/dispatch.rs`:
  - Add `plan` to the `use loom::commands::{...}` import.
  - Add `PlanCommands` to the `use super::types::{...}` import.
  - Add the dispatch arm: `Commands::Plan { command } => match command { PlanCommands::Verify { path, strict, json, no_color } => plan::verify::execute(&path, strict, json, no_color), }`.

_Command implementation:_

- Register module in `loom/src/commands/mod.rs`: `pub mod plan;`.
- Create `loom/src/commands/plan/mod.rs` with `pub mod verify;`.
- Create `loom/src/commands/plan/verify.rs` (~150 LOC) with `pub fn execute(path: &Path, strict: bool, json: bool, no_color: bool) -> Result<()>`.

_Behavior of `execute`:_

1. Resolve `path`:
   - Reject if missing or not a file.
   - Canonicalize via `path.canonicalize()` for filesystem operations (read, repo-root walk). Preserve the ORIGINAL user-supplied path for display and JSON `source`.
   - Enforce a max file size of 1 MiB before reading. `std::fs::metadata(&canonical)?.len() > 1_048_576` → hard error "Plan file too large (N bytes; limit 1 MiB)".
2. Read file contents via `std::fs::read_to_string(&canonical)`.
3. Extract structured errors by mirroring `parse_plan_content`'s body locally (do NOT call `parse_plan_content` — it collapses `Vec<ValidationError>` into a single anyhow message):
   - `let yaml = crate::plan::parser::extract_yaml_metadata(&content)?;`
   - `let metadata: crate::plan::schema::LoomMetadata = serde_yaml::from_str(&yaml).map_err(...)?;`
   - `let plan_name = crate::plan::parser::extract_plan_name(&content).ok();` (Option — H1 may be missing, treat as non-fatal for display)
   - `let validation_result = crate::plan::schema::validate(&metadata);` — `Result<(), Vec<ValidationError>>`. Capture them.
4. Resolve `repo_root_opt: Option<PathBuf>` for the preflight: walk up from the canonical plan path's parent looking for a `.git` entry (file OR directory — `.git` is a file in worktrees). Stop at filesystem root. `Some(dir)` if found, `None` otherwise. **Do NOT use cwd or `git rev-parse`** — the plan may live in a different repo than the caller's cwd; build-tool config-file checks must resolve relative to the PLAN's repo.
5. If `validation_result.is_ok()`, run the soft checks (skip on schema failure — no point chasing warnings on a plan that won't parse):
   - `validate_structural_preflight(&metadata.loom.stages, repo_root_opt.as_deref())`
   - `check_knowledge_recommendations(&metadata.loom.stages)`
   - `check_sandbox_recommendations(&metadata)`
6. Build the DAG: `crate::plan::graph::ExecutionGraph::build(metadata.loom.stages.clone())`. Capture any error as a hard error (cycles, malformed graph). Skip this step on schema failure.
7. Compute stage levels: `let depths = crate::plan::graph::levels::compute_all_levels(&metadata.loom.stages, |s| s.id.as_str(), |s| &s.dependencies);` → `HashMap<String, usize>`. Invert into `BTreeMap<usize, Vec<&StageDefinition>>` keyed by depth for ordered display.
8. Resolve displayed stage_type per stage: extract `detect_stage_type` from `loom/src/commands/init/plan_setup.rs:208` into `loom/src/plan/schema/` (e.g., `schema/detect.rs`) as `pub fn detect_stage_type(stage_def: &StageDefinition) -> StageType` and re-export from `schema/mod.rs`. Update `plan_setup.rs` to call the relocated function (no behavior change). In `verify::execute`, call it per stage so the displayed/JSON-emitted type matches what `loom init` will actually use at runtime.
9. Render results:
   - **Human (default):** Header with plan id/name/source path. Errors section (red ✗, indented). Warnings section (yellow ⚠, indented, grouped: structural, knowledge, sandbox). "Stages by Level" section showing topological levels with each stage's id, name, displayed stage_type (from `detect_stage_type`), and dependencies. Summary line: `N error(s), M warning(s)`. Respect `--no-color` by disabling `colored::control::set_override(false)` at the top of `execute`.
   - **JSON (`--json`):** Single object with shape:

     ```text
     {
       "plan": {"id": "<id>"|null, "name": "<name>"|null, "source": "<original input path>"},
       "valid": true|false,
       "errors": [{"stage_id": null|"<id>", "message": "..."}],
       "warnings": {
         "structural": ["..."],
         "knowledge": ["..."],
         "sandbox": ["..."]
       },
       "levels": [[{"id": "...", "name": "...", "stage_type": "<detected>", "dependencies": [...]}]]
     }
     ```

     Use serde-derived structs (define at the top of `verify.rs`) so the schema is documented in code. `plan.id` and `plan.name` are `Option<String>`. The output struct must `#[derive(Serialize)]`. When `--json` is set, suppress ALL human output.

10. Exit code policy — **JSON mode owns every error path**:
    - **Hard errors** (missing path, not-a-file, oversized file, file-read error, YAML extraction failure, YAML deserialization failure, schema validation failure, DAG cycle):
      - In **JSON mode**: emit the JSON envelope (with `null` for `plan.id`/`plan.name` when not yet parsed, and an `errors` array describing the failure), `println!`, `std::io::stdout().flush()?`, then `std::process::exit(1)`. **Never return Err in JSON mode** — anyhow's stderr message would pollute the JSON-only contract that tooling depends on.
      - In **human mode**: print details to stdout, then `bail!()` with a one-line summary. anyhow's short stderr line is acceptable for humans.
    - **No hard errors**:
      - `should_fail = errors > 0 || (strict && warnings > 0)` — pure function, unit-tested inline with all four branches.
      - JSON mode: print envelope, flush, `process::exit(if should_fail { 1 } else { 0 })`.
      - Human mode: print sections, then `if should_fail { bail!(...) } else { Ok(()) }`.

_Dynamic completions:_ See "Dynamic completions — four concrete edits" block under YAML metadata implementation checklist for exact instructions.

_Tests (in same stage):_

- Add `loom/tests/integration/plan_verify.rs` and register `pub mod plan_verify;` in `loom/tests/integration/mod.rs`. The wiring check for the test module is one of the goal-backward `wiring` entries — without registration the test target compiles to zero tests for this file.
- Use `std::process::Command` (NOT `assert_cmd` — not a dev-dep in `loom/Cargo.toml`). Pattern: see `loom/tests/integration/dependency_simple.rs:5`. Set the binary via `env!("CARGO_BIN_EXE_loom")` or build via `cargo build` and locate `target/debug/loom`. Most existing tests use `Command::new` against the project-built binary; match that.
- Test scenarios:
  1. Valid plan in a temp dir → exit 0, stdout contains "0 error(s)" and the plan name.
  2. Plan with schema error (bad version) → exit 1, stderr/stdout mentions "Unsupported version".
  3. **DAG cycle fixture**: two stages `a` (deps `[b]`) and `b` (deps `[a]`). **Each stage must have enough fields to pass schema validation first** — give both `acceptance: ["true"]` so the goal-backward check is satisfied and the cycle is reachable in `ExecutionGraph::build`. Expect exit 1, output mentions "Circular dependency".
  4. Plan with structural warning: a stage with `working_dir: "loom"` and an acceptance criterion starting with `loom/` (triggers redundant-prefix preflight warning). Stage must ALSO have at least one of acceptance/artifacts/wiring/wiring_tests to pass schema validation — use the same acceptance that triggers the warning. Expect exit 0 by default; same plan with `--strict` → exit 1.
  5. **Valid-fixture shape** for tests 1, 4, 5: use a single standard stage with `acceptance: ["true"]` (no artifacts) — `check_cross_stage_wiring_coverage` only triggers when `artifacts` is non-empty AND no wiring exists anywhere in the DAG. Acceptance-only standard stages are accepted by current schema validation.
  6. `--json` produces parseable JSON with `plan`, `valid`, `errors`, `warnings`, `levels` keys; redirect stdout, parse via `serde_json::from_str::<Value>`. Assert `plan.id` is a string (not null) for a valid plan.
  7. `--json` on nonexistent path → exit 1, stdout is valid JSON with `valid: false`, `errors` non-empty, `plan.id` and `plan.name` both `null`. Human mode on same nonexistent path → exit 1, stderr contains "not found".
  8. **Oversized plan**: write a file > 1 MiB (e.g., 1 MiB + 1 byte of arbitrary bytes). Expect exit 1 in both human and JSON modes; JSON envelope has `errors` mentioning "too large" or "limit".
  9. **`--no-color` test** (two branches — without (a) the test passes vacuously since assert_cmd's non-TTY auto-disables `colored`):
     - (a) Run with env `CLICOLOR_FORCE=1` and NO `--no-color` flag. Assert stdout contains `"\x1b["` (proves the test setup actually forces color).
     - (b) Run with env `CLICOLOR_FORCE=1` AND `--no-color`. Assert stdout contains NO `"\x1b["` sequences.
  10. **No side effects on target repo**: snapshot `target_repo/.work/` (mtime + child list) and `target_repo/.worktrees/` (existence + child list) BEFORE running `loom plan verify <plan-in-that-repo>`. Run the command. Assert the snapshots are byte-identical (or absent → absent). Also assert no `.work/` exists in the caller's cwd after the run.
- Add a small unit test in `loom/src/commands/plan/verify.rs` for the `should_fail(errors, warnings, strict)` pure function — all four branches.
- Add a wiring/review check: grep `loom/src/commands/plan/verify.rs` to confirm it does NOT use `WorkDir`, `find_work_dir`, `crate::fs::work_dir`, or any `.work/` loader. If any of those appear, the command is no longer side-effect-free.

_Subagent restriction:_ NO subagents in this stage. One agent does CLI + impl + tests + completions sequentially. The whole stage is ~250 LOC across known files and the coordination cost of subagents is not worth it.

**Files:**

- `loom/src/cli/types.rs`
- `loom/src/cli/dispatch.rs`
- `loom/src/commands/mod.rs`
- `loom/src/commands/plan/mod.rs` (new)
- `loom/src/commands/plan/verify.rs` (new)
- `loom/src/completions/dynamic/commands.rs`
- `loom/src/completions/dynamic/mod.rs` (one match arm)
- `loom/src/completions/dynamic/tests/tests_commands.rs` (or wherever completion unit tests live)
- `loom/src/plan/schema/detect.rs` (new — relocated `detect_stage_type`)
- `loom/src/plan/schema/mod.rs` (export `detect_stage_type`)
- `loom/src/commands/init/plan_setup.rs` (call relocated `detect_stage_type`; remove local copy)
- `loom/tests/integration/plan_verify.rs` (new)
- `loom/tests/integration/mod.rs` (register `pub mod plan_verify;`)

**Acceptance:**

- `cargo build` succeeds.
- `cargo test` passes (full suite — catches the plan_verify module registration and any regression in existing schema tests).
- A named test in the new file ran: `cargo test --test integration -- plan_verify::test_valid_plan_exits_zero --exact --nocapture` exits 0 and stdout contains `test result: ok. 1 passed`.
- `cargo clippy -- -D warnings` clean.

**Goal-backward verification:**

- `artifacts`: `src/commands/plan/verify.rs`, `src/commands/plan/mod.rs`, `tests/integration/plan_verify.rs`, `src/plan/schema/detect.rs`.
- `wiring`:
  - `src/cli/types.rs` contains `PlanCommands`
  - `src/cli/dispatch.rs` contains `plan::verify::execute`
  - `src/commands/mod.rs` declares `pub mod plan`
  - `src/completions/dynamic/commands.rs` registers `"plan",` in `TOP_LEVEL_COMMANDS`
  - `tests/integration/mod.rs` declares `pub mod plan_verify`
  - `src/plan/schema/mod.rs` re-exports `detect_stage_type`
- Extended `acceptance` (replaces the removed `truths` field): `./target/debug/loom plan verify --help` exits 0 and stdout contains `Verify a plan file`. End-to-end smoke test against `../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md` (a real valid plan) → exit 0.
- `wiring_tests`: top-level `loom --help` advertises `plan`; `loom plan verify <known-good-plan>` exits 0; `loom complete zsh "$PWD" "loom " "" "loom"` output contains `plan`.

---

### 3. Integration Verify

**Purpose:** Confirm the new command compiles, tests pass, lints clean, and is actually usable — including verifying it produces correct results on every existing plan in `doc/plans/`.

**Dependencies:** implement-plan-verify

**Tasks:**

_Build & Test:_

- `cargo test` (full suite, not just the new test file).
- `cargo clippy -- -D warnings`.
- `cargo build` (debug and release).

_Functional Verification (MANDATORY):_

- **Filter to metadata-bearing plans** when smoke-testing `doc/plans/` (the directory also contains REVIEW-_.md and codex-_.md docs without `<!-- loom METADATA -->` — the parser correctly rejects those, which would create false-positive failures):

  ```bash
  for f in $(rg -l "<!-- loom METADATA" ../doc/plans/*.md); do
    ./target/debug/loom plan verify "$f" || echo "FAIL: $f"
  done
  ```

  Record unexpected failures. Existing IN_PROGRESS-/PLAN-/DONE- plans should not have schema errors; new warnings are acceptable.
- Smoke test `--json` against one valid plan, pipe to `jq -e '.valid == true'` to verify JSON is well-formed AND `valid: true`.
- Smoke test `--strict` against a plan that produces warnings; verify exit code 1.
- **Verify the dynamic completion backend directly** (NOT the generated zsh script — that's a thin wrapper around `loom complete` and contains no per-command logic):

  ```bash
  ./target/debug/loom complete zsh "$PWD" "loom " "" "loom" | rg -qx "plan"
  ./target/debug/loom complete zsh "$PWD" "loom plan " "" "plan" | rg -qx "verify"
  ./target/debug/loom complete zsh "$PWD" "loom plan verify --" "--" "verify" | rg -q -- "--json|--no-color|--strict"
  ```

- **Verify `loom plan verify` has NO side effects on any repo**, not just the caller's cwd:
  - In a fresh TempDir (no `.git`, no `.work/`): run `loom plan verify <abs/path/to/known-good/plan.md>`. Assert exit 0; assert no `.work/`, `.worktrees/`, or other artifacts appear in the TempDir.
  - In the **target repo** (the one containing the plan): snapshot `target_repo/.work/` (mtime + recursive child list) and `target_repo/.worktrees/` (existence + list) BEFORE running. Run the command. Assert snapshots byte-identical.
  - Confirm via review that `commands/plan/verify.rs` does not import `crate::fs::work_dir`, `WorkDir`, `find_work_dir`, or anything from `crate::orchestrator`. (The code-review subagents check for this.)

_Code Review (MANDATORY — parallel review subagents):_

- Security review (`loom-code-reviewer` + `/loom-security-audit`): path handling (canonicalize before read), no command injection, **enforced 1 MiB file-size limit** (must be in code, not just a comment), no regression in existing validation. Confirm plans are documented as trusted input for terminal escape sequences in the command's `long_about`.
- Architecture review (`loom-code-reviewer`): module placement, no duplication with `commands/init/plan_setup.rs` (verify `detect_stage_type` is now in `plan/schema/detect.rs` and both callers use it), clear separation between human/JSON render paths, no `colored::Colorize` calls in the JSON path, no `WorkDir`/`find_work_dir`/`.work/` access in `commands/plan/`.
- Test coverage (`loom-code-reviewer`): edge cases covered (missing file, oversized file, no metadata block, malformed YAML, schema fail, DAG cycle, empty plan), both exit-code branches tested in BOTH human and JSON modes, JSON schema stable across error and success paths, `--no-color` test uses the two-branch pattern (force-on / force-on+flag).

_Fix-forward:_ All issues found by reviewers must be fixed in this stage. Nothing is "pre-existing."

**Files:** (no doc changes here — those are in knowledge-distill)

**Acceptance:**

- `cargo test`
- `cargo clippy -- -D warnings`
- `cargo build`
- Extended acceptance: `./target/debug/loom plan verify ../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md` exits 0 (path is `../` because working_dir is `loom`).

**Goal-backward verification:**

- Extended `acceptance`: full smoke run as above expressed as `command: ... exit_code: 0 stdout_contains: [...]` TruthCheck entries.
- `wiring`: `src/cli/dispatch.rs` contains `plan::verify::execute` (proves merge survived).

---

### 4. Knowledge Distillation

**Purpose:** Curate memories from this plan into permanent knowledge and update user-facing docs to mention the new command.

**Dependencies:** integration-verify

**Tasks:**

_Memory curation:_

- Read all stage memories (`loom memory show --all`) and curate any mistakes/decisions into `doc/loom/knowledge/`.
- If anything was learned about how `parse_plan_content` / `validate` split errors vs warnings that future plan-tooling work would benefit from, add to `patterns.md`.
- Add the new command entry to `entry-points.md` under the "Command Dispatch" table: `plan` → `commands/plan/verify.rs` → "Validate a plan without side effects".

_Documentation update (MANDATORY):_

- Update `README.md`:
  - In the "CLI Reference > Primary Commands" or a new "Plan Commands" subsection, document `loom plan verify <plan-path> [--strict] [--json] [--no-color]` with a short description.
  - Consider mentioning it in the "Write a Plan" section as a way to validate a draft.
- No CLAUDE.md changes (orchestration rules unchanged).
- If `/loom-plan-writer` skill's "After Writing Plan" step (currently runs `loom init --clean`) should change to use `loom plan verify` instead — note this in memory as a follow-up; do NOT edit the skill in this plan (the skill lives in `~/.claude/skills/` and is out of scope for this repo's plan).

**Files:**

- `doc/loom/knowledge/entry-points.md`
- `doc/loom/knowledge/patterns.md` (optional)
- `README.md`

**Acceptance:** `rg -q "## " doc/loom/knowledge/entry-points.md` and `rg -qF "loom plan verify" README.md`.

---

## Verification (end-to-end)

After `loom run` completes all stages, the user can manually verify (commands assume cwd is the loom repo root):

```bash
# Show help
loom plan verify --help

# Validate a known-good plan
loom plan verify doc/plans/DONE-PLAN-cwd-knowledge-resolution.md

# JSON output for tooling
loom plan verify --json doc/plans/DONE-PLAN-cwd-knowledge-resolution.md | jq -e '.valid == true'

# Strict mode (warnings → exit 1)
loom plan verify --strict doc/plans/PLAN-multimodel.md ; echo "exit=$?"

# Invalid path — JSON mode owns the error
loom plan verify --json /tmp/does-not-exist.md | jq -e '.valid == false and .plan.id == null'
echo "exit=$?"   # expect 1

# Run outside any git repo (no .work/, no init)
TMPDIR=$(mktemp -d) && cd "$TMPDIR" && loom plan verify "$OLDPWD/doc/plans/DONE-PLAN-cwd-knowledge-resolution.md" && \
  test ! -d .work && test ! -d .worktrees && echo "side-effect-free OK"

# Dynamic completions backend (NOT the generated zsh script)
loom complete zsh "$PWD" "loom " "" "loom" | rg -qx "plan" && echo "plan completion OK"
```

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  sandbox:
    enabled: true
    auto_allow: true
    excluded_commands:
      - "loom"
    filesystem:
      deny_read: []
      deny_write: []
      allow_write:
        - "loom/src/**"
        - "loom/tests/**"
        - "doc/loom/knowledge/**"
        - "README.md"
    network:
      allowed_domains:
        - "github.com"
        - "api.github.com"
        - "crates.io"
        - "static.crates.io"
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: knowledge-bootstrap
      name: "Bootstrap Knowledge Base"
      stage_type: knowledge
      model: "sonnet"
      reasoning_effort: "high"
      description: |
        Confirm existing knowledge covers the surface area for adding a new top-level
        command that validates a plan without side effects. Most knowledge already
        exists — this stage is a quick refresh, not a deep map.

        Use parallel subagents and skills to maximize performance.

        Step 0 — Check existing coverage:
          loom knowledge check
          If coverage >= 50% and the entries below exist, record:
            loom memory note "knowledge sufficient for plan-verify scope"
          and proceed.

        Step 1 — Confirm entry-points.md lists:
          - commands/verify.rs (existing goal-backward verify command)
          - plan/parser/mod.rs (parse_plan, parse_plan_content)
          - plan/schema/validation.rs (validate, validate_structural_preflight,
            check_knowledge_recommendations, check_sandbox_recommendations)
          - plan/graph/mod.rs (ExecutionGraph::build, cycle detection)
          - the "CLI Subcommand Registration Pattern" three-file checklist
          If any are missing, fill them with: loom knowledge update entry-points "..."

        Step 2 — Confirm patterns.md covers:
          - File-Based State Pattern
          - AcceptanceCriterion untagged enum
          If anything load-bearing for plan tooling is missing, add it.

        Step 3 — Record any insight about how loom init separates fatal errors
        (validate) from soft warnings (structural preflight, knowledge/sandbox
        recommendations) — this is the contract loom plan verify must mirror.

        MEMORY & KNOWLEDGE RECORDING:
        - loom memory note "..." for insights
        - loom knowledge update mistakes "..." if you find one worth preserving
        - loom memory list before completing

        ⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
        ALL memory/knowledge goes through loom memory and loom knowledge commands.
      dependencies: []
      acceptance:
        - "loom knowledge check --min-coverage 50"
        - 'rg -q "## " doc/loom/knowledge/architecture.md'
        - 'rg -q "## " doc/loom/knowledge/entry-points.md'
        - 'rg -q "## " doc/loom/knowledge/patterns.md'
      files:
        - "doc/loom/knowledge/**"
      working_dir: "."
      artifacts:
        - "doc/loom/knowledge/entry-points.md"
        - "doc/loom/knowledge/patterns.md"

    - id: implement-plan-verify
      name: "Implement loom plan verify"
      stage_type: standard
      model: "sonnet"
      reasoning_effort: "high"
      description: |
        Add a new top-level `loom plan` command with a `verify` subcommand that
        validates a plan file with NO side effects (no .work/, no git repo
        requirement, no worktrees). Reuses 100% of existing validation logic.

        Use skills (loom-rust, loom-testing) where helpful — but this stage
        runs as a SINGLE agent, no parallel subagents. The surface area is
        small and tightly coupled (CLI types, dispatch, command impl,
        completions, and tests all reference each other), so subagent
        coordination overhead outweighs the parallelism gain.

        WHY THIS COMMAND EXISTS:
        Today the only way to validate a plan is `loom init <path>`, which has
        heavy side effects. Plan authors (humans and /loom-plan-writer) need a
        cheap read-only validator. The validation logic already exists in
        plan/schema/validation.rs — this stage adds a new caller.

        IMPLEMENTATION CHECKLIST:

        1. CLI types (loom/src/cli/types.rs):
           - Add `Plan { command: PlanCommands }` variant to the Commands enum,
             grouped near `Check { ... }`.
           - Define `pub enum PlanCommands { Verify { ... } }` with fields:
               path: PathBuf            (positional, required)
               strict: bool             (--strict, promotes warnings to errors)
               json: bool               (--json, machine output, suppresses human)
               no_color: bool           (--no-color, disables ANSI)
           - Use the same clap attribute style as existing subcommands (Stage,
             Knowledge, Memory).

        2. Dispatch (loom/src/cli/dispatch.rs):
           - Add `plan` to the `use loom::commands::{...}` import.
           - Add the dispatch arm:
               Commands::Plan { command } => match command {
                 PlanCommands::Verify { path, strict, json, no_color } =>
                   plan::verify::execute(&path, strict, json, no_color),
               }

        3. Commands module registration (loom/src/commands/mod.rs):
           - Add `pub mod plan;`

        4. New module files:
           - Create loom/src/commands/plan/mod.rs containing `pub mod verify;`
           - Create loom/src/commands/plan/verify.rs (~150 LOC).

        5. execute(path, strict, json, no_color) -> anyhow::Result<()> behavior:

           a) Resolve `path`. If missing/not-a-file, bail with a clear error
              ("Plan file not found: <path>").

           b) Read file contents (std::fs::read_to_string).

           c) Extract structured errors. parse_plan_content collapses errors
              into one anyhow message — instead, mirror its body locally:
                let yaml = crate::plan::parser::extract_yaml_metadata(&content)?;
                let metadata: LoomMetadata = serde_yaml::from_str(&yaml)
                    .map_err(...)?;
                let validation_result = crate::plan::schema::validate(&metadata);
              This gives Vec<ValidationError>. Capture them as structured errors.
              Also call extract_plan_name(&content) for display.

           d) If validation_result is Ok, run the soft checks (otherwise skip
              them — there's no point chasing warnings on a plan that won't parse):
                - validate_structural_preflight(&metadata.loom.stages, repo_root_opt)
                - check_knowledge_recommendations(&metadata.loom.stages)
                - check_sandbox_recommendations(&metadata)

              `repo_root_opt`: walk up from plan_path.parent() looking for a
              directory containing a `.git` entry (file OR directory — `.git`
              is a file in worktrees). Stop at filesystem root. If found,
              Some(that_dir); else None. The structural preflight skips
              build-tool config-file checks gracefully when None.

              ⛔ Do NOT use cwd or `git rev-parse --show-toplevel` for this —
              the plan being verified may live in a different repo than the
              caller's cwd; build-tool checks must resolve relative to the
              PLAN's repo, not the user's.

           e) Build the DAG via crate::plan::graph::ExecutionGraph::build(stages.clone())
              and treat any error as a hard error (cycles, malformed graph).

           f) Compute topological levels via
              crate::plan::graph::levels::compute_all_levels(...).

           g) Render results:

              HUMAN (default) — match the visual conventions used by `loom init`:
                Header: plan id, name, source path (cyan/bold).
                Errors section: red ✗, indented, one per line. If a
                  ValidationError has a stage_id, prefix with "[stage_id]".
                Warnings section: yellow ⚠, grouped under headings
                  "Structural", "Knowledge", "Sandbox". Hide a heading if its
                  list is empty.
                Stages by Level: for each level, print "Level N (M stages)"
                  then for each stage at that level a line with:
                    indicator (● green if no deps, ○ yellow otherwise),
                    id, name (bold), stage_type, and dependencies if any.
                  Use colors::stage_color from commands/graph for visual
                  differentiation (read it — do not move it).
                Summary: "N error(s), M warning(s)". If --strict and M > 0,
                  add "(strict: failing)".
                Honor --no-color by calling colored::control::set_override(false)
                  at the top of execute().

              JSON (--json) — suppress ALL human output. Build:
                {
                  "plan": {"id": "<id>"|null, "name": "<name>"|null, "source": "<input path>"},
                  "valid": <bool>,
                  "errors": [{"stage_id": null | "<id>", "message": "..."}],
                  "warnings": {
                    "structural": ["..."],
                    "knowledge": ["..."],
                    "sandbox": ["..."]
                  },
                  "levels": [
                    [{"id": "...", "name": "...", "stage_type": "...", "dependencies": [...]}]
                  ]
                }
                Use serde_json::to_string_pretty and println! it. Use serde-derived
                structs so the schema is documented in code, not a free-form
                serde_json::Value (define helper structs at the top of verify.rs).

                EDGE CASE — YAML extraction or deserialization fails (no
                LoomMetadata available): plan.id and plan.name must be
                Option<String>. Emit:
                  {"plan": {"id": null, "name": null, "source": "<input>"},
                   "valid": false,
                   "errors": [{"stage_id": null, "message": "<extraction error>"}],
                   "warnings": {"structural": [], "knowledge": [], "sandbox": []},
                   "levels": []}
                Same when plan-name extraction fails but YAML parses.

           h) Exit code policy — split per output mode:

              Pure decision function (unit-test all four branches inline):
                fn should_fail(errors: usize, warnings: usize, strict: bool) -> bool {
                  errors > 0 || (strict && warnings > 0)
                }

              HUMAN mode: print all sections to stdout, then:
                if should_fail { bail!("Plan validation failed (N error(s), M warning(s))") }
                else { Ok(()) }
              anyhow's one-line stderr summary on bail is acceptable for humans.

              JSON mode: print serde_json::to_string_pretty(&output) to stdout
              via println!, call std::io::stdout().flush()?, then:
                std::process::exit(if should_fail { 1 } else { 0 })
              Do NOT return Err in JSON mode — anyhow's stderr message would
              pollute the "JSON-to-stdout-only" contract that tooling depends on.

        6. Dynamic completions — four concrete edits:

           loom/src/completions/dynamic/commands.rs:
           a. Add "plan" to TOP_LEVEL_COMMANDS (~line 6-28), alphabetically
              between "memory" and "repair".
           b. Add a `"plan" => &["verify"],` arm to complete_subcommands
              (~line 37-62 match), alphabetically.
           c. Add a `["plan", "verify"] => &["--json", "--no-color", "--strict"],`
              arm to complete_flags (~line 70+ match).
           d. Add "plan" to the matches! tuple in has_subcommands (~line 132)
              so the dispatcher routes "plan <TAB>" through complete_subcommands.

           loom/src/completions/dynamic/mod.rs:
           e. Add `("plan", "verify") => complete_plan_files(cwd, prefix),`
              arm to complete_after_subcommand (~line 186) so tab-completing
              the positional path returns plan files. complete_plan_files is
              already pub-used in mod.rs:26 — no new imports.

           loom/src/completions/dynamic/tests/tests_commands.rs (or equivalent test file):
           f. Assert "plan" appears in complete_commands("") output.
           g. Assert complete_subcommands("plan", "") returns ["verify"].
           h. Assert has_subcommands("plan") is true.
           i. Assert complete_flags(&["plan", "verify"], "--") contains
              "--json", "--no-color", "--strict".
           j. Assert the route for ("plan", "verify") returns plan files —
              create a TempDir tree with doc/plans/PLAN-foo.md, invoke the
              positional-completion path, expect "doc/plans/PLAN-foo.md" in
              the output (matches the existing complete_plan_files tests in
              completions/dynamic/tests.rs).

           Note: we deliberately use complete_plan_files for `plan verify`
           rather than mirroring `init` (which returns empty and falls back to
           native shell file completion). Reason: this is a plan-validation
           command, so completing on plan files is strictly more useful than
           generic file completion. If the user wants to verify a file outside
           doc/plans/, they type the path directly (just like they would for
           init).

        7. Integration tests (loom/tests/integration/plan_verify.rs, register
           `pub mod plan_verify;` in loom/tests/integration/mod.rs):

           Use std::process::Command (NOT assert_cmd — not a dev-dep).
           Pattern: see loom/tests/integration/dependency_simple.rs:5.
           Locate the binary via env!("CARGO_BIN_EXE_loom"). Each test writes
           its plan into a TempDir (tempfile crate, already in dev-deps) and
           invokes `loom plan verify <path>`.

           Fixture rules — these matter because schema validation runs BEFORE
           DAG cycle detection. Any test that wants to reach
           ExecutionGraph::build must define enough fields per stage to pass
           schema validation first:
             - Minimal valid stage: stage_type: standard, working_dir: ".",
               acceptance: ["true"]. NO artifacts (artifacts trigger
               check_cross_stage_wiring_coverage on leaf stages).
             - To exercise warnings without errors: same minimal stage plus
               the warning-triggering field (e.g. acceptance starting with
               "loom/" when working_dir is "loom").
             - Cycle fixture: TWO stages each with acceptance: ["true"], one
               depending on the other and vice-versa. Schema validates, cycle
               surfaces in ExecutionGraph::build.

           Scenarios (one #[test] each):
           - test_valid_plan_exits_zero: minimal valid plan as above. Expect
             exit 0, stdout contains "0 error(s)" and the plan name.
           - test_invalid_version: version: 2. Expect exit 1, stdout/stderr
             mentions "Unsupported version".
           - test_self_dependency: stage depends on itself. Expect exit 1.
           - test_dag_cycle: cycle fixture as defined above. Expect exit 1,
             output contains "Circular dependency".
           - test_structural_warning_not_strict: working_dir "loom" with
             acceptance "loom/...". Expect exit 0, stdout contains
             "redundant working_dir prefix".
           - test_structural_warning_strict: same plan with --strict. Expect
             exit 1.
           - test_json_valid: valid plan with --json. Parse stdout via
             serde_json::from_str::<serde_json::Value>; assert keys exist;
             assert "valid": true; assert plan.id is a string.
           - test_json_missing_path: --json /nonexistent. Expect exit 1, stdout
             is parseable JSON with valid=false, errors non-empty, plan.id and
             plan.name both null.
           - test_json_oversized_plan: write 1_048_577 bytes (1 MiB + 1) of
             arbitrary content to a temp file. Expect exit 1 in both human
             and JSON modes; JSON errors[0].message mentions "too large" or "limit".
           - test_human_missing_path: human mode against nonexistent path.
             Expect exit 1, stderr contains "not found" or similar.
           - test_no_color_strips_ansi: TWO branches (without (a) the test
             passes vacuously since process Command is non-TTY which already
             disables `colored`):
               (a) Run with env `CLICOLOR_FORCE=1` and NO --no-color flag;
                   assert stdout contains "\x1b[" (proves the test setup
                   actually forces color, so the next assertion is meaningful).
               (b) Run with env `CLICOLOR_FORCE=1` AND --no-color flag;
                   assert stdout contains NO "\x1b[" sequences.
           - test_no_side_effects_on_target_repo: create a TempDir, init a
             git repo inside, write a known-good plan into it. Snapshot the
             dir contents (recursive list, sorted). Run loom plan verify
             against the plan. Snapshot again. assert_eq!(before, after).
             Also assert no `.work/` was created in the caller's cwd
             (use a different TempDir as cwd).
           - test_no_metadata_block: plan markdown without `<!-- loom METADATA -->`.
             Expect exit 1; in --json mode, plan.id is null, errors describes
             the missing metadata block.

           Inline unit tests in verify.rs:
           - test_should_fail: cover all four branches of
             should_fail(errors, warnings, strict).
           - test_repo_root_walk: create a TempDir tree with .git at depth N,
             a plan file at depth N+M (M>0). Assert repo_root walk returns
             the .git parent. Also: tree with no .git anywhere → returns None.

        STYLE / REUSE:
        - Reuse colors::stage_color from loom/src/commands/graph/colors.rs for
          stage coloring in human output. Do NOT duplicate stage_color.
        - Reuse indicators::status_indicator only if it makes sense for "ready"
          (deps empty) vs "pending" (deps non-empty); otherwise use simple
          ●/○. Levels output is NOT live state, so symbols should be muted.
        - Do NOT touch parse_plan_content. Mirror its body inside execute()
          so we can capture Vec<ValidationError> structurally.

        ERROR HANDLING:
        - Use anyhow::Result throughout, .with_context() for I/O and YAML errors.
        - On hard errors, the Err message should be a short summary like
          "Plan validation failed (3 error(s))" — the detailed errors are
          already printed (human) or in the JSON object (json mode).

        MEMORY RECORDING (use loom memory ONLY — never loom knowledge):
        ⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
        ⛔ NEVER use loom knowledge update (reserved for knowledge-distill).
        Record IMMEDIATELY when these happen:
        - MISTAKE: tried X, failed -> loom memory note "mistake: ..."
        - DECISION: chose X over Y -> loom memory decision "..." --context "..."
        - SURPRISE: unexpected behavior -> loom memory note "found: ..."
        - GOTCHA: trap for future agents -> loom memory note "gotcha: ..."
        Do NOT record procedural actions ("read file", "ran tests").

        Before completing: loom memory list (verify insights captured, not empty).
      dependencies: ["knowledge-bootstrap"]
      acceptance:
        - "cargo build"
        - "cargo test"
        - "cargo clippy -- -D warnings"
        - command: "./target/debug/loom plan verify --help"
          exit_code: 0
          stdout_contains: ["Verify"]
      files:
        - "src/cli/types.rs"
        - "src/cli/dispatch.rs"
        - "src/commands/mod.rs"
        - "src/commands/plan/**"
        - "src/completions/dynamic/commands.rs"
        - "src/completions/dynamic/mod.rs"
        - "src/completions/dynamic/tests/**"
        - "src/plan/schema/detect.rs"
        - "src/plan/schema/mod.rs"
        - "src/commands/init/plan_setup.rs"
        - "tests/integration/plan_verify.rs"
        - "tests/integration/mod.rs"
      working_dir: "loom"
      artifacts:
        - "src/commands/plan/mod.rs"
        - "src/commands/plan/verify.rs"
        - "src/plan/schema/detect.rs"
        - "tests/integration/plan_verify.rs"
      wiring:
        - source: "src/cli/types.rs"
          pattern: "PlanCommands"
          description: "PlanCommands enum is defined in CLI types"
        - source: "src/cli/dispatch.rs"
          pattern: "plan::verify::execute"
          description: "Dispatch wires loom plan verify to the command impl"
        - source: "src/commands/mod.rs"
          pattern: "pub mod plan"
          description: "plan module is registered under commands"
        - source: "src/completions/dynamic/commands.rs"
          pattern: '"plan",'
          description: "plan subcommand is registered in TOP_LEVEL_COMMANDS"
        - source: "src/plan/schema/mod.rs"
          pattern: "detect_stage_type"
          description: "Relocated stage_type detector is re-exported from schema"
        - source: "tests/integration/mod.rs"
          pattern: "pub mod plan_verify"
          description: "Integration test file is registered in the test target"
      wiring_tests:
        - name: "Top-level help lists plan subcommand"
          command: "./target/debug/loom --help"
          success_criteria:
            stdout_contains: ["plan"]
          description: "loom --help advertises the new plan command"
        - name: "Verify known-good plan exits zero"
          command: "./target/debug/loom plan verify ../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md"
          success_criteria:
            exit_code: 0
          description: "Smoke test against a real completed plan"

    - id: integration-verify
      name: "Integration Verification"
      stage_type: integration-verify
      model: "opus"
      reasoning_effort: "high"
      description: |
        Final integration verification after implement-plan-verify is merged.

        Use parallel subagents and skills to maximize performance.

        CRITICAL: tests passing != feature working. This stage must prove
        loom plan verify is reachable from a fresh shell, returns correct
        exit codes against real plans, and produces stable JSON output.

        ⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
        ALL memory/knowledge goes through loom memory and loom knowledge commands.

        CONTEXT GATHERING (FIRST):
        0a. Read doc/plans/PLAN-loom-plan-verify.md (or the IN_PROGRESS-/DONE-
            prefixed variant — check .work/config.toml for source_path).
        0b. Read ALL stage memories: loom memory show --all.
        0c. Read doc/loom/knowledge/*.md.

        ZERO TOLERANCE: ALL warnings, lints, and test failures must be FIXED.

        BUILD & TEST:
        1. cargo test (full suite).
        2. cargo clippy -- -D warnings.
        3. cargo build (debug and release).

        CODE REVIEW (MANDATORY — parallel subagents):
        4. Spawn parallel loom-code-reviewer agents:
           - Security review (+/loom-security-audit): path traversal in `path`
             arg, command injection in optional `git rev-parse` shell-out, file
             size limit on plan read, no regression in existing validate().
           - Architecture review: module placement under commands/plan/, no
             duplication with commands/init/plan_setup.rs, separation between
             human and JSON render paths, no colored::Colorize leakage into
             JSON output.
           - Test coverage review: every exit-code branch covered, JSON schema
             stable, --no-color verified, error path on nonexistent file.
        5. Fix ALL issues using loom-software-engineer (loom-code-reviewer is
           read-only). Do not just report.

        FUNCTIONAL VERIFICATION (MANDATORY):
        6. Run the new command against every METADATA-BEARING plan in
           doc/plans/ — the directory contains REVIEW-*.md and codex-*.md docs
           without a `<!-- loom METADATA -->` block, which the parser correctly
           rejects. Filter first:
             for f in $(rg -l "<!-- loom METADATA" ../doc/plans/*.md); do
               ./target/debug/loom plan verify "$f" || echo "FAIL: $f"
             done
           Record which plans produce warnings vs errors. No DONE-* or PLAN-*
           plan that was previously valid (i.e., that loom init accepts today)
           should fail with errors. New warnings are acceptable.
        7. Verify --json output is jq-parseable:
             ./target/debug/loom plan verify --json ../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md | jq -e '.valid == true'
        8. Verify --json owns the missing-file path:
             ./target/debug/loom plan verify --json /tmp/does-not-exist.md | \
               jq -e '.valid == false and .plan.id == null and (.errors | length > 0)'
             expect exit 1 from loom (jq exit 0 means the JSON shape matched).
        9. Verify --strict promotes warnings:
             ./target/debug/loom plan verify --strict <plan-with-warnings>
             expect exit 1.
        10. Verify NO side effects in a fresh TempDir AND in the target repo:
            (a) TempDir test:
                tmp=$(mktemp -d) && cd "$tmp" && \
                  ./target/debug/loom plan verify <abs/path/to/known-good.md> && \
                  test ! -d .work && test ! -d .worktrees
            (b) Target-repo test:
                Snapshot find ../.work -printf "%T@ %p\n" | sort > before.txt
                Run loom plan verify against a plan in the target repo.
                find ../.work -printf "%T@ %p\n" | sort > after.txt
                diff before.txt after.txt  # must be empty
        11. Verify dynamic completion backend (NOT the generated zsh script —
            it is a thin wrapper around `loom complete`):
              ./target/debug/loom complete zsh "$PWD" "loom " "" "loom" | rg -qx "plan"
              ./target/debug/loom complete zsh "$PWD" "loom plan " "" "plan" | rg -qx "verify"

        Record discoveries to loom memory for knowledge-distill to curate.
      dependencies: ["implement-plan-verify"]
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
        - "cargo build"
        - command: "./target/debug/loom plan verify ../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md"
          exit_code: 0
        - command: "./target/debug/loom --help"
          stdout_contains: ["plan"]
      files:
        - "src/**"
        - "tests/**"
      working_dir: "loom"
      wiring:
        - source: "src/cli/dispatch.rs"
          pattern: "plan::verify::execute"
          description: "Dispatch arm survived merge to main"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      stage_type: knowledge-distill
      model: "sonnet"
      reasoning_effort: "high"
      description: |
        Curate stage memories into permanent knowledge and update user docs.

        ⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
        ALL memory/knowledge goes through loom memory and loom knowledge commands.

        CONTEXT GATHERING:
        1. Read doc/plans/PLAN-loom-plan-verify.md (or current renamed variant).
        2. loom memory show --all.
        3. Read doc/loom/knowledge/*.md.

        MEMORY CURATION:
        4. Curate mistakes as actionable prevention rules with the standard
           format (What happened / Why / Prevention / Fix).
        5. Add `loom plan verify` to entry-points.md "Command Dispatch" table.
        6. If any pattern about "side-effect-free validators reusing init's
           soft-warning helpers" came up, add it to patterns.md.

        DOCUMENTATION UPDATE (MANDATORY):
        7. README.md:
           - Document `loom plan verify <plan-path> [--strict] [--json] [--no-color]`
             under "CLI Reference > Primary Commands" (or add a small new
             "Plan Commands" subsection — match the existing style).
           - Optionally mention it in the "Write a Plan" section as a way to
             validate a draft without `loom init`.
        8. CONTRIBUTING.md — only if it documents CLI verbs that should now
           mention plan verification (skip otherwise).
        9. Skip /loom-plan-writer skill edits — that file lives in
           ~/.claude/skills/ and is out of scope for this repo's plan. Record
           a follow-up memory note suggesting the skill's "After Writing Plan"
           step migrate from `loom init --clean` to `loom plan verify`.

        loom review at the end to catch stale entries.
      dependencies: ["integration-verify"]
      acceptance:
        - 'rg -q "## " doc/loom/knowledge/entry-points.md'
        - 'rg -qF "loom plan verify" README.md'
      files:
        - "doc/loom/knowledge/**"
        - "README.md"
      working_dir: "."
      artifacts:
        - "doc/loom/knowledge/entry-points.md"
        - "README.md"
```

<!-- END loom METADATA -->
