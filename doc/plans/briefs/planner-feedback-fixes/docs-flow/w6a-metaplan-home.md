# W6a: the metaplan page becomes the plans home (codex sol, high)

Read `common.md` beside this file first. Decision D2 is authoritative. In wave 1, W4 created
`SettleAllButton` in `web/src/plan/trace/settle-all.tsx` and W6b swept the page-key and concept
references. Their reports are pasted below this brief.

## Goal

`/f/<f>/engineering` and `/f/<f>/run` redirect to `/f/<f>/metaplan`. The metaplan page carries what
the engineering overview carried: the Run panel, the plan-scope gaps, the pace cards, the plan
queue, and the Settle all plans button. A feature with a metaplan lands on the metaplan page. The
product overview gains Settle all specs.

## Files you own

- `web/src/plan/router.tsx`
- `web/src/plan/feature/landing.ts` and `landing.test.ts`
- `web/src/plan/feature/document-page.tsx`
- `web/src/plan/feature/page-offer.ts`
- `web/src/plan/feature/gaps.tsx` and `gaps.test.tsx`
- `web/src/plan/feature/queue.tsx`
- `web/src/plan/feature/run-panel.test.tsx` and `run-start-control.test.tsx`

## Steps

1. `router.tsx`, lines 22 to 46. `conceptRoutes("engineering")` keeps `engineering/:doc`. The bare
   `engineering` path becomes `<Navigate to="../metaplan" replace />`, and so does `run` (line 46).
   The product route's `above` gains `<SettleAllButton feature={...} role="section" />` beside its
   existing content.
2. `document-page.tsx` `MetaplanPage` (about lines 107 to 117) passes `above` to `ConceptRoute`.
   `ConceptRoute` renders `above` on summary pages (`concept-route.tsx:292-296`). `above` holds:
   - `<RunPanel />`
   - `<Gaps scope="engineering" />`
   - the pace cards and the plan queue, as the overview page rendered them (find where with `rg -n
     'Pace|Queue' web/src/plan/router.tsx web/src/plan/concept`)
   - `<SettleAllButton feature={...} role="plan" />`

   Keep their order.
3. `landing.ts` line 12: `metaplans.length > 0 ? "metaplan" : "vision"`. Update its doc comment and
   `landing.test.ts`.
4. `page-offer.ts` line 9 (`GAP_STEP.engineering`), `gaps.tsx` line 7 and `queue.tsx` about line
   189: page names that pointed at the `engineering` overview now point at `metaplan`. Keep scope
   names that mean per-plan pages.
5. `run-panel.test.tsx` and `run-start-control.test.tsx`: render the Run panel through the metaplan
   page instead of the engineering overview. Keep every assertion about the panel's own behaviour.

## Traps

- `ConceptRoute` for the metaplan uses key `<f>/metaplan/index`. Do not change the key.
- The frozen `metaplan-home.contracts.test.tsx` is the spec. Do not edit it.

## Done

`cd web && node_modules/.bin/tsc -b --noEmit` is clean, and `node_modules/.bin/vitest run
src/plan/feature` passes if vitest runs for you. Report the files changed, and any file outside
your list that still names the `engineering` overview page.
