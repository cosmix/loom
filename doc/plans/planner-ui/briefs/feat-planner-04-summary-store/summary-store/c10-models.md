# C10: the summary model resolver

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`loom/src/planning/summary/config.rs` and `loom/src/planning/summary/config/tests.rs` (config.rs is seeded with a doc line; declare `#[cfg(test)] mod tests;` in it).

## Files to read

`doc/plans/briefs/feat-planner-04-summary-store/summary-store/w4-config.md` step 6 of "Mirror [pressure] exactly", `loom/src/commands/pressure/models.rs` lines 30-70 (the slot pattern), `loom/src/commands/pressure/mod.rs` lines 300-310 (project tier through WorkDir::new), and C8 and C9 output from the previous wave (`loom/src/user_config/plan.rs`, `loom/src/fs/work_dir/config_sections/plan_config.rs`; the base-layer graph does not show them).

## Entry points

`pub struct SummaryModels { pub document_model: String, pub document_effort: String, pub overview_model: String, pub overview_effort: String }`, `impl SummaryModels { pub fn resolve(project: &PlanConfig, user: &UserConfig) -> SummaryModels; pub fn load(root: &Path) -> anyhow::Result<SummaryModels> }`.

## Steps

1. Write `config.rs`: project value, else user value (which already falls back to its default).
2. Write the unit tests in config/tests.rs: a non-default value at each tier wins as specified.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::summary::config` and `cargo test --test planning_summary_cli_contracts plan_summary_keys_resolve`.

## Traps

load reads .loom/work only when WorkDir::new(root) resolves; an absent workspace is no project tier, not an error.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Every read of doc/, drafts/ or claims/ goes through crate::fs::safe_read; never std::fs::read on them.
