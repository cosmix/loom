# W5: the bootstrap warning and the bookend lint tests

Read `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md` first.

## Files you own

- `src/plan/schema/validation.rs`
- `src/plan/schema/validation/knowledge_recommendations.rs` (new)
- `src/commands/plan/verify.rs`
- `src/commands/init/plan_setup.rs`
- `src/plan/schema/tests/knowledge_recommendations_tests.rs`
- `tests/integration/plan_verify.rs`
- `src/plan/schema/tests/v2_lint_tests.rs` (one submodule declaration)
- `src/plan/schema/tests/v2_lint_bookend_tests.rs` (new)
- `src/plan/schema/tests/v2_lint_command_tests.rs`
- `src/plan/schema/tests/v2_lint_environment_repo_tests.rs`

Read-only: `src/fs/knowledge/dir.rs` (`KnowledgeDir::new`, `has_content`), `src/plan/schema/validation/v2_fields.rs` (`split_lint_findings`), `src/plan/schema/detect.rs` (`detect_stage_type_from_id_name`). C1, a codex unit running in parallel, writes `src/plan/schema/validation/v2_lints/bookends.rs` and registers it in `v2_lints/mod.rs`; you never edit either.

## 1. The bootstrap warning fires only for a missing or scaffold-only tree

`check_knowledge_recommendations` (`validation.rs`, about lines 1105-1137) warns whenever a plan has a root stage and no stage whose id or name contains "knowledge". It ignores the repository, so `loom plan verify --strict` fails every plan that correctly omits a bootstrap stage in a documented repository (`commands/plan/verify.rs`, `JsonWarnings::total` counts the warning).

1. Move the function to the new module `src/plan/schema/validation/knowledge_recommendations.rs` (`validation.rs` is 1222 lines in the ledger and must not grow). In `validation.rs`, delete the function and its doc comment, and declare `mod knowledge_recommendations;` beside the other child modules (about lines 16-22) plus `pub use knowledge_recommendations::check_knowledge_recommendations;`. The re-export in `src/plan/schema/mod.rs` (`pub use validation::{check_knowledge_recommendations, ...}`) and the test import `crate::plan::schema::validation::check_knowledge_recommendations` keep working unchanged.
2. New signature: `pub fn check_knowledge_recommendations(stages: &[StageDefinition], repo_root: Option<&Path>) -> Vec<String>`. Keep the id/name rule and the root-stage rule. When `repo_root` is `Some(root)` and `crate::fs::knowledge::KnowledgeDir::new(root).has_content()` is true, return no warning. When `repo_root` is `None` (no repository root found), keep warning: the check cannot see a tree. Keep the warning's first sentence (`Consider adding a 'knowledge-bootstrap' stage to capture codebase knowledge.`) and add the reason: the repository's knowledge tree is missing or holds only the scaffold.
3. Callers: `commands/plan/verify.rs`, inside `execute`, `knowledge: check_knowledge_recommendations(stages),` becomes `knowledge: check_knowledge_recommendations(stages, repo_root.as_deref()),` (one line; `repo_root` is the `Option<PathBuf>` from `find_repo_root` a few lines above; `execute` stays at 133 lines, the file at 555). `commands/init/plan_setup.rs`, inside `initialize_with_plan`, `let warnings = check_knowledge_recommendations(&parsed_plan.stages);` becomes `let warnings = check_knowledge_recommendations(&parsed_plan.stages, Some(repo_root));` (one line; the function stays at 192).
4. `knowledge_recommendations_tests.rs` (six tests): pass `None` in the six existing calls. Add `a_populated_knowledge_tree_needs_no_bootstrap` (a `TempDir` with `doc/loom/knowledge/architecture.md` holding `# Architecture\n\n## Module map\n\n- src/main.rs - entry point\n`, a root stage `make_stage("feature", "Feature")`, `Some(temp.path())`: no warning) and `a_scaffold_only_tree_still_warns` (`KnowledgeDir::new(temp.path()).initialize()`, same stage: one warning naming `knowledge-bootstrap`).
5. `tests/integration/plan_verify.rs`, `test_knowledge_warning_category_populated` (about line 804): the warning now depends on the repository that encloses the plan, which `find_repo_root` resolves by walking up from the plan to the first `.git`. Pin that repository instead of depending on what encloses the `TempDir`: replace the test's first comment line with `fs::create_dir(temp.path().join(".git")).unwrap(); // a repository root with no knowledge tree`, placed after `let temp = TempDir::new().unwrap();` (the marker `test_missing_brief_path_warns` uses). The file length stays at 1028 (ledger row).

