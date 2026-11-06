# R2: Review the feature, end to end; Built candidates from the ledger (trace-server)

Read `../common.md` "Offers and sessions" in full.

## Files you own

`loom/src/planning/checklist/offers.rs`, `offers_tests.rs`, `plan_server/wire/checklist.rs`,
`wire/checklist_built.rs` (+ `checklist_built/`), `wire/checklist_tests.rs`,
`planning/guidance/opening.rs` (+ `opening/`), `guidance/start.rs` (+ `start/`),
`guidance/tests.rs`, `loom/src/assets/mod.rs`, `plan-plugin/skills/review/SKILL.md`,
`web/src/plan/chat/offer-handlers.ts`, `offer-handlers.test.ts`,
`web/src/plan/feature/next-action-copy.ts`.

## Tasks

1. **Offer.** Follow `Offer::PlanFollowUps` through every arm (`id`, `target`, `kind`, `feature`,
   `label`, `Serialize`) and add `Offer::ReviewFeature`. Emit it from `extras` for any feature
   whose metaplan has a plan in state Running or Done (the checklist's plan facts), deduplicated
   by id. It is never a primary.
2. **Wire.** `OfferKind::ReviewFeature` (`review_feature`) and its `From` arm; update
   `wire/checklist_tests.rs` and the checklist fixture JSONs only if they list offer kinds.
3. **Session.** `opening.rs::offer_step_and_skill` gives `("review, Review the feature",
   "review")`; `offer_target` the sentence in common.md. `start.rs` accepts it as an authoring
   offer for a feature (`from_offer_id` finds it among the secondaries).
4. **Skill.** `plan-plugin/skills/review/SKILL.md`, frontmatter like the other skills: the
   procedure in common.md ("The review skill"), the ledger format verbatim, the Status values,
   "write only doc/plans/<f>/findings.md", and the turn-end duty the other skills end with
   (`loom plan check`). Confirm `loom/build/assets.rs` embeds every file under `plan-plugin/`
   (read it); update the skill list pinned in `guidance/tests.rs` and the key check in
   `assets/mod.rs`.
5. **Built candidates.** built-and-new reads candidates from the run reviews
   (`wire/checklist_built.rs`). Read them from the feature's `Trace.findings` instead: open
   findings in ledger order, each with its id. Keep the Built view's other parts. If the merged
   `BuiltView` shape needs an id field for the browser, add it and update both fixture sides.
6. **Web.** Register `review_feature` in `authoringKinds` (`offer-handlers.ts`) and add its
   sentence to `SENTENCES` in `next-action-copy.ts`: "Review what was built and record what you
   find." R1 adds the kind to `OFFER_KINDS`.

## Tests

`offers_tests.rs` (offered with a running plan, with a done plan, not before), opening and start
tests beside the existing ones, a Built candidates test from a ledger fixture, and
`offer-handlers.test.ts`. The frozen contracts `review-feature-offered-after-a-run` and
`review-feature-is-an-authoring-offer` must pass unedited.

## Done

The stage's Rust and web acceptance commands pass for your files; report every pinned test you
had to change (the orchestrator files one integrity dispute).
