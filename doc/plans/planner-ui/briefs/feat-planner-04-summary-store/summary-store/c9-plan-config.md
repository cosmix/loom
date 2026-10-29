# C9: the project-tier planning table

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`loom/src/fs/work_dir/config_sections/plan_config.rs` and `loom/src/fs/work_dir/config_sections/plan_config/tests.rs` (both new; declare `#[cfg(test)] mod tests;` in plan_config.rs; W4 adds `mod plan_config;` and the re-export in config_sections.rs in the same wave).

## Files to read

`doc/plans/briefs/feat-planner-04-summary-store/summary-store/w4-config.md` step 4 of "Mirror [pressure] exactly", `loom/src/fs/work_dir/config_sections/pressure_config.rs` (the pattern, lines 1-160), `loom/src/fs/work_dir/config_sections.rs` lines 11-12 (the run-owned plan table you must NOT read).

## Entry points

`pub struct PlanConfig` (four Option<String> fields, serde Deserialize, deny_unknown_fields) with `pub fn from_section_toml(body: &str) -> anyhow::Result<Self>`, validated accessors `document_summary_model()`, `document_summary_effort()`, `overview_model()`, `overview_effort()` returning Option<&str>, and `pub fn read_plan_config(work_dir: &Path) -> PlanConfig` reading the `[planning]` table (const PLANNING_SECTION = "planning").

## Steps

1. Write `plan_config.rs` mirroring pressure_config.rs but reading the planning table.
2. Write the unit tests in plan_config/tests.rs (the cases pressure_config.rs covers in-file): a file holding the run plan table plus a planning table resolves the planning values; a missing file gives defaults; an unknown key in planning is rejected.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib fs::work_dir::` after W4 wires the module.

## Traps

Reading the plan table here would make the project tier dead whenever a run exists: only planning.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Every read of doc/, drafts/ or claims/ goes through crate::fs::safe_read; never std::fs::read on them.
