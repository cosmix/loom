# C3: summary wire types

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/plan-summary-rebuilds/common.md` first.

## Files owned

`loom/src/plan_server/wire/summaries.rs`, `loom/src/plan_server/wire/tests_summaries.rs`. The seed declared `pub mod summaries;` in wire/mod.rs; W3 (same wave) adds `#[cfg(test)] mod tests_summaries;` there.

## Files to read

`doc/plans/briefs/plan-summary-rebuilds/common.md` section "Wire" (the shapes), `doc/plans/briefs/plan-summary-rebuilds/rebuild-engine/w3-routes.md` section "Wire", plan 2 merged: an existing `loom/src/plan_server/wire/tests_*.rs` parity test (the pattern) and `loom/src/quota/model.rs` (QuotaSnapshot), C1 output `loom/src/planning/summary/cost.rs` (CostLine, TokenVector; new in this stage).

## Entry points

`SummaryPanel`, `RunningRebuild`, `FailedSummary`, `SummaryQuestion`, `SummaryTurnCost`, `TurnCostRow` with serde Serialize and Deserialize, snake_case, every Option field skip_serializing_if is_none.

## Steps

1. Write `wire/summaries.rs` with the structs of the Wire section, including terminal_claims and start_error on SummaryPanel.
2. Write `wire/tests_summaries.rs`: for each of the three fixtures `web/src/plan/api/fixtures/summary-panel.json`, `summary-question.json`, `summary-turn-cost.json` (W3 writes them in this wave; include_str! from the crate) a fixture_matches_serde_output test (deserialize, serialize, serde_json::Value equality) and a fixture_deserializes test.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib plan_server::wire::` and it passes.

## Traps

The PlanMessage variants live in message.rs, which W3 owns; do not add them here.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
