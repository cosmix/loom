# W3: Coverage and Findings pages (trace-ui, wave 1)

Read `../design.md` in full: its "Coverage page" and "Findings page" sections are your
specification. The pages exist from `PLAN-planning-trace`'s `trace-web-maps` stage. Rework them; do
not start over. Keep the data loading (`TraceGate`, `useLedgerHash`, `findings-actions.ts`) and the
finding actions' behaviour. You write markup and behaviour; V2 sets the look after you.

## Files you own

- `web/src/plan/trace/`: `coverage-page.tsx`, `coverage-model.ts`, `coverage-model.test.ts`,
  `coverage-keys.ts` (delete), `findings-page.tsx`, `findings-cards.tsx`, `findings-model.ts`,
  `findings-model.test.ts`, `pages.test.tsx`
- `web/src/plan/help/maps-help.tsx` and its test, `web/src/plan/help/topics.ts`,
  `web/src/plan/help/shortcuts.tsx`
- `web/src/plan/feature/run-page.tsx`, `web/src/plan/feature/gaps.tsx`

## Tasks

1. **Coverage model.** Replace the column and lineage model with sections:
   - `buildCoverage(trace, feature)` returns `{ tiles, sections }`.
   - `tiles` hold counts for all, uncovered, changed and open.
   - Each section has `{ number, title, path, settle, written, covered, total, rows }`. Each row has
     `{ anchor, title, chains, open, state }`.
   - `chains` is one entry per covering chain: `{ entry, plan (short slug), planState, stage }`.
   - `state` is one of `built`, `planned`, `changed`, `uncovered`.
     - `changed` when a covering plan's doc reads `upstream_changed`.
     - `built` when every covering plan is done.
     - `uncovered` when nothing covers the heading.
   - `filterRows(model, filter)` takes the filters `all`, `uncovered`, `changed` and `open`.
   - Remove `lineage`, `visibleKeys`, `COLUMNS` and their tests.
2. **Coverage page.**
   - Header: title, standfirst, and four tile `button`s with `aria-pressed`, named "All headings",
     "Not covered", "Changed since covered" and "With open findings", each holding its count.
   - Body: sections, each with a header and a column-header row. Rows share one grid.
   - Each row's toggle is a `button` with `aria-expanded`, named after the heading.
   - The expansion has the Why, How and Findings columns. Why quotes the section's vision row,
     using `stub-text.ts` the way the scope line does.
   - Row state words: Built, Planned, Changed since covered, Not covered.
   - A row with the `changed` state shows the amber note with See what changed (reuse
     `settled-changes.tsx` for the covering plan).
   - Arrow up and down move between row toggles. Remove the `h`/`l` keys, `coverage-keys.ts`, the
     focus panel, and their help topic text; `maps-help.tsx` must describe the new page.
   - `#<section anchor>` in the URL scrolls to that section.
3. **Findings page.**
   - Header: a tally line ("3 open · 2 planned · 1 folded · 1 dismissed", omitting zero parts) and
     the Review the feature button (the existing offer).
   - Groups: "Run of <short slug>" (the slug from the run review stem, with `PLAN-`, `REVIEW-` and
     the feature prefix stripped), ordered newest first as design.md defines, then "Reviews".
   - Open findings render as cards. Decided ones fold to one line with the outcome in words:
     "Planned in <plan>", "Folded into <entry>", "Dismissed: <reason>".
   - Fold into entry's picker lists `foldableEntries`, the suggested entry first (the entry whose
     `covers` include the most of the finding's About nodes), and a note naming what is left out.
     Its confirm names the entry.
   - All finding text stays plain text.
4. **Entry points.**
   - A "Coverage" link beside the product overview's gaps block (`feature/gaps.tsx`, the `Gaps`
     component, when its scope is the product).
   - A "Findings · n open" link on the Run page (`feature/run-page.tsx`), with the count from
     `openCount`, shown when the trace has any findings.

## Tests

- `coverage-model.test.ts`: the states, the tiles, the filters, and a section that is not written.
- `findings-model.test.ts`: group order, short labels and the decided-outcome text.
- `pages.test.tsx`: tile filtering, row expansion, arrow keys, the fold picker's suggested entry, the
  Dismiss reason gate, and the two entry links.
- Never edit a `*.contracts.test.tsx`. The frozen contracts "dismiss needs a reason and posts the
  status", "Plan this seeds Plan follow-ups with the finding" and "finding markup renders as text"
  must keep passing. The orchestrator handles the superseded ones.

## Done

`cd web && node_modules/.bin/vitest run src/plan/trace src/plan/help src/plan/feature` passes,
apart from the tests the orchestrator lists as superseded. tsc, oxlint and oxfmt are clean on your
files, and every file is under 400 lines. Report the class names and data attributes V2 must
style, and the CSS rules in `maps.css`, `maps-columns.css` and `maps-findings.css` whose markup you
removed.
