# C7: cross-references

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/plan-documents/common.md first; its Public API section is authoritative.

## Files owned

`loom/src/planning/xref.rs`, `loom/src/planning/tests/xref_tests.rs` (seeded with a doc line; the test module is declared in `loom/src/planning/tests/mod.rs`).

## Files to read

`doc/plans/briefs/plan-documents/checklist-check/w1-parsers.md` section "xref.rs", `doc/product/section_03.md` "Cross-references", `loom/src/plan/schema/structural_checks.rs` (unit C5 added `extract_prefixed_paths(text, prefix)` in the previous wave; the base-layer graph does not show it).

## Entry points

`pub enum RefForm { RelativeLink, PathToken, PlanFile }`, `pub struct Reference { pub form: RefForm, pub raw: String, pub line: usize, pub target: Option<(DocId, Option<String>)> }`, `pub fn references(doc_rel: &Path, text: &str, tree: &DocTree, sections: &dyn Fn(&Path) -> Option<DocSections>) -> Vec<Reference>`.

## Steps

1. Write `xref.rs` per the rules; it stays pure (the sections lookup is a parameter).
2. Write `xref_tests.rs`: each form resolving, each form broken, a code link skipped, a placeholder skipped, a glob token skipped, a fragment that is not an anchor, a pure fragment resolved against the document itself.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::xref_tests::` and it passes.

## Traps

Resolve relative links lexically; a result outside doc/ is a code link and is skipped, never reported broken.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
