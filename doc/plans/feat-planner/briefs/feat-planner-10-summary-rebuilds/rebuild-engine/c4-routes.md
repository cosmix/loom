# C4: the summaries routes

Codex unit (gpt-6.1-sol, effort high). Read `doc/plans/briefs/feat-planner-10-summary-rebuilds/common.md` first.

## Files owned

`loom/src/plan_server/summaries/routes.rs`, `loom/src/plan_server/summaries/routes/tests.rs` (the seed declared `pub mod routes;` in summaries/mod.rs; declare `#[cfg(test)] mod tests;` in routes.rs).

## Files to read

`doc/plans/briefs/feat-planner-10-summary-rebuilds/rebuild-engine/w3-routes.md` section "Routes" (the four routes, refusals and status codes), plan 2 merged: `RouteSpec`, `ApiRequest`, `ApiResponse`, an existing route module and its tests (the pattern), `loom/src/plan_server/summaries/rebuilds.rs` (W2 wrote it in the previous wave: `Rebuilds::{panel, enqueue, release_terminal_claims}` and the answer call; the base-layer graph does not show it), C3 output `loom/src/plan_server/wire/summaries.rs`.

## Entry points

`pub fn routes() -> Vec<RouteSpec>` with GET /api/summaries/panel, POST /api/summaries/rebuild, POST /api/summaries/release, POST /api/summaries/answer.

## Steps

1. Write `routes.rs`: ids validated with `crate::planning::summary::store::validate_id` and checked against `expected_ids` (at most 64); the right to act is holder, or anyone while nobody holds authoring; read-only mode refused with 409 read_only; taking authoring is never a side effect.
2. Write `routes/tests.rs` with every refusal of the Routes section, through the route test helpers plan 2 uses in its own route tests.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib plan_server::summaries::` and it passes (these tests bind a loopback port; the stage sandbox allows it).

## Traps

The handler never calls the scheduler before the role and read-only checks pass.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
