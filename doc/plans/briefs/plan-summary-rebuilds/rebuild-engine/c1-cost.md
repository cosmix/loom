# C1: cost lines, the log and transcript tokens

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/plan-summary-rebuilds/common.md` first.

## Files owned

`loom/src/planning/summary/cost.rs`, `loom/src/planning/summary/cost/tests.rs`. The contract scaffold already created cost.rs holding `#[cfg(test)] mod contract_tests;`: keep that line, and add `pub mod estimate; #[cfg(test)] mod tests; #[cfg(test)] mod estimate_tests;` (unit C2 writes those two files in the same wave). In `loom/src/planning/summary/mod.rs` (yours) the contract scaffold declared `mod cost;`: change that one line to `pub mod cost;` (add it if absent) and change nothing else there.

## Files to read

`doc/plans/briefs/plan-summary-rebuilds/rebuild-engine/w1-cost.md` (the API and rules: TokenVector, Trigger, Level, CostLine with source_bytes, transcript_tokens, append, read_log), `doc/plans/briefs/plan-summary-rebuilds/common.md` "Cost model", `loom/src/tokens/accumulate.rs` lines 87 and 117 (`AgentState::new`, `ingest`, the `total` field), `loom/src/tokens/model.rs` line 28 (`TokenUsage`), the frozen contract file `loom/src/planning/summary/cost/contract_tests.rs`.

## Entry points

`TokenVector`, `Trigger`, `Level`, `CostLine`, `transcript_tokens(path: &Path) -> anyhow::Result<TokenVector>`, `append(paths: &RuntimePaths, line: &CostLine) -> anyhow::Result<()>`, `read_log(paths: &RuntimePaths) -> anyhow::Result<Vec<CostLine>>`.

## Steps

1. Write `cost.rs` per w1-cost.md; read whole lines only (skip a final line that has no trailing newline).
2. Write `cost/tests.rs`: absent fields omitted in JSON, two threads appending give two whole lines, a final line without a newline skipped, an unreadable transcript gives every field None.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::summary::cost::` (the contract tests included) and it passes.

## Traps

Use the accumulator, never your own usage parser, so counts match loom status. Open the log with O_APPEND, O_CREAT and O_NOFOLLOW, mode 0600, under an fs2 exclusive lock.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
