# W1: trace state, models, spine marks and frontier hints (trace-web-documents, wave 1)

Read `../common.md` "The browser" in full. You build the foundation W1B builds on, plus the
spine. V1 sets the look later; use semantic markup and the accessible names the contracts read
(`Settled`, `Draft`).

## Files you own

`web/src/plan/trace/state.ts`, `state.test.ts`, `model.ts`, `model.test.ts`,
`frontier-hints.tsx`, `spine-marks.test.tsx`, `web/src/plan/feature/spine.tsx`,
`spine-model.ts`, `spine-row.tsx`, `next-action.tsx`, `doc/product/feat-planner/section_05.md`.

## Tasks

1. **State.** `traceAtom(feature)` and `useTrace(feature)` in `trace/state.ts`, following
   `concept/page-state.ts` (per-key ticket, keep the loaded view on a failed refetch, drop on
   404). Subscribe to `trace_changed` and resync on the plan socket the way `subscribeConcept`
   does.
2. **Models** (`trace/model.ts`, pure, exported for W1B): `settleLabel(state)`;
   `docOfPath(trace, repoPath)`; `chipsFor(trace, sentence, pageKind)` (common.md "Edge chips";
   plan pages read the stage from a `Source` citation's `stage:<id>` anchor);
   `frontierLinks(trace, feature)` (at most five, each `{ text, to }`, with the copy below);
   `impactFor(trace, changedPaths)` (common.md "Turn footer"); `bundleLinks(trace, planPath,
   feature)` (each brief with its `featureSourceHref`). Frontier copy: Revised "<title> changed
   since you settled it", StubChanged "<title>'s stub changed since you settled it",
   UpstreamChanged "The spec under <title> changed since you settled it", Revisit "Revisit
   <entry> before writing its plan" (unwritten) or "Revisit <plan> before running it" (written),
   Settle "Read <title> and settle it", Uncovered "<heading> is not covered by any plan", Triage
   "<n> open findings".
3. **FrontierHints** (`frontier-hints.tsx`): the links from `frontierLinks`, rendered under
   `<NextAction />` in `spine.tsx`.
4. **Spine marks.** `spine-model.ts` gains the settle state on `SpineChild` (and on the vision and
   metaplan rows) from the trace; `spine-row.tsx` draws it beside the existing glyphs with its
   word in the accessible name (`sr-only` and `title`, as the pressure badge does). Keep every
   existing glyph and the slug's width.
5. **Spec.** `section_05.md`: settle state on summary pages, the stub panel and brief list, edge
   chips, and that summaries are built for drafts and settled documents alike.

## Tests

`state.test.ts` (refetch on `trace_changed`, failed refetch keeps the view, 404 drops it),
`model.test.ts` (every frontier kind and the cap, chip lookup by heading anchor, by whole section
and by stage citation, impact for a changed section, metaplan and plan, bundle links), and
`spine-marks.test.tsx` (marks on rows, hints under the next step). The frozen contracts in
`trace/trace.contracts.test.tsx` that cover the spine and hints must pass; the others pass after
W1B. Never edit a `*.contracts.test.tsx`. `spine.test.tsx` is already over 400 lines: add no
tests to it.

## Done

tsc, oxlint, oxfmt and `vitest run src/plan/trace src/plan/feature` pass; report the exported
model signatures for W1B.
