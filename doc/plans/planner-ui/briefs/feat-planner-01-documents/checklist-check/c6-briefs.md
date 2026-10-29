# C6: plan brief grouping

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/feat-planner-01-documents/common.md first; its Public API section is authoritative.

## Files owned

`loom/src/planning/briefs.rs`, `loom/src/planning/tests/briefs_tests.rs` (seeded with a doc line; the test module is declared in `loom/src/planning/tests/mod.rs`).

## Files to read

`doc/plans/briefs/feat-planner-01-documents/checklist-check/w1-parsers.md` section "briefs.rs", `doc/product/section_03.md` "Briefs", `loom/src/plan/inputs.rs` line 89 (`collect_inputs`, pub(crate), returns `Inputs { files, invalid }`).

## Entry points

`pub struct PlanBriefs { pub by_stage: BTreeMap<String, Vec<PathBuf>>, pub common: Vec<PathBuf>, pub other: Vec<PathBuf>, pub broken: Vec<String> }`, `pub fn plan_briefs(root: &Path, entry: &PlanEntry, stage_ids: &[String]) -> anyhow::Result<PlanBriefs>`, `pub fn orphan_bundles(tree: &DocTree) -> Vec<String>`.

## Steps

1. Write `briefs.rs` calling `collect_inputs`, never reimplementing it.
2. Write `briefs_tests.rs` covering the frozen contract brief_bundle_grouping case and an input path that does not exist (broken).
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::briefs_tests::` and it passes.

## Traps

Build fixture brief paths with format! from the briefs directory; never write a literal path under doc/plans/briefs into a test, since plan verify scans plan inputs for such tokens.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
