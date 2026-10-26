# C5: the loom plan summary command module

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/plan-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`loom/src/commands/plan/summary.rs` and `loom/src/commands/plan/summary/tests.rs` (declare `#[cfg(test)] mod tests;` in summary.rs; W3 declares `pub mod summary;` in commands/plan/mod.rs and the SummaryCommands enum in cli/types_ops.rs in the same wave).

## Files to read

`doc/plans/briefs/plan-summary-store/summary-store/w3-commands.md` section "CLI" (exit codes and the root resolution), `loom/src/commands/plan/check.rs` (merged plan 1: the same root resolution), `loom/src/planning/runtime.rs` (`runtime_paths`).

## Entry points

`pub fn execute(command: SummaryCommands) -> anyhow::Result<()>` where `SummaryCommands::{Seal { id: String, force: bool }, Brief { ids: Vec<String>, note: Option<String> }, Status { json: bool }, Release { ids: Vec<String> }}` comes from `crate::cli::types_ops` (pinned; W3 writes it); it calls `seal::seal(root, paths, id, force)`, `brief::brief`, `status::{status_report, render_status}`, `claims::release`.

## Steps

1. Write `summary.rs`: resolve the root, paths through runtime_paths, dispatch the subcommand, print, and exit 0, 1 (a rejected seal printing each absolute-draft-path:line: message to stderr, a claimed brief, a refused release) or 2 (any other error).
2. Write summary/tests.rs for the pure helpers (exit code mapping, the draft-line error formatting); keep every process exit inside execute.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib commands::plan::summary::` (this unit tests) and, after W3 wires the dispatch arm, `cargo test --test planning_summary_cli_contracts`.

## Traps

Never resolve the runtime dir yourself: runtime_paths applies the LOOM_PLAN_STATE rules.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Every read of doc/, drafts/ or claims/ goes through crate::fs::safe_read; never std::fs::read on them.
