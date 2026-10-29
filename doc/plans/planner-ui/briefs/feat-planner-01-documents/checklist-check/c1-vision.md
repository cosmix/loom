# C1: the vision sections table

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/feat-planner-01-documents/common.md first; its Public API section is authoritative.

## Files owned

`loom/src/planning/vision.rs`, `loom/src/planning/tests/vision_tests.rs` (seeded with a doc line; the test module is declared in `loom/src/planning/tests/mod.rs`).

## Files to read

`doc/plans/briefs/feat-planner-01-documents/checklist-check/w1-parsers.md` section "vision.rs" (the rules), `doc/product/index.md` lines 171-180 (the real table shape), `loom/src/planning/sections.rs` (merged: `sections_of`, `DocSections`).

## Entry points

`pub struct VisionRow { pub number: u32, pub name: String, pub line: usize }`, `pub enum VisionProblem { NoHeading, NoTable { heading_line: usize }, NoNumberedRows { heading_line: usize } }`, `pub fn parse_sections_table(text: &str) -> Result<Vec<VisionRow>, VisionProblem>`; both types derive Debug, Clone, PartialEq, Eq, serde::Serialize.

## Steps

1. Write `vision.rs` per the rules.
2. Write `vision_tests.rs`: the real table shape (copy rows 01 to 03), a table under another heading, the heading inside a fence, backticked numbers, duplicate numbers (first kept), a heading with no table, a table with no numbered row.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::vision_tests::` and it passes.

## Traps

Never look outside the Specification sections heading span. Match the heading title case-insensitively (ASCII).

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