## 2. Unit tests for the bookend lints

C1 adds two lints to `v2_lints::run`, every finding with `error_in_v2: true`. Their messages are byte-exact:

- plan-wide (`stage_id: None`): `the plan has {count} knowledge-distill stages; a plan needs exactly one, after every other stage, so the memory of every stage is curated once`
- on the distill stage: `knowledge-distill stage has dependents ({ids}); it must be the last stage of the plan`
- on a stage outside the distill stage's ancestry: ``is not an ancestor of knowledge-distill stage `{distill_id}`, so its memory is never distilled``
- on a standard stage no integration-verify stage follows: `is not an ancestor of any integration-verify stage, so its merged code is never verified`

Write them in the new file `src/plan/schema/tests/v2_lint_bookend_tests.rs`, declared in `v2_lint_tests.rs` directly after the `command_lints` declaration with the same form:

```rust
#[path = "v2_lint_bookend_tests.rs"]
mod bookend_lints;
```

Use the parent's helpers through `use super::{matching, plan, stage};` (as `v2_lint_command_tests.rs` does) and `crate::plan::schema::validation::v2_fields::split_lint_findings`. A small local helper that sets a stage's `dependencies` keeps each test short. Select findings with `matching(&metadata, None, "<needle>")`, and prove both plan versions from one finding list: `split_lint_findings(2, found.clone())` returns them as errors, `split_lint_findings(1, found)` as warnings and no errors.

1. `distill_lint_flags_a_stage_outside_its_ancestry`: bootstrap `Knowledge` → `feature` `Standard` → `integration-verify` → `knowledge-distill`, plus a root `Knowledge` stage `notes` that nothing depends on. Findings for `not an ancestor of knowledge-distill stage` name only `notes`; errors under version 2, warnings under version 1.
2. `distill_lint_counts_and_dependents`: a plan with two distill stages yields one finding with `stage_id: None` matching `knowledge-distill stages; a plan needs exactly one`; a plan where a stage depends on the distill stage yields the `has dependents` finding on the distill stage.
3. `iv_lint_flags_a_standard_stage_no_verify_stage_follows`: bootstrap → `parser` → `greeting` → `integration-verify` → distill, plus `farewell` (`Standard`, depends on bootstrap) that distill also depends on. Findings for `not an ancestor of any integration-verify stage` name only `farewell` (the transitive ancestor `parser` is clean); errors under version 2, warnings under version 1.
4. `a_conforming_plan_has_no_bookend_findings`: stages with the ids `knowledge-bootstrap` (`Knowledge`) → `feature` (`Standard`) → `integration-verify` (`IntegrationVerify`) → `knowledge-distill` (`KnowledgeDistill`), each depending on the one before; no finding matches `ancestor of`, `knowledge-distill stages` or `has dependents`. Use exactly these ids in every test of this file: test 5 relies on them, and `detect_stage_type_from_id_name` resolves an id such as `distill` to `Standard` (`src/plan/schema/detect.rs`).
5. `an_omitted_stage_type_is_resolved_by_id`: the conforming plan with `stage_type = None` on the stages whose ids are `knowledge-distill` and `integration-verify` still has no bookend finding (`detect_stage_type_from_id_name` resolves both ids).

Keep the file under 200 lines and every function under 50.

Two existing tests assert that a whole lint run is empty; they break once the lints exist. Narrow each to its own lint:

- `v2_lint_command_tests.rs`, `ordinary_wiring_pattern_is_clean`: assert that no finding's message starts with `Wiring #` (the regex lint's label the sibling tests above it use).
- `v2_lint_environment_repo_tests.rs`, `version_one_plans_and_missing_repositories_have_no_provision_findings`: assert no finding whose message contains `provision` or starts with `package \``; keep the two`notes.is_empty()` assertions.

Before finishing, run `rg -n "findings.is_empty|lint_with_notes\(|= lint\(" src/plan/schema/tests` and confirm no other test asserts an empty full run.

## Check (run once, at the end)

```bash
cargo test --lib plan::schema::
```

If only the bookend tests fail, and only because `bookends.rs` is missing, unregistered or still being written by C1, report that and stop: the main agent reruns them once C1 has returned. Report the final line count of `validation.rs`, and confirm `execute`, `initialize_with_plan` and `tests/integration/plan_verify.rs` kept their recorded counts.
