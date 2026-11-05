# S3: one metaplan per feature in the web, and the Built page's spine group (R3 follow-through, R5)

Tier: sonnet (`loom-software-engineer`). Read `common.md`, then this brief, then the "Layout route
and landing" and "Spine" sections of `doc/loom/knowledge/architecture/planner-feature-screens.md`
before code. Anchor edits by symbol.

## Why

follow-ups-core made `doc/plans/<feature>/index.md` the feature's only metaplan: the server lists
at most that one in `ChecklistView.metaplans`, and the only metaplan summary key is
`<feature>/metaplan/index`. The web still handles several metaplans and a per-file address
`/f/<feature>/metaplan/<file>`. That code is now unreachable; remove it, so `/f/<feature>/metaplan`
is the metaplan's one address. F2 removes the `metaplan/:file` route in `web/src/plan/router.tsx`
(its file) next wave; everything else is yours.

## Build

1. `web/src/plan/feature/spine-model.ts`:
   - delete `metaplanChildren`; `childrenOf` gives the metaplan group no children;
   - delete `metaplanFile` and `firstMetaplan` (every caller reads `checklist.metaplans[0]`);
   - `currentPathname`: drop the metaplan branch (the bare `/f/<f>/metaplan` route is the group's
     own target, so it is current as any other path is); keep the source-route branch;
   - `summaryPathnameOfSource`: a metaplan source maps to the group's target;
   - `GROUP_OF_PAGE` also maps `built` to `run`, so the Run group is the open group on the Built
     page (`{ ...Object.fromEntries(...), built: "run" }`).
2. `web/src/plan/feature/document-page.tsx` `MetaplanPage`: no `:file` param; it shows
   `checklist.metaplans[0]` through `ConceptRoute`, or the "no metaplan yet" offer. Delete the
   per-file lookup, the "No such metaplan" state, the redirect to a file route and the
   source fallback for a file name outside the summary stem grammar (`index` is always a valid
   stem). Drop the imports this orphans.
3. `web/src/plan/api/page-key.ts`: a metaplan summary key is only `<feature>/metaplan/index`
   (`FIRST_METAPLAN`); `parsePageKey` rejects any other metaplan doc (drop `METAPLAN_STEM`);
   `pageHref` gives `/f/<feature>/metaplan` for it; `pageKeyOfPath` no longer reads
   `/f/<feature>/metaplan/<file>.md` (null, as for any unknown address). Update the doc comments.
4. `web/src/plan/chat/viewed-document.ts` `metaplanOf`: the bare route only, the path of
   `metaplans[0]`.
5. `web/src/plan/files/section-chips.ts`: delete `metaplanSummaryKey` if nothing else imports
   it after step 2 (`rg -n metaplanSummaryKey web/src`); keep `summaryKeyOf`'s metaplan case.
6. `rg -n 'metaplanFile|firstMetaplan|metaplan/:file|metaplan/[a-z0-9_-]+\.md|metaplans\.length < 2' web/src`
   after your edits: nothing may remain outside `router.tsx` and `router.test.tsx` (F2's).

## Tests (pinned tests you change; name each in your report)

- `feature/metaplan-page.test.tsx`: delete the two-metaplan tests ("the bare metaplan route of
  two metaplans shows the index summary, not the first by path", "the bare metaplan route marks
  the index metaplan's row as current") with `renderTwoMetaplans`, and the two invalid-stem tests
  ("a metaplan whose file name is no summary stem opens as its source, not as a missing page",
  "the bare metaplan route of a metaplan with an invalid stem does not redirect to a summary");
  keep "a metaplan with a valid stem still opens its summary page".
- `feature/spine.test.tsx`: "lists the metaplans as documents only when there is more than one"
  becomes: the metaplan group lists no documents; delete "marks the group's head on the page of a
  lone metaplan that is not index.md"; keep "marks the group's head for a lone metaplan".
- `feature/spine-model.test.ts`: drop the per-file metaplan expectation (`round%202.md`); add the
  row `["/f/atlas/built", "run"]`.
- `api/page-key.test.ts`: drop the `demo/metaplan/metaplan-02` cases and the
  `/f/demo/metaplan/index.md` address; add: `demo/metaplan/metaplan-02` does not parse.
- `chat/viewed-document.test.ts`: drop the `/f/atlas/metaplan/second-round.md` row and
  `/f/atlas/metaplan/gone.md`.
- `concept/concept-route.test.tsx`: `conceptPageLabel("/f/demo/metaplan/index.md")` becomes
  `conceptPageLabel("/f/demo/metaplan")`.
- `concept/page-heading-metaplan.test.tsx`: the page key `demo/metaplan/index`, title
  `Roadmap`, path `doc/plans/demo/index.md`, Source `/f/demo/source/plans/demo/index.md`.

No frozen contract changes: `feature/polish.contracts.test.tsx` "the metaplan route shows the
metaplan summary with Source one click away" (`/f/atlas/metaplan`, `index.md`) must pass
unedited.

## Files

Your row in the stage's worker table. Never edit S1's, S2's, F1's or F2's files. S2 runs beside
you in `web/src/plan/feature/`; its files and yours are disjoint.

## Run once at the end

`node_modules/.bin/vitest run src/plan/feature/spine src/plan/feature/metaplan-page.test.tsx src/plan/feature/spine-model.test.ts src/plan/api/page-key.test.ts src/plan/chat/viewed-document.test.ts src/plan/concept/concept-route.test.tsx src/plan/concept/page-heading-metaplan.test.tsx`,
from `web/`. The frozen contracts run in the main agent's gate.

## Report

Files changed; the test run and its result; each pinned test you changed and why; every `rg`
hit left outside your files.
