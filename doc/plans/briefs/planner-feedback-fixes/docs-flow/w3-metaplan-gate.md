# W3: plans wait for a settled metaplan (codex sol, high)

Read `common.md` beside this file first. Decision D1 is authoritative.

## Goal

While a feature's metaplan is not settled, its "Write plan <slug>" and "Draft all with a team"
offers are visibly disabled with the reason "Settle the metaplan first", and the server refuses to
start them.

## Files you own

- `loom/src/planning/trace/gate.rs` (new) and `loom/src/planning/trace/mod.rs` (add `pub mod gate;`
  only)
- `loom/src/planning/guidance/start.rs` and `loom/src/planning/guidance/start/**` (except the frozen
  `gate_contract_tests.rs`)
- `loom/src/planning/guidance/opening.rs`
- `loom/src/plan_server/wire/checklist.rs`, `loom/src/plan_server/wire/checklist_card.rs` and
  `loom/src/plan_server/wire/fixtures/checklist.json` (also any other `wire/fixtures/*.json` that
  serializes an `OfferView`; `rg -l '"kind": *"write_plan"' loom/src/plan_server/wire/fixtures`
  finds them, and the concept fixture stays W2's)
- `plan-plugin/skills/plan/SKILL.md` and `plan-plugin/skills/metaplan/SKILL.md`
- `web/src/plan/api/schema.ts` (`offerViewSchema` only), `web/src/plan/api/schema.test.ts` and
  `web/src/plan/api/fixtures/checklist.json` (and any other web fixture holding an offer; `rg -l
  '"write_plan"' web/src/plan/api/fixtures`)
- `web/src/plan/offers/**`

## Steps

1. `gate.rs`: `pub fn metaplan_settled(feature: &Feature) -> bool`. Find the feature's
   `feature.trace` document with role metaplan. Use `crate::planning::trace::SettleState`
   (`trace/mod.rs` line 115) and the document's `settle` and `settleable`, built in
   `trace/build/docs.rs` lines 219 to 282. Return `settleable == false || settle ==
   SettleState::Settled`, and false when there is no metaplan document. Add unit tests beside it.
2. `start.rs` `AuthoringStart::from_offer_id`, lines 56 to 80: after the offer is found, refuse
   `Offer::WritePlan` and `Offer::DraftAllPlans` with `StartError::MetaplanUnsettled` when
   `!metaplan_settled(feature)`. Add the variant in `start/error.rs` with its `Display` text.
   `plan-follow-ups` is not gated. Find the callers' match arms with `loom map --impact
   StartError`; `plan_server/guidance/provider.rs:148` and `planning/guidance/team.rs:34` must map
   the new variant to a 409-class refusal, the way `NotOffered` maps. Those two files are not in
   your list: if a match is not exhaustive there, report it and the orchestrator adds the arm.
3. Wire: `OfferView` in `wire/checklist.rs` gains `blocked_reason: Option<String>`. Set it in the
   project-level conversion (`ProjectChecklistView::from(&Project)`, lines 308 to 320, or wherever
   offers are converted with access to `Feature`) for write-plan and draft-all-plans offers while
   the gate is closed. The checklist is built before the trace (`planning/project.rs:239`), so the
   gate belongs here at conversion time, never in `gap_offers`. Update every fixture.
4. Web: `offerViewSchema` gains `blocked_reason: z.string().nullable()`. `OfferButton`
   (`offers/offer-button.tsx`) ORs it with the existing `authorBlockReasonAtom` reason and renders
   the disabled state and `DisabledReason` as it does for that atom. Add a test in
   `offers/offer-button.test.tsx`.
5. Skills: in `plan-plugin/skills/plan/SKILL.md`, add one short paragraph near the top. When the
   chat is asked to write a plan and the metaplan has no `Settled:` line, or the user says it
   changed since, stop and ask the user to settle the metaplan first. In
   `plan-plugin/skills/metaplan/SKILL.md`, add: after the metaplan is agreed, tell the user to mark
   it settled before plans are written. `opening.rs`: the WritePlan opening (about lines 109 and
   136) needs no gate text, because the server refuses first. Change it only if a test there pins
   gate-free wording.

## Traps

- A Settled line can be present yet stale (state `Revised`). The gate reads the settle state,
  never the presence of the line.
- The frozen contract `start/gate_contract_tests.rs` is the spec. Do not edit it.

## Done

`cargo check --manifest-path loom/Cargo.toml --all-targets` and `cd web &&
node_modules/.bin/tsc -b --noEmit` are clean for your files. Report the files changed and any match
arm outside your files that needs the new variant.
