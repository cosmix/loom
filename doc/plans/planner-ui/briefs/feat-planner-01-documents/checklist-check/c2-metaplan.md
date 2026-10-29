# C2: metaplan entries

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/feat-planner-01-documents/common.md first; its Public API section is authoritative.

## Files owned

`loom/src/planning/metaplan.rs`, `loom/src/planning/tests/metaplan_tests.rs` (seeded with a doc line; the test module is declared in `loom/src/planning/tests/mod.rs`).

## Files to read

`doc/plans/briefs/feat-planner-01-documents/checklist-check/w1-parsers.md` section "metaplan.rs" (the rules, including the numbered heading opt out), `doc/plans/metaplan-planner-ui.md` line 134 and `doc/plans/optim-token-metaplan.md` lines 110 and 387 (real heading forms), `loom/src/planning/slug.rs` (merged: `plan_slug`).

## Entry points

`pub fn is_metaplan(rel: &Path) -> bool`, `pub struct MetaplanEntry { pub heading: String, pub anchor: String, pub plan_file: Option<String>, pub slug: Option<String>, pub line: usize }`, `pub fn entries(text: &str) -> Vec<MetaplanEntry>`.

## Steps

1. Write `metaplan.rs`: a numbered level-2 heading naming no plan file is returned with plan_file None UNLESS its text holds `no plan file` (ASCII case-insensitive); unnumbered headings without a plan file are not returned.
2. Write `metaplan_tests.rs`: every lifecycle spelling inside the backticks, both real heading forms above, `## 6. Policy notes (no plan file)` not returned, `## 3. Missing name` returned with None, a heading inside a fence ignored.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::metaplan_tests::` and it passes.

## Traps

The anchor is the HeadingSection anchor from sections_of (GitHub form with repeat numbering), never recomputed here.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
