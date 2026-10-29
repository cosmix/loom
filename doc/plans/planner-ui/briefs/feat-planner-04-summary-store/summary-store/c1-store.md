# C1: store layout, ids, scope and link helpers

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`loom/src/planning/summary/store.rs`, `loom/src/planning/summary/store/tests.rs` (seeded; store.rs declares the test module).

## Files to read

`doc/plans/briefs/feat-planner-04-summary-store/summary-store/w1-format.md` section "store.rs" (the rules), `doc/product/section_04.md` "The store", plan 1 merged: `loom/src/planning/project.rs` (`Project`, `tree.plans`, `metaplans`, `vision_rows`), `loom/src/planning/kinds.rs`.

## Entry points

`STORE_DIR`, `summary_path(id)`, `summary_id_for(project, rel)`, `Scope`, `scope(project)`, `expected_ids(project)`, `validate_id(id) -> anyhow::Result<()>`, `source_of(project, id) -> Option<PathBuf>`, `link_from_summary(summary_id, target_rel) -> String`, `resolve_link(summary_id, href) -> Option<(PathBuf, Option<String>)>`, as common.md pins them.

## Steps

1. Write `store.rs`.
2. Write `store/tests.rs` with every case w1-format.md lists for store.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::summary::store::` and `cargo test --test planning_summary_cli_contracts summary_id_rejects_traversal`, and both pass. `validate_id` is security relevant: every draft, claim and store path is built from the id, and the frozen contract summary_id_rejects_traversal pins its refusals; integration-verify security review reads this file.

## Traps

W1 (Claude, same wave) calls resolve_link and link_from_summary from format.rs: keep the pinned signatures. Ids use forward slashes; validate before building any path.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Every read of doc/, drafts/ or claims/ goes through crate::fs::safe_read; never std::fs::read on them.
