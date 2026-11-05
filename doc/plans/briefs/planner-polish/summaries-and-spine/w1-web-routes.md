# W1: metaplan summary route, product-overview landing, pressure record on plan rows (issues 2, 6, 7)

Tier: sonnet (`loom-software-engineer`). Read `common.md` (D3, D4, D5, D6), `doc/loom/knowledge/architecture/planner-feature-screens.md` and `conventions/plan-web.md` ("Concept Mode Test Rules", "UI Repair Test Rules") before code. This is behaviour work. The spine badge's look is the next stage's (fable V1), so render plain markup with the class names below and no new CSS.

## Build

1. **Metaplan route (D3).**
   - `web/src/plan/router.tsx`: `metaplan` and `metaplan/:file` render a page that resolves the metaplan, then renders `ConceptRoute` for page key `<feature>/metaplan/<stem>`.
   - `feature/document-page.tsx` `MetaplanPage`: keep its no-metaplan and not-found branches, and replace `<SourcePage path={metaplan.path}/>` with the summary page.
   - `api/page-key.ts`: `parsePageKey`, `docKindOfPageKey`, `scopeOfKey`, `pageHref` and `pageKeyOfPath` learn the `metaplan` scope (`/f/<f>/metaplan/<file>` ↔ `<f>/metaplan/<stem>`, with the first metaplan at `/f/<f>/metaplan`), and `ConceptScope` (`api/page-key.ts:9`) gains `"metaplan"`. Check `api/schema-concept.ts` for a scope enum the wire page carries and widen it to match M1's `"metaplan"`.
   - `concept/column-context.ts` `summaryIdOfKey`: a metaplan key's id is the key itself.
   - `concept/concept-route.tsx` and `concept/document-key.tsx`: page naming for a metaplan page ("Metaplan" or the metaplan's title).
   - `feature/spine-model.ts` `summaryPathnameOfSource`: the metaplan source maps to its own summary row, not the group target.
   - `chat/viewed-document.ts`: the metaplan summary route names its source document, as section pages do.
2. **Landing (D4).** `feature/landing.ts` and its test.
3. **Pressure (D5, D6).**
   - `spine-model.ts`:
     - drop the `pressure` `GroupId` and its group;
     - drop the `markOf` pressure override;
     - `planChildren` carries `pressure_date` from `checklist.plan_rows` (match by path) and the plan's state.
   - `feature/spine-row.tsx` `ChildRow` renders `<span className="plan-spine__badge" data-tested={tested}>` holding a glyph and the words. A tested plan reads "Pressure-tested <date>" (the date as `pressure_date` gives it), an untested one "Not pressure-tested", and a `retired` plan shows nothing.
   - Delete `feature/pressure-page.tsx` and the `pressure` route.
   - In `feature/page-offer.ts`, drop the `page === "pressure"` branch of `pageDrawsPrimary` and `pressureOffer` if nothing else imports it (`rg` first).
   - `chat/viewed-document.ts` and `chat/context-kit.ts` drop the pressure route's context.
   - `plan/help/legend-entries.tsx` and `help/getting-around-scene.tsx`: drop the spine's optional-pressure mark entry and any `Pressure test` spine row text, and keep their tests green.

## Planned frozen edits (common.md table; yours, exactly as worded there)

- `feature/spine.contracts.test.tsx`: `SPINE_ROWS` loses `/\bPressure test\b/`, and "a feature opens its vision" is deleted.
- `feature/review-fixes.contracts.test.tsx`: "a feature with a written spec opens on its vision" is deleted.
- `feature/pages.contracts.test.tsx`: "the Pressure page lists every plan with its record state" is deleted, with the helpers only it used.

## Contracts (written before you start; make them pass, do not edit them)

`web/src/plan/feature/polish.contracts.test.tsx`:
- "a feature with a written spec opens on its product overview";
- "the metaplan route shows the metaplan summary with Source one click away";
- "plan rows carry the pressure record and the spine has no Pressure test row".

## Pinned tests you will change (non-frozen)

`router.test.tsx` (the `/f/demo/pressure` case), `spine.test.tsx` (six head rows, "Pressure test 1 untested", the "Pressure test mark" describe), `spine-model.test.ts`, `page-offer.test.ts`, `landing.test.ts`, `chat/viewed-document.test.ts`, `api/page-key.test.ts`, `concept/document-key.test.tsx`. Keep every assertion this plan does not change, and name each one you change in your report.

## Files

Your row in the plan's worker table. Never edit M1's or N1's files.

## Run once at the end

`node_modules/.bin/vitest run src/plan/feature src/plan/concept src/plan/api src/plan/chat/viewed-document.test.ts src/plan/help src/plan/router.test.tsx`, from `web/`.
