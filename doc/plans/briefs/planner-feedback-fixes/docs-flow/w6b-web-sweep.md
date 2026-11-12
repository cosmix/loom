# W6b: web sweep of engineering overview references (codex terra, high)

Read `common.md` beside this file first. Decision D2 is authoritative.

## Goal

No web module outside the router and feature pages still knows the engineering overview page or
the summary id `overview/<f>/engineering`. Per-plan pages under `engineering/:doc` keep working.

## Files you own

- `web/src/plan/api/page-key.ts`, `page-key.test.ts` and `schema-concept.test.ts`
- `web/src/plan/concept/**`, including its tests
- `web/src/plan/files/doc-heading-view.tsx`
- `web/src/plan/chat/viewed-document.ts`, `viewed-document.test.ts`, `chip-label.ts` and
  `context-prompts.tsx`
- `web/src/plan/help/shortcuts.tsx`

## Anchors

- `page-key.ts`, lines 9, 43 to 49, 95 and 141: the overview key for the engineering scope. Remove
  it. A plan-scope fallback goes to the metaplan key.
- `concept/concept-route.tsx`: line 56 (the `g e` shortcut moves to the metaplan page), lines 73 and
  148. `concept/empty-states.tsx`: lines 41 and 185 to 188. `concept/page-heading.tsx`: line 74.
  Concept tests: `concept-route.test.tsx`, `concept-route-headers.test.tsx`,
  `concept-route-engineering-empty.test.tsx` (delete it if it tests only the overview),
  `page-heading*.test.tsx`, `document-key.test.tsx`, `page-state-chips.test.ts`,
  `summary-id-of-key.test.ts`, `empty-states.test.tsx`, `concept.contracts.test.tsx`,
  `levels.contracts.test.tsx` and `plan-glyph.test.tsx`.
- `files/doc-heading-view.tsx`: line 74.
- `chat/viewed-document.ts`: line 12 and lines 117 to 124. `chat/chip-label.ts`: lines 37 to 41.
  `chat/context-prompts.tsx`: line 20.
- `help/shortcuts.tsx`: line 30 (`g e` now reads "Metaplan").
- `rg -n 'overview/[^/"]+/engineering|"engineering"|Engineering overview'` in your files lists the
  rest. Judge each hit: per-plan scope stays, the overview goes.

## Traps

- Concept contract test files from earlier plans are ordinary test files now, and you may edit
  them. Remove only the overview cases. Each edit is an expected integrity event.
- `router.tsx` and `feature/**` belong to W6a. If a type you change breaks them, list where in your
  report.

## Done

`cd web && node_modules/.bin/tsc -b --noEmit` is clean for your files, and the `rg` above prints no
overview hit in them. Report the files changed and any breakage outside your files.
