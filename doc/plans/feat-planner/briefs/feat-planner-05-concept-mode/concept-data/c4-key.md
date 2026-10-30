# concept-data C4: document key chips

Codex unit (gpt-6.1-terra, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/concept/key.rs`; `src/plan_server/concept/key_tests.rs`
- May read: `src/plan_server/wire/concept.rs (unit C1)`; `src/plan_server/concept/state.rs (unit C2 in the same wave writes out_of_date_reason; call it by that name)`; `the plan 4 function planning::summary::store::scope`

## Entry points

pub(crate) fn key_chips(project: &Project, eval: &Evaluation, scope: Scope, ctx: &PageContext) -> Vec<KeyChip>; pub(crate) fn colour_of(chips: &[KeyChip], doc_path: &str) -> Option<u8>

## Steps

1. key_chips: one chip per summary id in store::scope(project).product (Scope::Product) or .engineering (Scope::Engineering), in that order; colour (index % 8 + 1) as u8; label the two-digit section number or the plan slug; doc from the source document; out_of_date from state::out_of_date_reason(id, state, ctx).
2. no_plan is true for a product chip whose number has no entry in project.implemented_by, and only when the engineering scope is not empty. colour_of returns the colour of the chip whose doc.path equals doc_path.
3. In key_tests.rs (declared by the orchestrator in mod.rs as a cfg(test) module; start it with use super::key::*;) write tests chips_follow_scope_order, colours_repeat_after_eight, no_plan_only_with_an_engineering_scope, chip_carries_colour_label_and_title.

## Done

Both functions and four tests compile.

Proof (the orchestrator runs it after you): `cargo test --lib plan_server::concept::key_tests::`

## Traps

Scope here is crate::plan_server::concept::Scope (defined in page.rs in a later wave; the orchestrator adds a placeholder enum with Product and Engineering if it is missing). Do not name plan 4 Scope struct and this enum the same in one scope: import plan 4 one as SummaryScope.

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
