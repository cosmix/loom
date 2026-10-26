# C8: the user-tier plan section

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/plan-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`loom/src/user_config/plan.rs` and `loom/src/user_config/plan/tests.rs` (both new; declare `#[cfg(test)] mod tests;` in plan.rs; W4 declares `mod plan;`, adds the `plan: plan::PlanSection` field to UserConfig, and calls `PlanSection::parse` and `plan_value_of` in the same wave).

## Files to read

`doc/plans/briefs/plan-summary-store/summary-store/w4-config.md` sections "Keys" and step 2 of "Mirror [pressure] exactly", `loom/src/user_config/pressure.rs` (the pattern to copy, lines 1-169).

## Entry points

`pub(super) struct PlanSection` (four Option<String> fields document_summary_model, document_summary_effort, overview_model, overview_effort) with `pub(super) fn parse(doc: &DocumentMut) -> Result<Self>`; on UserConfig: `plan_document_summary_model()`, `plan_document_summary_effort()`, `plan_overview_model()`, `plan_overview_effort()` returning &str, `pub(super) fn plan_value_of(&self, spec: &KeySpec) -> Option<(ConfigValue, Origin)>`, `pub(super) fn plan_toml(&self) -> String`; consts `DEFAULT_PLAN_DOCUMENT_SUMMARY_MODEL` (haiku), `DEFAULT_PLAN_DOCUMENT_SUMMARY_EFFORT` (medium), `DEFAULT_PLAN_OVERVIEW_MODEL` (sonnet), `DEFAULT_PLAN_OVERVIEW_EFFORT` (high).

## Steps

1. Write `plan.rs` mirroring pressure.rs: the section is `plan` in ~/.loom/config.toml, models validated against crate::claude::CLAUDE_MODELS, efforts against crate::models::stage::ALLOWED_REASONING_EFFORTS.
2. Write plan/tests.rs: defaults, a set value, an unknown variant rejected naming the key, and plan_toml listing all four keys. Keep each list easy to extend: plan 8 appends four more keys later.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib user_config::` after W4 wires the field.

## Traps

The user tier is the [plan] section of ~/.loom/config.toml; the project tier is a different table (C9). Do not touch any other file.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Every read of doc/, drafts/ or claims/ goes through crate::fs::safe_read; never std::fs::read on them.
