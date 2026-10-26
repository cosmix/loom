# C1: document kinds and ids

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/plan-documents/common.md first.

## Files owned

`loom/src/planning/kinds.rs`, `loom/src/planning/tests/kinds_tests.rs` (both seeded with a one-line doc comment; the test file is already declared in `loom/src/planning/tests/mod.rs`).

## Files to read

`doc/plans/briefs/plan-documents/document-model/w1-discovery.md` section 3 (the kinds table: the rules you implement), `doc/product/section_03.md` "Document kinds" and "Headings as anchors" (Document id), `loom/src/planning/slug.rs` (unit C2 writes it in the same wave: use only the pinned items below).

## Entry points

`DocKind`, `classify(rel: &Path) -> DocKind`, `DocId`, `doc_id(rel: &Path, kind: &DocKind) -> DocId`, exactly as common.md "Kinds and ids" pins them. From C2 you may call `crate::planning::slug::{plan_slug(file_name: &str) -> Option<String>, plan_state(rel: &Path) -> Option<PlanState>}` and use `PlanState` (derives Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, Serialize).

## Steps

1. Write `kinds.rs`: the enum with `#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]` and `#[serde(tag = "kind", rename_all = "snake_case")]`, `DocId(pub String)` with Serialize, `classify` following the table row order of w1-discovery.md section 3 (first match wins), `doc_id`.
2. Write `kinds_tests.rs`: one test per table row, the id rules (two-digit zero pad for sections, `plan/<slug>` for every plan spelling, the path under doc/ otherwise), a plan in a subdirectory of doc/plans that is not archive (Other), and a REVIEW file under archive.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::kinds_tests::` and it passes.

## Traps

Paths are repo-relative and start with doc/. Compare path components, never string prefixes (doc/productx is not doc/product). A metaplan is only directly in doc/plans. While C2 is unwritten your build may fail inside slug.rs: wait and rebuild, never edit it.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines
  (cargo fmt grows them; the hard limits are 400 and 50).
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment (the gate runs cargo doc with warnings as errors).
- Touch no file outside the Files owned list. Library code returns anyhow::Result and never unwraps
  outside tests.
