# concept-data C2: out-of-date reason

Codex unit (gpt-6.1-terra, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/concept/state.rs`; `src/plan_server/concept/state_tests.rs`
- May read: `src/plan_server/wire/concept.rs (written by unit C1 in this stage: OutOfDate and its reason enum)`; `src/plan_server/concept/page.rs is NOT written yet; PageContext is defined there later, so take it by path crate::plan_server::concept::PageContext and only read its field read_only: Option<String>`; `the plan 4 module planning::summary::evaluate (SummaryState)`

## Entry points

pub fn out_of_date_reason(id: &str, state: &SummaryState, ctx: &PageContext) -> Option<OutOfDate>

## Steps

1. Write the function as one match: Current gives None; Rebuilding gives rebuilding; Missing gives None; every other SummaryState variant (Stale and the partly-current variant) gives reason source_changed.
2. When ctx.read_only is Some(reason), turn every Some result into reason cannot_rebuild with detail Some(reason).
3. In state_tests.rs (declared by the orchestrator in mod.rs as a cfg(test) module; start it with use super::state::*;) write five tests named current_gives_none, stale_states_give_source_changed, rebuilding_gives_rebuilding, missing_gives_none, read_only_turns_reasons_into_cannot_rebuild; build PageContext with ..Default::default().

## Done

The function and its five tests compile; the id parameter is accepted and unused for now (prefix it with an underscore and add a rustdoc line saying plan 10 reads it).

Proof (the orchestrator runs it after you): `cargo test --lib plan_server::concept::state_tests::`

## Traps

PageContext lands in page.rs in a later wave. If it does not exist when you compile, the orchestrator has added a placeholder struct with one field read_only; do not define your own.

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
