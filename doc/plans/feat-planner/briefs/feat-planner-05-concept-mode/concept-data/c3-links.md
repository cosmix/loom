# concept-data C3: Implements links

Codex unit (gpt-6.1-terra, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/concept/links.rs`; `src/plan_server/concept/links_tests.rs`
- May read: `src/plan_server/wire/concept.rs (unit C1: ImplementsView, SectionLink, PlanLink, DocLink)`; `the plan 1 module planning::project (Project fields implements, implemented_by, docs, tree.plans) and planning::implements (Implements, ImplementsLine)`

## Entry points

pub(crate) fn implements_view(project: &Project, slug: &str) -> ImplementsView; pub(crate) fn implemented_by(project: &Project, number: u32) -> Vec<PlanLink>

## Steps

1. implements_view: Implements::Unmapped gives NotStated; NoneDeclared gives Sections with no links; Sections(numbers) gives one SectionLink per number, doc None when no product section file section_<nn>.md exists (name is then the literal text section_<nn>), else name is that document title from ProjectDoc.title; then one SectionLink { number: None, name: token, doc: None } per bad_tokens entry.
2. implemented_by: the slugs in project.implemented_by for the number, each as PlanLink { slug, title, state } with title from the plan document and state as the lowercase PlanState word, in slug order; an absent number gives an empty Vec.
3. In links_tests.rs (declared by the orchestrator in mod.rs as a cfg(test) module; start it with use super::links::*;) write tests unmapped_is_not_stated, none_declared_is_empty, bad_token_kept_as_written, implemented_by_in_slug_order, building the Project with planning::project::Project::load over a tempfile::TempDir fixture tree.

## Done

Both functions and four tests compile.

Proof (the orchestrator runs it after you): `cargo test --lib plan_server::concept::links_tests::`

## Traps

DocLink.path is repo-relative (doc/product/section_02.md). DocLink.page is the page key product/section_02 or engineering/<slug>. The tests need a TempDir, which your sandbox cannot run; compile them only.

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
