# W1: the web side of the offer change (R2, R3)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first (its "Code and test rules"). Knowledge: `conventions/plan-web.md`. Anchor edits by symbol and snippet; line numbers are from the stage base.

## Settled decisions

- The wire kind `refine_spec` is gone and `new_metaplan_round` is renamed `plan_follow_ups` (id `plan-follow-ups`, label `Plan follow-ups`). R1 edits `web/src/plan/api/schema.ts` `OFFER_KINDS` (the Rust schema test reads it); you do not touch that file.
- `plan_follow_ups` is an authoring offer, like `new_metaplan_round` was: it names no feature on the wire, so the screen's feature applies (`chat/offer-flow.ts` ~105).
- The Run page loses its follow-ups section (`Followups`, "Next round") entirely. Nothing draws the Plan follow-ups button in this stage; the later built-and-new stage draws it in the feature's Built state. Do not add it anywhere.
- A feature has one metaplan, `doc/plans/<f>/index.md`. `chat/turn-footer.tsx` `changedDocKind` marks `doc/plans/(<f>/)?index.md` as a metaplan and nothing else (drop the `[^/]*metaplan[^/]*` alternative). Leave the rest of the web's metaplan-list handling as it is.

## Build

1. `web/src/plan/chat/offer-handlers.ts` `authoringKinds`: delete `"refine_spec"`, rename `"new_metaplan_round"` to `"plan_follow_ups"` in place. Keep the literal `"plan_follow_ups"` there (a wiring check greps it).
2. `web/src/plan/chat/offer-flow.ts` ~105: the comment names `plan_follow_ups` only.
3. `web/src/plan/feature/next-action-copy.ts` `SENTENCES`: delete `refine_spec`; rename `new_metaplan_round` to `plan_follow_ups` with `({ owner }) => \`Every plan for ${owner} is done; follow-ups start from here.\``.
4. `web/src/plan/feature/run-page.tsx`: delete `Followups` and its doc comment (~66-95) and its use (~147); fix the route doc comment (~118, "then follow-ups"); remove imports this orphans (`offerOfKind`, `OfferButton`, `allPlansDone` if unused, `OfferView` if unused). `rg` the CSS for `plan-followups` and remove a rule only it used.
5. `web/src/plan/run/plan-run-actions.tsx` ~13: the doc comment no longer names follow-ups ("A done plan has nothing here.").
6. `web/src/plan/chat/turn-footer.tsx` `changedDocKind` per the decision.

## Tests

Edited to the new truth (keep every assertion that is not about the removed behaviour):
- `chat/offer-handlers.test.ts`: the kinds list loses `refine_spec`, renames the round kind.
- `chat/session-home.test.tsx` ~266-279: one row, `["plan_follow_ups", "plan-follow-ups", "Plan follow-ups"]`, and the comment above it says "this offer".
- `feature/run-page.test.tsx`: drop the two `registerOfferAction` lines for the removed kinds (~28-29) if nothing else needs them; replace the two follow-up tests (~274-301) with one, `"draws no follow-up section once every plan is done"`: `finished([offer("plan_follow_ups", "Plan follow-ups")])` renders "Every plan is done." and no button named "Plan follow-ups", no text "Next round".
- `run/plan-run-actions.test.tsx` ~229-244: the comment says the follow-ups belong to the feature; the two offers become one `plan_follow_ups` offer.
- `files/doc-heading-view-parts.test.tsx` ~73-78 and `files/plan/plan-actions-extras.test.tsx` ~43-58: the absence checks name `Plan follow-ups` and the `plan_follow_ups` kind; the handler registration uses `plan_follow_ups`.
- `chat/turn-footer.test.tsx` ~380: `["doc/plans/demo/metaplan-02.md", null]`.

Frozen contract files (planned edits, plan table; change only these lines):
- `feature/polish.contracts.test.tsx` ~130-133, `feature/spine.contracts.test.tsx` ~144-147, `feature/review-fixes.contracts.test.tsx` ~64-67: the fixture's `secondary` becomes `[offer("plan_follow_ups", "Plan follow-ups")]`.
- `feature/pages.contracts.test.tsx`: its one test, `"a finished feature's Run page holds the compare offer once"`, pins the removed behaviour. Rename it `"a finished feature's Run page lists every plan and holds no follow-up offer"`; the fixture's secondary is `[offer("plan_follow_ups", "Plan follow-ups")]`; register `plan_follow_ups` with a recording handler in place of the two removed kinds; assert the routed page holds no button whose text includes `Plan follow-ups` and the handler never ran; keep both `planRowOf` assertions. Drop `COMPARE`, `compareRuns` and the retro comments (~105, ~180).
List each frozen edit in your report by file and test title.

## Not yours

`web/src/plan/api/schema.ts` (R1). `web/src/plan/chat/follow-ups.contracts.test.tsx` is frozen: make it pass, never edit it. Never touch `web/src/plan/feature/spine-row.tsx`, `spine.css` or `web/src/plan/ui/DESIGN.md`: the planner-visual stage edits them at the same time.

## Check

`rg -n -i 'refine_spec|refine-spec|Compare spec|new_metaplan_round|New metaplan round|Next round|retro' web/src` prints nothing.

## Run once at the end

`cd web && node_modules/.bin/vitest run src/plan/chat src/plan/feature src/plan/run src/plan/files`

A failure parsing `plan_follow_ups` through `offerKindSchema` means R1 has not edited `schema.ts` yet: report it, do not edit that file.
