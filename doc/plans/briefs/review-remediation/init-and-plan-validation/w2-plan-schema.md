# init-and-plan-validation / w2-plan-schema: dependency-cycle check, parent-component test, lexed ungrantable-resource lint

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/orchestrator.md`, sections R4 (cycle half) and CR-43; `doc/plans/briefs/review-remediation/validation/planner-backend.md`, section CR-75 (sub-claims b and c; d is refuted, do nothing for it).

Unit rules (restated): you run no `git` command, touch no `.loom/` path and run no tests. Your only proof is `cargo check --all-targets` from `loom/`. The orchestrator runs the tests you write. You answer from the base tree and cannot see another worker's edits; the other codex unit (w3) owns `src/plan/parser/**`, `src/plan/amendment*`, `src/fs/memory/**` and never touches your files.

## Findings covered
- R4 (cycle half) and CR-43: plan schema validation checks unknown and self dependencies only, so a multi-stage cycle passes `parse_plan` and `loom init`; the first cycle check runs later, at `loom run`.
- CR-75(b): `.contains("..")` rejects valid names such as `foo..bar.rs`; four sites in `validate`.
- CR-75(c): `criterion_needs_ungrantable_resource` matches raw text, so `rg -n 'loom map' doc/x.md` or `echo "loom knowledge context"` is flagged, and refused as an error by the v2 lint.

## Files
Own (write), all under `loom/`:
- `src/plan/schema/validation.rs`
- `src/plan/schema/validation/dependency_cycles.rs` (new)
- `src/plan/schema/validation/path_checks.rs` (new)
- `src/plan/schema/validation/criterion_hazards.rs`
- `src/plan/schema/validation/criterion_hazards/ungrantable.rs` (new)
- `src/plan/schema/tests/mod.rs`
- `src/plan/schema/tests/dependency_cycle_tests.rs` (new)
- `src/plan/schema/tests/path_component_tests.rs` (new)
- `src/plan/schema/tests/ungrantable_resource_tests.rs` (new)

Read only: `src/plan/graph/build.rs` (`ExecutionGraph::build`), `src/plan/graph/cycle.rs`, `src/plan/schema/validation/shell_lex.rs`, `src/plan/schema/validation/v2_lints/sandbox_capability.rs` (caller; no edit needed), `src/plan/schema/tests/validation_tests.rs`, `src/plan/schema/tests/v2_lint_command_tests.rs`.
Frozen (contract tests, never edit): `src/plan/schema/dependency_cycle_contract_tests.rs`, `src/plan/schema/path_component_contract_tests.rs`.

Size limits that bind this unit (`loom/maintainability-baseline.txt`): `validation.rs` is 1063 lines (baseline row) and `validate` is 357 lines (baseline row): neither may grow. `src/plan/schema/tests/validation_tests.rs` is also a baseline row: add nothing to it. `criterion_hazards.rs` is 388 lines: it must shrink.

## Changes

### 1. Dependency-cycle check (R4 cycle half, CR-43)
New file `src/plan/schema/validation/dependency_cycles.rs` with exactly this entry point (pinned; `validate` in `validation.rs` calls it):

```rust
pub(super) fn finish(
    stages: &[StageDefinition],
    errors: Vec<ValidationError>,
) -> Result<(), Vec<ValidationError>>
```

Behaviour: when `errors` is non-empty return `Err(errors)` untouched (the graph build needs every dependency to name an existing stage, which the earlier checks guarantee only when they passed). When `errors` is empty call `crate::plan::graph::ExecutionGraph::build(stages.to_vec())`; on `Err(error)` return `Err(vec![ValidationError { message: error.to_string(), stage_id: None }])`; otherwise `Ok(())`. The message is the existing text from `plan/graph/cycle.rs`: `Circular dependency detected: a -> b -> c -> a` (stage ids in cycle order, first id repeated at the end). `ValidationError` renders a `stage_id: None` error as the bare message, so `parse_and_validate` fails with `Validation errors:\n  - Circular dependency detected: a -> b -> a`. Use the imports `super::super::types::{StageDefinition, ValidationError}` as `criterion_hazards.rs` does. Doc comment: why the call sits at the end of `validate` (one place: `loom plan verify`, `loom init` preflight, the run loader and the planner all go through `validate`).

In `validation.rs`: add `mod dependency_cycles;` next to the other `mod` lines (line 17-23 region). In `validate`, replace the closing

```rust
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
```

with `dependency_cycles::finish(&metadata.loom.stages, errors)`. That shortens `validate` by four lines. Do not add a duplicate-edge or duplicate-id check: both claims of CR-43 are refuted (duplicate ids are rejected earlier, duplicate edges have no observable effect).

`commands/plan/verify/checks.rs` (`dag_levels`) keeps its own `ExecutionGraph::build` call; it is outside this unit and now unreachable for a cycle. Leave it.

### 2. Parent-component test (CR-75b)
New file `src/plan/schema/validation/path_checks.rs`:

```rust
pub(super) fn has_parent_component(path: &str) -> bool
```

True when `std::path::Path::new(path).components().any(|c| c == std::path::Component::ParentDir)`. Doc comment: a name such as `foo..bar.rs` is an ordinary component; only a component equal to `..` escapes the directory. Add `mod path_checks;` and `use path_checks::has_parent_component;` to `validation.rs` and replace the four `.contains("..")` tests inside `validate`: `stage.working_dir` (about line 351), `artifact` in the artifacts loop (about 403), `wiring.source` (about 420), `rt.file` of `regression_test` (about 545). Keep every message text unchanged (`working_dir cannot contain path traversal (..)`, `Artifact #{} contains path traversal (..)`, `Wiring #{} source contains path traversal (..)`, `regression_test.file cannot contain path traversal (..)`): existing tests match on `path traversal`. Do not touch the `starts_with('/')` checks. Each replacement keeps the line count of `validate`.

Net effect on `validation.rs` with both steps: three added lines (the two `mod` lines and the `use`) against four removed lines in `validate`, so the file shrinks by one. Count with `wc -l` and keep the file at or below 1063 lines.

### 3. Lexed ungrantable-resource detection (CR-75c)
Today `criterion_hazards.rs` defines `pub(super) fn criterion_needs_ungrantable_resource(cmd: &str) -> Option<&'static str>` (about lines 149-165, with its doc comment): `cmd.contains("loom map")`, `cmd.contains("loom knowledge context")`, then whitespace tokens `tmux` / `docker`. Callers: `validation.rs` (`warn_ungrantable_acceptance`) and `v2_lints/sandbox_capability.rs` (`check_commands`, imports it as `super::super::criterion_hazards::{criterion_needs_ungrantable_resource, scan, Hazard}`). Both keep working unchanged if the function keeps its name, signature and path.

Change:
- Delete the function and its doc comment from `criterion_hazards.rs`. Add near the other `mod` lines of that file `mod ungrantable;` and `pub(super) use ungrantable::criterion_needs_ungrantable_resource;` (the function in the new file must be `pub(in crate::plan::schema::validation)` so the re-export is legal).
- New file `src/plan/schema/validation/criterion_hazards/ungrantable.rs`, same signature, moved doc comment (keep its explanation of why these resources cannot be granted). Implementation: lex with `lex` and `simple_commands` from `super::super::shell_lex`, and walk every simple command exactly as `scan_into` in the parent does (parent items `command_start`, `nested_scripts`, `MAX_NESTING` are reachable through `super::`): `let argv = &simple.words[command_start(&simple.words)..]`, recurse into `nested_scripts(simple)` up to `MAX_NESTING` (command substitutions and `sh -c` scripts). Match on the command word only: `argv[0].command_name()` (the word without its directory):
  - `loom` and `argv[1].value == "map"` gives `"loom map"`;
  - `loom`, `argv[1].value == "knowledge"`, `argv[2].value == "context"` gives `"loom knowledge context"`;
  - `tmux` gives `"tmux"`; `docker` gives `"docker"`.
  Return the first match in command order. Text that is only an argument (`rg -n 'loom map' doc/x.md`, `echo "loom knowledge context"`, `grep tmux notes.md`, `cargo test --test tmux_backend`) matches nothing. `./target/debug/loom map ...` still matches (`command_name` strips the directory).

## Interfaces shared with other workers
None used from other workers. Exposed for the orchestrator and the frozen contracts: `crate::plan::schema::validate(&LoomMetadata) -> Result<(), Vec<ValidationError>>` (existing, public) now returns the cycle error described in step 1; `crate::plan::parser::parse_and_validate(&str)` reports it as `Validation errors:\n  - Circular dependency detected: ...`.

## Tests (write them; the orchestrator runs them)
Build stages with `StageDefinition { id: ..., name: ..., working_dir: ".".into(), ..Default::default() }` inside `LoomMetadata { loom: LoomConfig { version: 1, stages, ..Default::default() } }` (as `src/commands/init/tests.rs` `create_test_plan` does); `validation_tests.rs` has more fixtures to copy from. Register the three new files in `src/plan/schema/tests/mod.rs` in alphabetical position.
1. `src/plan/schema/tests/dependency_cycle_tests.rs`: `two_stage_cycle_fails_validation` (`a -> b -> a`; `validate` returns `Err` with one message containing `Circular dependency detected` and both ids); `acyclic_diamond_validates` (a plan with `d` depending on `b` and `c`, both on `a`, returns `Ok`); `unknown_dependency_is_reported_without_a_cycle_error` (the error list holds only the `Unknown dependency` message, no `Circular`). The first fails today.
2. `src/plan/schema/tests/path_component_tests.rs`: for each of the four sites a name with an embedded `..` (`foo..bar.rs`, `out/a..b`) yields no error containing `path traversal`; `../x` and `a/../b` still yield one. Use `working_dir`, an `artifacts` entry, a `wiring` entry (`WiringCheck { source, pattern, description, literal }`) and `regression_test` (see `src/plan/schema/tests/regression_test_tests.rs` for the shape). The accepted-name cases fail today.
3. `src/plan/schema/tests/ungrantable_resource_tests.rs`: through `validate_structural_preflight(&[stage], None)` (see `validation_tests.rs` line 527 for the call and the assertion style): `loom map --outline src/lib.rs`, `./target/debug/loom map --outline src/lib.rs`, `sh -c 'loom knowledge context --stage x'`, `tmux kill-server` and `docker ps` each produce a warning naming the resource; `rg -n 'loom map' doc/x.md`, `echo "loom knowledge context"`, `rg -n tmux docs/x.md` and `cargo test --test tmux_backend` produce none. The v2 lint side (`sandbox_capability`) is exercised by the existing `ungrantable_resource_is_error_in_v2` and `grantable_commands_are_clean` in `v2_lint_command_tests.rs`; add one case there only if the helpers `matching`, `plan`, `stage` (imported in that file from `super`) are reachable from your new file without editing `tests/mod.rs` beyond the three `mod` lines.
Existing tests you must not weaken: `validation_tests.rs` lines 527-607 (`test_preflight_warns_on_loom_map_acceptance`, `..._binary_path_acceptance`, `..._loom_knowledge_context_acceptance`, `test_preflight_no_warning_for_tmux_substring_in_test_name`, `test_preflight_warns_on_tmux_token_acceptance`), `ungrantable_resource_is_error_in_v2`, `grantable_commands_are_clean`, and `tests/integration/plan_verify.rs` `test_dag_cycle` (still passes: the schema stage now reports the cycle). Search the crate for plan fixtures that contain a cyclic `dependencies` list and are parsed through `validate`: `rg -n -i 'circular|cycle' src tests --glob '*.rs'` and read each hit; none is expected beyond `plan/graph` tests, which call `ExecutionGraph::build` directly. Report any other.

Contract tests this unit turns green (by id): `schema-refuses-three-stage-cycle`, `dotdot-in-a-file-name-is-accepted`.

## Proof
From `loom/`: `cargo check --all-targets`. Then `wc -l src/plan/schema/validation.rs src/plan/schema/validation/criterion_hazards.rs` and state both counts in your report (first at most 1063, second at most 388). You run no tests.

## Traps
- Calling `ExecutionGraph::build` when `errors` is non-empty turns every unknown dependency into a second error from `populate_edges`: call it only on an empty list.
- `Path::components` drops `.` and doubled slashes and sees `a/../b` as containing `ParentDir`, but it does not treat `foo..bar` as one; a hand-written `split('/')` is not needed and a `.contains("..")` left at any of the four sites is the bug.
- A re-export `pub(super) use` of an item declared `pub(super)` in the child does not compile; the item needs `pub(in crate::plan::schema::validation)`.
- Do not edit `v2_lints/sandbox_capability.rs`: it imports the function by path and keeps working.
- `validate` and `validation.rs` are at their baseline: do not leave a net line gain in either.
