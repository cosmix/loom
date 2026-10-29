# C4: the status report

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`loom/src/planning/summary/status.rs`, `loom/src/planning/summary/status/tests.rs` (seeded).

## Files to read

`doc/plans/briefs/feat-planner-04-summary-store/summary-store/w3-commands.md` section "status.rs", `doc/plans/briefs/feat-planner-04-summary-store/common.md` Public API "Commands" (the status JSON shape), `loom/src/planning/summary/evaluate.rs` and `claims.rs` (W2 wrote them in the previous wave).

## Entry points

`#[derive(serde::Serialize)] pub struct StatusReport` with the JSON fields of common.md, `pub fn status_report(root: &Path, paths: &RuntimePaths) -> anyhow::Result<StatusReport>`, `pub fn render_status(report: &StatusReport) -> String` (human form).

## Steps

1. Write `status.rs`: summaries in expected_ids order then orphans; each state is the snake_case name of its SummaryState variant.
2. Write `status/tests.rs`: the JSON key set on a fixture project with one sealed summary, a claimed summary showing its session, the human line format.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::summary::status::` and `cargo test --test planning_summary_cli_contracts summary_cli_status_json`.

## Traps

Use the SummaryState serde names; never hand-write state strings that could drift from the enum.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Every read of doc/, drafts/ or claims/ goes through crate::fs::safe_read; never std::fs::read on them.
