# C4: pressure records and open questions

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/feat-planner-01-documents/common.md first; its Public API section is authoritative.

## Files owned

`loom/src/planning/pressure.rs`, `loom/src/planning/tests/pressure_tests.rs` (seeded with a doc line; the test module is declared in `loom/src/planning/tests/mod.rs`).

## Files to read

`doc/plans/briefs/feat-planner-01-documents/checklist-check/w1-parsers.md` section "pressure.rs", `doc/product/section_01.md` "Pressure tests".

## Entry points

`pub struct PressureRecord { pub date: String, pub line: usize, pub text: String }`, `pub fn records(text: &str) -> Vec<PressureRecord>`, `pub fn open_questions(text: &str) -> usize`.

## Steps

1. Write `pressure.rs`.
2. Write `pressure_tests.rs`: two records, a malformed date skipped, a record outside the heading ignored, open questions under two headings summed, nested list items not counted, numbered items counted.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::pressure_tests::` and it passes.

## Traps

A date is exactly four digits, dash, two digits, dash, two digits; validate month 01 to 12 and day 01 to 31.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
