# C5: prefixed path tokens

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/plan-documents/common.md first; its Public API section is authoritative.

## Files owned

`loom/src/plan/schema/structural_checks.rs` only.

## Files to read

`loom/src/plan/schema/structural_checks.rs` lines 30-60 (`extract_brief_paths`) and its existing tests in the same file or module.

## Entry points

Add `pub(crate) fn extract_prefixed_paths(text: &str, prefix: &str) -> Vec<String>`; `extract_brief_paths(text)` becomes a call to it with `"doc/plans/briefs/"`.

## Steps

1. Move the body of `extract_brief_paths` into `extract_prefixed_paths`, replacing the PREFIX const with the parameter; keep every rule (delimiters, trimming, the backtick and hash cut, placeholder skip).
2. Add two tests next to the existing ones: the doc/ prefix finds `doc/product/section_01.md` in prose, and the brief prefix result is unchanged on a sample text.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib plan::schema::` and every existing test passes unchanged.

## Traps

Behaviour for the brief prefix must not change by one byte: plan verify and loom run depend on it.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
