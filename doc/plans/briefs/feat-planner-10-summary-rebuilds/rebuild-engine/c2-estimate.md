# C2: estimates and totals

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/feat-planner-10-summary-rebuilds/common.md` first.

## Files owned

`loom/src/planning/summary/cost/estimate.rs`, `loom/src/planning/summary/cost/estimate_tests.rs` (C1 declares both in cost.rs in the same wave).

## Files to read

`doc/plans/briefs/feat-planner-10-summary-rebuilds/rebuild-engine/w1-cost.md` (estimate rules), `doc/plans/PLAN-feat-planner-10-summary-rebuilds.md` Decision 8 (starting figures 40000 and 150000), the frozen contract estimate_uses_log_or_starting_figure in `loom/src/planning/summary/cost/contract_tests.rs`.

## Entry points

`pub enum EstimateBasis { Last, PerByte, Starting }`, `pub struct Estimate { pub tokens: u64, pub per_summary: Vec<(String, u64)>, pub basis: EstimateBasis }`, `pub fn estimate(log: &[CostLine], pending: &[(String, Level, u64)]) -> Estimate`, `pub struct Totals { pub today: TokenVector, pub week: TokenVector }`, `pub fn totals(log: &[CostLine], now: chrono::DateTime<chrono::Local>) -> Totals`. `CostLine`, `Level`, `TokenVector` come from C1 in cost.rs (pinned in w1-cost.md; the base-layer graph does not show them).

## Steps

1. Write `estimate.rs`: per summary its last recorded fresh tokens; else the per-level median of fresh tokens per source byte times its source bytes; else the starting figure for its level. The overall basis is the weakest basis used.
2. Write `estimate_tests.rs`: each basis, a median over three lines, a day boundary for totals, the week window.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::summary::cost::` and it passes.

## Traps

Fresh tokens are input plus cache writes plus output; cache reads never count toward the estimate.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
