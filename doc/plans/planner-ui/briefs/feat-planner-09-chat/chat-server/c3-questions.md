# chat-server C3: open-question counts

Codex unit (gpt-6.1-terra, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/review/questions.rs`; `src/plan_server/review/questions_tests.rs`
- May read: `the plan 1 module planning::project (ProjectDoc: kind via doc.kind, sections, open_questions)`

## Entry points

pub fn open_question_counts(project: &Project) -> BTreeMap<String, usize>

## Steps

1. For every ProjectDoc of kind ProductSection, Metaplan or Plan whose sections hold a heading titled Open questions (case-sensitive), insert its repo-relative path with its open_questions count (0 kept).
2. In questions_tests.rs (declared by the orchestrator in mod.rs as a cfg(test) module; start it with use super::questions::*;) write tests zero_count_kept_for_empty_heading, documents_without_the_heading_omitted, using Project::load over a tempfile::TempDir tree.

## Done

The function and two tests compile.

Proof (the orchestrator runs it after you): `cargo test --lib plan_server::review::questions_tests::`

## Traps

Keys are repo-relative paths as strings (doc/product/section_02.md).

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
