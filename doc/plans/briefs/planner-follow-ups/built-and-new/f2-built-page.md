# F2: the feature's Built page (R5)

Tier: fable (`loom-senior-software-engineer` with the model override `fable`).

Before anything else:

1. Invoke `frontend-design:frontend-design` with the Skill tool.
2. Read `common.md`, this brief, `web/src/plan/ui/DESIGN.md` in full, and the "Layout route and
   landing", "Offers have one owner each" and "Run page" sections of
   `doc/loom/knowledge/architecture/planner-feature-screens.md`.
3. Read `web/src/plan/feature/run-page.tsx`, `plan-page.tsx` and `pages.css`: the Built page is
   their sibling and uses the same page anatomy.

There is no mockup for this page: it follows DESIGN.md (page rhythm, hairline lists, status as a
glyph and a word, one primary per screen). The user reviews it from the screenshots.

## Built for you (S2, previous wave; read them, do not edit them)

- `feature/built-model.ts`: `builtOf(checklist)`, `isBuilt(checklist)`, `reviewHref(path)`,
  `reviewFile(path)`, `followUpSeed(candidate)`.
- `feature/landing.ts`: an all-done feature already lands on `/f/<feature>/built`.
- `offers/actions.ts`: `runOffer(offer, seed?)`; a registered Plan follow-ups handler starts the
  session with `seed` as the user's first message.
- The offer: `offerOfKind(checklist, "plan_follow_ups")` (`feature/page-offer.ts`), the
  feature's sole secondary once every plan is done.

## Build

1. `web/src/plan/feature/built-page.tsx` (new): `export function BuiltPage()`. `useFeature()`;
   loading as the Run page does; when `!isBuilt(checklist)`,
   `<Navigate replace to={landingPath(checklist)} />` (no Built state for an unfinished
   feature). Otherwise `PlanPage` with `label="built"`, `title="Built"` and the lede "Every plan
   is done. What the runs left open is below; each item can become a follow-up entry in the
   metaplan.", then four sections, each a `section` labelled by its `.plan-section-title`:
   - **Done plans**: a `.plan-list` named "Done plans": per `plan_rows` row in state `done`, the
     slug in mono linking to `/f/<f>/engineering/<slug>`, the title, and at the row's end a
     quiet link "Run review" to `reviewHref` of that slug's entry in `builtOf(checklist).reviews`,
     or "No run review" in `.plan-fine` muted.
   - **Spec check**: a list named "Spec check": per `spec` entry (the review's
     `## Spec Conformance` items: what integration-verify changed so spec and build agree), the
     text, and a `.plan-fine` source line "<stage> · <review file name>" (the file name links to
     `reviewHref`). Spec gaps it left open arrive as open questions under Candidate follow-ups.
     Empty: one `.plan-empty` sentence "The runs recorded no change to the spec."
   - **Candidate follow-ups**: a list named "Candidate follow-ups": per candidate, the kind word
     ("Open finding" or "Open question") in `.plan-fine`, the text, a `.plan-fine` source line
     "<stage> · <review file name>" (the file name links to `reviewHref`), and a secondary
     `size="sm"` button named exactly `Plan this`, described by the candidate's text
     (`aria-describedby`), whose click is `runOffer(offer, followUpSeed(candidate))`. Draw it
     through `DisabledReason` with `authorBlockReasonAtom` as its reason, as `OfferButton` does;
     with no Plan follow-ups offer, or no registered action, draw no button. Empty: "The run
     reviews list no open findings or questions."
   - **Something else**: a labelled textarea "What else needs doing?" (an input, so it keeps its
     border: 1px `--border`, `--corner-control`, 3 rows, `--type-body`), then the screen's one
     primary `Plan follow-ups` (`DisabledReason look="primary"`), blocked by the author block
     reason, else by "Describe the follow-up" while the text is blank; its click is
     `runOffer(offer, text.trim())` and clears the box. Beside it a quiet link "Start it on New
     work" to `/new` with router state `{ followUp: checklist.feature }`.
   Every server string goes through `<Untrusted>`; nothing renders Markdown or HTML from a
   review.
2. `web/src/plan/feature/built.css` (new, imported by `built-page.tsx`): only what `pages.css`
   and `design.css` do not already give. Theme tokens only.
3. `web/src/plan/router.tsx`: `{ path: "built", element: <BuiltPage /> }` among the feature
   children, after `run`. Also delete the `metaplan/:file` child: a feature has one metaplan
   now, at `/f/<feature>/metaplan` (S3 removed the per-file handling last wave).
   `router.test.tsx`: add `/f/demo/built` renders the page for a built checklist; drop
   `/f/demo/metaplan/index.md` from the `it.each` of "shows the metaplan's summary page at %s,
   not its source"; delete "moves a first metaplan that is not the index to its own file route";
   in "keeps the offer to start a metaplan while there is none, and refuses an unknown file",
   `/f/demo/metaplan/other.md` now renders "No such page" (the catch-all), so assert that.
   Name each in your report.

## Fixed visual targets (Walnut and Ledger, 1440px and 390px; feature `feat-planner`)

1. Eyebrow `feat-planner · built`, title `Built`, lede in `--type-body` muted at 48ch, then the
   sections at `--space-6`, each title `.plan-section-title`.
2. Rows on hairlines at 36px minimum; no box around a list, no card, no coloured chip.
3. A candidate row: text in `--foreground` at `--type-body`, wrapping (never truncated), the
   kind word and source line in `--type-fine` muted, `Plan this` at the row's end, centred on the
   first text line; at 390px the button sits under the text, left-aligned.
4. The primary `Plan follow-ups` is the only filled button on the screen, and the spine's next
   step draws no button (a finished feature has no primary offer).
5. Long review paths and stage names wrap (`overflow-wrap: anywhere`); 390px never scrolls
   sideways.
6. The spine's Run group is the open group (S3 maps `built` to it); no row is current.

## Tests: `web/src/plan/feature/built-page.test.tsx`

Mount through the routes as `feature/polish.contracts.test.tsx` does (copy its helpers; never
import a frozen file), with `@/plan/chat/panel` mocked as `feature/pages.contracts.test.tsx`
mocks it and the Plan follow-ups action registered by the test. Cases: the three lists render
their entries and their empty sentences; a plan without a review reads "No run review"; the free
text box sends `runOffer` with the trimmed text and clears; blocked while blank; an unfinished
feature at `/f/atlas/built` lands on its `landingPath`; no `Plan this` without the offer.

## Contracts (written before you start; make them pass, never edit them)

`web/src/plan/feature/built.contracts.test.tsx`: "an all-done feature lands on its Built state";
"Plan this starts Plan follow-ups seeded with the finding and its source"; "a finding with markup
renders as plain text on the Built page". Read them first.

## DESIGN.md

F1 is its only editor this stage. If the page needs a rule DESIGN.md does not already give,
report the exact `exception:` line for the main agent to add.

## Files

Your row in the stage's worker table. Never edit S1's, S2's, S3's or F1's files. Before finishing, `rg`
for every class and text you added in `web/src` and report any collision.

## Run once at the end

`node_modules/.bin/vitest run src/plan/feature src/plan/router.test.tsx`, from `web/`.
