# C4: plan metadata spans

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/feat-planner-01-documents/common.md first.

## Files owned

`loom/src/planning/sections/metadata.rs`, `loom/src/planning/sections/metadata_tests.rs` (seeded; declared in sections.rs by the seed).

## Files to read

`doc/plans/briefs/feat-planner-01-documents/document-model/w2-sections.md` section 3 (the rules), `loom/src/plan/parser/extraction.rs` line 72 (`extract_yaml_metadata_with_ranges`, `ExtractedMetadata`), `loom/src/planning/sections.rs` (W2 writes it in the same wave; use exactly the pinned `pub struct Span { pub start_line: usize, pub end_line: usize, pub hash: String }` and `pub fn normalized_hash(text: &str) -> String`; the base-layer source graph does not show them, and the file may still be partial while you work, so build against the pinned names and wait and rebuild if the build fails inside it).

## Entry points

`pub(crate) fn metadata_spans(text: &str, lines: &[&str]) -> (Option<Span>, Vec<(String, Span)>)`, called by `sections_of` in sections.rs.

## Steps

1. Write `metadata.rs`: byte ranges to 0-based lines; the loom-metadata span runs from the start-marker line through the end-marker line inclusive; each stage span from its `- id:` line to the next `- id:` at the same indent or the closing fence line, exclusive; hash each span text with `normalized_hash`.
2. Write `metadata_tests.rs`: no block, two stages with the second id quoted, a CRLF document, a nested `- id:` deeper than the stage indent ignored.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::sections::metadata_tests::` and it passes.

## Traps

Do not parse YAML. A hand-written line matcher is enough; the regex crate is available if needed.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines
  (cargo fmt grows them; the hard limits are 400 and 50).
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment (the gate runs cargo doc with warnings as errors).
- Touch no file outside the Files owned list. Library code returns anyhow::Result and never unwraps
  outside tests.
