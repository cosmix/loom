# R1: drop Compare spec with build, rename the round offer to Plan follow-ups (R2, R3)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first. Knowledge: `architecture/planning-document-model.md` ("Project View and Checklist"). Line numbers below are from the stage base; anchor every edit by symbol and snippet.

## Settled decisions

- `Offer::RefineSpec` and the `loom-plan:retro` skill are deleted everywhere. Nothing replaces them: integration-verify now checks the spec (S1).
- `Offer::NewMetaplanRound` becomes `Offer::PlanFollowUps`: still a unit variant (no feature field; the screen or the request names the feature, as before). Id `plan-follow-ups`, wire kind `plan_follow_ups`, label `Plan follow-ups`.
- `CurrentStep::Ready` offers only `StartRun`; `CurrentStep::ReadyAllDone` offers `[Offer::PlanFollowUps]` as its sole secondary and no primary.
- Opening for `PlanFollowUps` (three lines, through `three_lines`): step `done, Plan follow-ups`, target `Append follow-up entries under "## Follow-ups" in doc/plans/<f>/index.md.` (format it from `plans.display()` as `StartMetaplan`'s target does at opening.rs ~137; there is no metaplan-path helper and none is needed), skill `metaplan`. `step_class` puts it in `StepClass::Plan` (the default arm already does). `display_name` gives `loom plan: metaplan`.
- One metaplan per feature (R2 changes the classifier in parallel): `doc/plans/<f>/index.md` is the only metaplan; any other `*metaplan*.md` is an `Other` document. In the test files you own, a fixture that writes a second metaplan file (`metaplan-02.md`, `metaplan-04.md`, `round-2-metaplan.md`) is rewritten: fold its entries into `index.md` after a `## Follow-ups` heading (entries stay level-2 headings, `## <n>. <Title> (`PLAN-<slug>.md`)`), or delete the test when its only point was choosing between two metaplans. A `WritePlan.metaplan` value that is never classified (pure serialization) may stay.

## Build

1. `loom/src/planning/checklist/offers.rs`: delete `RefineSpec` (variant, `kind` arm `"refine-spec"`, `label` arm); rename `NewMetaplanRound` with its doc comment ("Plan follow-up entries in the feature's metaplan."), kind `"plan-follow-ups"`, label `"Plan follow-ups"`. In `choose` (line ~239): `CurrentStep::Ready => vec![Offer::StartRun]`, `CurrentStep::ReadyAllDone => vec![Offer::PlanFollowUps]` (keep this exact text: a wiring check greps it).
2. `loom/src/plan_server/wire/checklist.rs`: `OfferKind::RefineSpec` deleted, `NewMetaplanRound` renamed `PlanFollowUps` (doc "Plan follow-ups for a finished feature."), and the `From<&Offer>` arms.
3. Do this step FIRST, before step 1: W1's web tests parse the new kind through this schema. `web/src/plan/api/schema.ts` `OFFER_KINDS`: delete `"refine_spec"`, rename `"new_metaplan_round"` to `"plan_follow_ups"` in place. Nothing else in `web/` is yours (W1 owns the rest); `loom/src/plan_server/wire/tests_schema.rs` reads this file with `include_str!`, which is why you own it.
4. `loom/src/plan_server/features.rs` test (`ReadyAllDone` case, line ~204): secondary is `[("plan-follow-ups", "Plan follow-ups")]`.
5. `loom/src/planning/guidance/opening.rs`: delete the `RefineSpec` arms (lines ~114, ~145); `PlanFollowUps` arms per the decision above; delete `next_round` (line ~275) and its call.
6. `loom/src/planning/guidance/start.rs` `step_class`: drop `| Offer::RefineSpec`.
7. Delete `plan-plugin/skills/retro/` (the directory). `loom/build/assets.rs` walks the directory, so no build list changes. In `loom/src/assets/mod.rs` `plan_plugin_includes_required_files` (line ~67) delete `"skills/retro/SKILL.md"`.
8. `plan-plugin/skills/metaplan/SKILL.md`: remove rounds (description line 3 and lines ~10-35). The skill now: writes `doc/plans/<feature>/index.md` when the feature has no metaplan; otherwise appends follow-up entries under a `## Follow-ups` heading at the end of `index.md` (add the heading if it is missing; it names no plan and is not numbered, so the app does not read it as an entry). Follow-up entries are level-2 headings in the existing entry form, numbered on from the last entry. Finished entries (and every entry whose plan exists) stay unchanged. Keep the read-only rules for plans and `REVIEW-*.md`. Remove every "round" word. The text must contain `doc/plans/<feature>/index.md` and `## Follow-ups` (step 9's test checks both).
9. Tests you own, edited to the new truth (delete what pinned the removed behaviour, keep every other assertion):
   - `wire/checklist_tests.rs` (~231-241), `wire/tests_schema.rs` (`all_offers`, `all_offer_kinds`: exhaustive matches and lists).
   - `guidance/opening/tests.rs`: delete `refine_spec`; replace `new_metaplan_round_starts_at_two` with `plan_follow_ups_appends_to_the_index` (step `done, Plan follow-ups`, the target above for `f`, skill `metaplan`); delete `new_metaplan_round_uses_first_gap`; `missing_plan_entry_names_offer_id` and `plan_entry_must_belong_to_selected_metaplan` keep their point with a metaplan path that is not a metaplan (`doc/plans/f/metaplan-02.md` is now `Other`, so the error still names the offer id).
   - `guidance/opening/tests/names.rs` (~76-77): drop the retro row; `PlanFollowUps` maps to `loom plan: metaplan`.
   - `guidance/opening/tests/features.rs`: the `RefineSpec` use at ~63 becomes `PlanFollowUps`; rewrite `metaplan_round_ignores_other_features` as `plan_follow_ups_names_its_own_feature_index` (feature `b` has its own `index.md`; the opening for `f` names `doc/plans/f/index.md`).
   - `guidance/start/tests/labels.rs` (~53-54), `start/tests/classification.rs` (~23, ~42).
   - `guidance/tests.rs`: line ~206 the metaplan skill phrases become `["doc/plans/<feature>/index.md", "## Follow-ups"]`; delete the `("retro", vec!["REVIEW-"])` row. ALSO replace the two line-number pins: `product_lines("section_03.md", 282, 290)` and `product_lines("section_09.md", 375, 380)`. D1 rewrites those spec files in this stage and their line numbers move. Replace `product_lines` with a marker-based helper `product_block(file, first_prefix, last_prefix) -> String` that returns the lines from the first line starting with `first_prefix` through the next line starting with `last_prefix`, and panics naming the file when either is missing. Section 03: from `- Files at the paths in the kinds table` to `- In each plan:`. Section 09: from `loom plan is ending this session` to `Then reply "Closing write done." and stop.` Same assertions otherwise.
   - `guidance/tests/starting.rs` (~100-104): the `RefineSpec` case becomes `PlanFollowUps` with its Step/Target/Skill lines; at ~64-78 make `state.metaplans` one path, `doc/plans/f/index.md`, and its expected line.
   - `guidance/tests/drafting.rs` (~79): drop `"retro"` from the skill directory list.
   - `guidance/team/tests.rs` (`fixture()`, ~10-40): fold `doc/plans/f/metaplan-02.md`'s entries into `index.md` under `## Follow-ups` (`Zeta again` duplicates `Zeta` and goes; Omega, Beta, Gamma, Delta follow Alpha in that order). Then fix the plan-drafting expectations to the merged reading order; the section-drafting test does not change.
10. Frozen contract files (planned edits, plan table; change only these lines):
    - `loom/tests/planning_guidance_contracts.rs`: remove `"retro"` from `SKILLS` (and its length `[&str; 10]` to 9); delete `compare_spec_with_build_keeps_retro_guidance`.
    - `loom/tests/plan_redesign_contracts.rs` `finished_feature_has_no_primary` (~284): `for id in ["plan-follow-ups"]`.
    List each in your report by file and test name.

## Not yours

`planning/kinds.rs`, `planning/summary/**`, `planning/checklist/{tests,order_tests,offers_tests}.rs`, `loom/tests/planning_checklist_contracts*`, `loom/tests/planning_documents_contracts/**` are R2's (R2 also updates the offer ids in those). The new contracts in `loom/tests/follow_ups_contracts.rs` are frozen: make them pass, never edit them.

## Check

`rg -n -i 'refine_spec|RefineSpec|refine-spec|Compare spec|new_metaplan_round|NewMetaplanRound|new-metaplan-round|next_round' loom/src/planning/guidance loom/src/plan_server loom/src/assets plan-plugin loom/tests/planning_guidance_contracts.rs loom/tests/plan_redesign_contracts.rs` prints nothing, and `rg -n -w retro loom/src plan-plugin loom/tests` prints nothing.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::guidance:: plan_server::wire:: plan_server::features:: assets::`

R2 and S1 edit the same crate at the same time; a compile error in a file you do not own is theirs: report it, do not fix it.
