# C3: Implements lines

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/feat-planner-01-documents/common.md first; its Public API section is authoritative.

## Files owned

`loom/src/planning/implements.rs`, `loom/src/planning/tests/implements_tests.rs` (seeded with a doc line; the test module is declared in `loom/src/planning/tests/mod.rs`).

## Files to read

`doc/plans/briefs/feat-planner-01-documents/checklist-check/w1-parsers.md` section "implements.rs", `doc/product/section_03.md` "Product-to-plan links".

## Entry points

`pub enum Implements { Unmapped, NoneDeclared, Sections(Vec<u32>) }`, `pub struct ImplementsLine { pub value: Implements, pub line: Option<usize>, pub duplicate_lines: Vec<usize>, pub bad_tokens: Vec<String> }`, `pub fn parse(text: &str) -> ImplementsLine`; derive Debug, Clone, PartialEq, Eq, serde::Serialize.

## Steps

1. Write `implements.rs`: only lines before the loom METADATA start marker and outside fences (use `crate::fs::knowledge::splice::fence_mask`, pub(crate) since stage document-model).
2. Write `implements_tests.rs`: the six cases of the frozen contract implements_token_forms plus an empty value and `NONE` in capitals.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::implements_tests::` and it passes.

## Traps

A link token counts when its text OR its target file stem is section_<digits>. The first line wins; later lines go to duplicate_lines only.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
