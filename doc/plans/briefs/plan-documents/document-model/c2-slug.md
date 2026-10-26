# C2: plan slugs and resolution

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/plan-documents/common.md first.

## Files owned

`loom/src/planning/slug.rs`, `loom/src/planning/tests/slug_tests.rs` (seeded; the test module is declared).

## Files to read

`doc/plans/briefs/plan-documents/document-model/w1-discovery.md` section 2 (the rules), `doc/product/section_03.md` "Plans and their lifecycle", `loom/src/fs/plan_lifecycle.rs` lines 23-24 (the prefix constants; W1 adds `RETIRED_PREFIX` there in a later wave, so write the three prefixes as local consts in slug.rs with a comment naming plan_lifecycle as their origin).

## Entry points

`PlanState`, `plan_slug`, `plan_state`, `PlanEntry`, `resolve_plans(paths: impl Iterator<Item = PathBuf>) -> BTreeMap<String, PlanEntry>`, as common.md "Plan slugs" and w1-discovery.md section 2 pin them.

## Steps

1. Write `slug.rs` with `PlanState` deriving Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize (`rename_all = "snake_case"`), in the order Written, Running, Done, Retired.
2. Write `slug_tests.rs`: every spelling of the lifecycle table, the tie rule (two done copies: the smaller path wins, the other is a stray), `DONE-PLAN-DONE-x.md` gives slug `DONE-x`, `PLAN-.md` and `REVIEW-PLAN-a.md` give None, a plan directly in doc/plans/archive without a prefix is Done.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::tests::slug_tests::` and it passes (the frozen contracts plan_slug_most_advanced_wins and plan_slug_strips_every_prefix run later against the same code).

## Traps

Strip at most ONE status prefix. Only files directly in doc/plans or doc/plans/archive are plans.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines
  (cargo fmt grows them; the hard limits are 400 and 50).
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment (the gate runs cargo doc with warnings as errors).
- Touch no file outside the Files owned list. Library code returns anyhow::Result and never unwraps
  outside tests.
