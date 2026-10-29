# C2: front matter parse and emit

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`loom/src/planning/summary/format/front.rs`, `loom/src/planning/summary/format/front_tests.rs` (seeded; format.rs declares both).

## Files to read

`doc/plans/briefs/feat-planner-04-summary-store/summary-store/w1-format.md` section "format/front.rs", `doc/plans/briefs/feat-planner-04-summary-store/common.md` "The front matter" in Format rules, `loom/Cargo.toml` (serde-saphyr is the YAML crate).

## Entry points

`#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)] pub enum SummaryKind { Document, Overview }` (serde snake_case), `pub struct FrontMatter { pub kind: SummaryKind, pub of: String, pub file: Option<String>, pub cites: BTreeMap<String, String>, pub members: BTreeMap<String, String> }`, `pub(crate) fn parse_front(yaml: &str, first_line: usize) -> Result<FrontMatter, Vec<FormatError>>`, `pub(crate) fn render_front(front: &FrontMatter) -> String` (returns the block including both --- lines). `FormatError { pub line: usize, pub message: String }` is defined by W1 in format.rs (same wave): use exactly that name and those fields.

## Steps

1. Write `front.rs`: parse with serde_saphyr::from_str into a private struct with deny_unknown_fields; emit with a hand-written writer in the fixed key order kind, of, file, cites, members, maps sorted, every cites and members key and value double-quoted with double quote and backslash escaped.
2. Write `front_tests.rs`: emit then parse gives back the same value for keys holding hash, colon and double quote; unknown key rejected with its line; a document without file rejected; render twice gives identical text.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::summary::format::front_tests::` and it passes.

## Traps

No time, model or other volatile value is ever emitted: a reseal must produce no diff.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Every read of doc/, drafts/ or claims/ goes through crate::fs::safe_read; never std::fs::read on them.
