# W6c: web sweep, remaining modules and tests (codex terra, high)

Read `common.md` beside this file first. Decision D2 is authoritative. This job is the same sweep as
W6b, over the files W6b does not own.

## Files you own

- `web/src/plan/chat/chips-labels.test.tsx`, `chips.test.tsx`, `composer-context.test.tsx`,
  `context-line.test.tsx`, `session-home.test.tsx` and `summary-link.test.ts`
- `web/src/plan/files/doc-heading-view-summary.test.tsx`, `heading-parts.tsx` and
  `section-chips.ts`
- `web/src/plan/header/feature-switcher.test.tsx`, `header.test.tsx` and `tabs.test.tsx`
- `web/src/plan/home/features.contracts.test.tsx` and `plan-href.ts`
- `web/src/plan/review/review.test.tsx`
- `web/src/plan/tabs.test.ts`
- `web/src/plan/trace/coverage-model.test.ts`, `coverage-page.test.tsx`, `doc-model.test.ts`,
  `doc-parts.test.tsx`, `map-page.tsx` and `model.test.ts`

## What to do

Run `rg -n 'overview/[^/"]+/engineering|Engineering overview|"engineering"|/engineering' <file>`
on each file and judge every hit:

- **Per-plan pages stay:** `/f/<f>/engineering/<doc>`, `engineering/:doc`, `Scope` "engineering"
  meaning plan scope. Most hits are these. Leave them unchanged.
- **The engineering overview goes:**
  - the page `/f/<f>/engineering` with no doc;
  - the summary id `overview/<f>/engineering`;
  - a link, label or href that lands on the Plans overview.

  Point such a link at `/f/<f>/metaplan`. Delete a test case that only exercises the overview. In a
  test fixture that lists both overviews, keep the product one.

A file with no overview hit stays unchanged; say so in your report.

## Done

`cd web && node_modules/.bin/tsc -b --noEmit` is clean for your files, and `rg -n
'overview/[^/"]+/engineering|Engineering overview'` prints nothing in them. Report, per file:
unchanged, or what changed.
