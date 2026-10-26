# C8: the loom plan check command

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/plan-documents/common.md first; its Public API section is authoritative.

## Files owned

`loom/src/commands/plan/check.rs`, `loom/src/commands/plan/check/tests.rs` (seeded; the seed declares the test module in check.rs).

## Files to read

`doc/plans/briefs/plan-documents/checklist-check/w3-command.md` section "check.rs" (the behaviour, exit codes and output), `doc/plans/briefs/plan-documents/checklist-check/common.md` section "JSON", `loom/src/planning/project.rs` (W2 wrote it in the previous wave: `Project::load`, `Project.checklist`, `Project.problems`), `loom/src/git/runner.rs` line 178 (`run_git_checked`), `loom/src/context/untrusted.rs` line 123 (`inline_safe`).

## Entry points

`pub fn execute(json: bool) -> anyhow::Result<()>` and the pure `pub(crate) fn report(project: &Project) -> CheckReport` (serde Serialize, fields schema, root, checklist, problems).

## Steps

1. Write `check.rs`: report, the human renderer through inline_safe, and execute with the exit calls only in execute.
2. Write `check/tests.rs`: the JSON key set of report on a fixture project, and the human renderer stripping an escape byte in a heading title.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib commands::plan::` and, after W3 wires the arm, `cargo test --test planning_check_cli_contracts`.

## Traps

Exit code 2 when the root cannot be found or Project::load fails, 1 when problems is non-empty, otherwise return Ok. Print JSON with serde_json::to_string_pretty.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
