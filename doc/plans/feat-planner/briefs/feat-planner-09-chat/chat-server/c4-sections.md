# chat-server C4: changed sections of a turn

Codex unit (gpt-6.1-sol, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/review/sections.rs`; `src/plan_server/review/sections_tests.rs`
- May read: `the plan 1 function planning::sections::sections_of and DocSections`

## Entry points

pub(crate) fn turn_sections(start: &str, end: &str) -> Vec<String>

## Steps

1. Compute sections_of for both texts; return the anchors (the preamble as the empty string) whose span hash differs between the two, or that exist in only one, in the order they appear in end then start.
2. In sections_tests.rs (declared by the orchestrator in mod.rs as a cfg(test) module; start it with use super::sections::*;) write tests changed_heading_listed, unchanged_heading_omitted, added_and_removed_headings_listed, preamble_change_listed_as_empty_anchor.

## Done

The function and four tests pass.

Proof (the orchestrator runs it after you): `cargo test --lib plan_server::review::sections_tests::`

## Traps

Compare span hashes (Span.hash), never raw line ranges: a section that only moved is unchanged.

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
