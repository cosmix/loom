# R2: one metaplan per feature, and the checklist's follow-up path (R3)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first. Knowledge: `architecture/planning-document-model.md` ("Kinds, Ids and Slugs", "Project View and Checklist"), `architecture/summary-store.md` ("Store Layout and Ids"). Both describe several metaplans per feature: that is now stale; record each such claim with `loom memory note "stale-knowledge: ..."`. Anchor edits by symbol and snippet; line numbers are from the stage base.

## Settled decisions

- `doc/plans/<f>/index.md` is a feature's only metaplan. `doc/plans/index.md` stays a misplaced metaplan (`needs-feature`, move offer). Every other `*metaplan*.md`, in a feature or at the top of `doc/plans/`, is `DocKind::Other`: no entries, no checklist items, no problems, no move offer, no summary.
- `Feature.metaplans` and `ProjectState.metaplans` keep their list types (zero or one element). No refactor of their consumers.
- Follow-up entries are ordinary level-2 entries placed after a `## Follow-ups` heading in `index.md`. `metaplan::entries` already skips that heading (unnumbered, no plan file), so no parser change.
- The metaplan summary id is `<feature>/metaplan/index` only.
- R1 renames the offer in parallel: `Offer::RefineSpec` is gone, `Offer::NewMetaplanRound` is `Offer::PlanFollowUps` (id `plan-follow-ups`, label `Plan follow-ups`). `Ready` offers `[start-run]` only (as primary; no secondary from the step), `ReadyAllDone` has no primary and secondary `[plan-follow-ups]`. Your checklist test files use these names.

## Build

1. `loom/src/planning/kinds.rs` `plans_kind` (~216-222): the metaplan arm matches `name == "index.md"` only. Update the doc comment of `DocKind::Metaplan` if it names the old rule.
2. `loom/src/planning/summary/store.rs`: `metaplan_feature` accepts `<feature>/metaplan/index` only (stem must equal `index`); its doc comment, `is_metaplan`'s, and the module doc (lines 1-28: "`<feature>/metaplan/<stem>`", "metaplan file whose stem the id carries") say so. `summary_id_for` needs no change (only `index.md` is classified a metaplan) but its id must still validate.
3. Prompts: `plan-plugin/agents/summary.md` (~29-31) and `loom/src/planning/guidance/text/summary_codex.md` (~22-24, ~66-68): the metaplan summary id is `<feature>/metaplan/index`, one per feature; drop the stem grammar text.
4. Fold check: `fd -g '*metaplan*.md' doc/plans -E briefs` lists nothing at the stage base (verified); if it lists a file when you run it, fold its entries into that feature's `index.md` under `## Follow-ups` and report it.

## Tests

New, in `loom/src/planning/checklist/tests.rs` (beside `ready_rows_secondaries`):
- `a_follow_up_entry_reopens_the_plans_step`: a done feature (vision, sections, `index.md` with entry `a`, `archive/DONE-PLAN-a.md`) gets `ReadyAllDone`, no primary, secondary `[plan-follow-ups]`; then the same tree with `index.md` ending in `## Follow-ups\n\n## 2. Polish (`PLAN-polish.md`)\n` gets `CurrentStep::Step(Step::Plans)` and primary `WritePlan { metaplan: doc/plans/demo/index.md, slug: "polish" }`, and `plan-follow-ups` is in neither primary nor secondary.

New, in `loom/src/planning/tests/kinds_tests.rs`: `a_second_metaplan_file_is_another_document`: `doc/plans/f/metaplan-02.md`, `doc/plans/f/round-2-metaplan.md` and `doc/plans/optim-metaplan.md` classify as `DocKind::Other`; and a loaded project whose `doc/plans/demo/metaplan-02.md` names `PLAN-x.md` has `feature.metaplans` holding only `index.md`, no checklist item or offer naming `x`, and no `needs-feature` problem or move offer for the file.

Rewritten to the new rule (keep every assertion that is not about a second metaplan):
- `planning/checklist/tests.rs`: line ~103 `Offer::PlanFollowUps.id() == "plan-follow-ups"`; ~145 and ~218 `ids(&c.secondary)` is empty (`[]`) for `Ready`; ~150 `["plan-follow-ups"]`.
- `planning/checklist/order_tests.rs`: `finished_feature_has_no_primary_and_keeps_followups_in_order` expects `[Offer::PlanFollowUps]`; the `SECOND_META` tests (~76-110) become one-metaplan tests where the second metaplan's entries sit under `## Follow-ups` in `index.md`, or are deleted when their only point was ordering two metaplans; remove `SECOND_META` and `metaplan_at` if unused.
- `planning/checklist/offers_tests.rs`: the `NewMetaplanRound` row (~142-146) becomes `PlanFollowUps` (`plan-follow-ups`, `Plan follow-ups`); delete the `RefineSpec` row (~135-139); ~98-105 use `doc/plans/demo/index.md`; the move-offer fixture (~204-228) drops the top-level `doc/plans/metaplan-02.md` (now `Other`, no move) and keeps `doc/plans/index.md`.
- `planning/summary/store/tests.rs` (~26, ~99-102, ~155-161): no `alpha/metaplan/metaplan-02` id; the `metaplan-02.md` fixture yields no id.
- `planning/summary/store/metaplan_id_tests.rs`: `alpha/metaplan/index` is the only accepted shape; `alpha/metaplan/metaplan-02` and `a1/metaplan/0` move to the rejected list.
- `planning/paths/tests/feature.rs` (~80), `planning/tests/metaplan_tests.rs` (`optim-token-metaplan.md` and `b/metaplan-02.md` move to the not-a-metaplan list), `planning/tests/kinds_tests.rs` (~128-150, ~249, ~263-264), `planning/tests/kinds_tests/features.rs` if it builds a second metaplan, `planning/tests/problems_tests.rs` (~151), `planning/tests/problems_tests/metaplans.rs` (~10-18: put the entry text in `index.md`), `planning/tests/project_tests.rs` (~138, ~166).
- `plan_server/wire/checklist_rows.rs` tests (~170-215: one metaplan row; the second file's entries move under `## Follow-ups` in `index.md`), `plan_server/wire/checklist_card.rs` test `test_listed_plan_count_deduplicates_across_metaplans` (~246: duplicate entries now live in one `index.md`; keep the dedup assertion), `plan_server/concept/tests.rs` (~74 `second-metaplan.md`), `plan_server/pressure/options/tests.rs` (~176 `other-metaplan.md`).

Frozen contract files (planned edits, plan table; change only these):
- `loom/src/planning/summary/store/metaplan_contract_tests.rs`: drop `SECOND_ID`'s expectations; `expected_ids` holds exactly one metaplan id (`alpha/metaplan/index`); `summary_id_for(doc/plans/alpha/metaplan-02.md)` is `None`; add `alpha/metaplan/metaplan-02` to the rejected ids; update the module doc's first line to one metaplan per feature.
- `loom/tests/planning_checklist_contracts.rs`: ~160 `["plan-follow-ups"]`; `metaplan_entry_any_spelling` (~240-275): drop the `round-2-metaplan.md` file, append its two entries to `index` under `## Follow-ups` (renumbered 5 and 6), `paths == [META]`, the primary is `WritePlan { metaplan: META, slug: "next" }`; keep the slug list of the original entries and the `metaplan-entry-without-plan` line 9 assertion.
- `loom/tests/planning_checklist_contracts/features.rs` `complete_documents_have_no_draft_all_offer` (~232): `["start-run"]`.
- `loom/tests/planning_documents_contracts/kinds.rs` (~59): only `index.md` is a metaplan in both homes; `round-2-metaplan.md` asserts `DocKind::Other` with id `&rel[4..]`.
- `loom/tests/planning_documents_contracts/features.rs` (~60-80): drop `doc/plans/round-2-metaplan.md` from the misplaced list (it is `Other` now); assert it is in neither `needs-feature` problems nor `project.moves`.
List each frozen edit in your report by file and test name.

## Not yours

Production offers, guidance, wire enum, `schema.ts` and the plugin skills are R1's. The frozen contracts in `loom/tests/follow_ups_contracts.rs` are yours to make pass, never to edit.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::checklist:: planning::summary::store:: planning::tests:: planning::paths:: plan_server::wire:: plan_server::concept:: plan_server::pressure::`

R1 and S1 edit the same crate at the same time; a compile error in a file you do not own is theirs: report it.
