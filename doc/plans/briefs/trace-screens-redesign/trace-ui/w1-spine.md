# W1: the spine, its head, and Firefox screenshots (trace-ui, wave 1)

Read `../design.md` in full: its "Spine" section is your specification. You write markup and
behaviour. V1 sets the look after you, so add the class names and data attributes it will style
and leave the CSS to it, apart from what you must delete.

## Files you own

- `web/src/plan/feature/spine.tsx`, `spine-model.ts`, `spine-model.test.ts`, `spine-row.tsx`
- `web/src/plan/trace/model.ts`, `model.test.ts`, `spine-head.tsx` (new), `frontier-hints.tsx`
  (delete), `spine-marks.test.tsx`
- `web/scripts/plan-screens.mjs`, `scripts/plan-screens.sh`

## Tasks

1. **One glyph per row** (`spine-row.tsx`). Fold `SettleMark`, the plan state glyph and
   `PressureBadge` into one leading glyph chosen in a pure function in `spine-model.ts`.
   - **Order of precedence:**
     1. running: the roundel
     2. attention: an amber seal, for settle `revised`, `stub_changed` or `upstream_changed`
     3. settled: a seal with a check
     4. done plan: a check
     5. not written: a dashed circle
     6. otherwise draft: an empty seal
   - Expose the choice as `data-glyph="<kind>"` on the glyph span.
   - The glyph's word goes in `title` and `.sr-only`, so a row's accessible name still holds
     "Settled", "Draft", "Changed since you settled it", "Spec changed since you settled it",
     "Running" or "Done". The frozen contract "the spine shows each document's settle state" reads
     those words.
   - Drop the pressure flask from rows.
   - Use lucide icons already imported in `ui/glyphs.tsx` where one fits (`BadgeCheck`, `Badge`,
     `BadgeAlert`, `CircleDashed`, `Check`).
2. **Running row.** Render `BusyRoundel` from `@/aurora-ui/feedback/BusyRoundel` with `busy`,
   `busyLabel=""` and `idleLabel=""`, the way `web/src/plan/header/activity.tsx` draws it in the
   header pill. Size it to the glyph box. The word "Running" stays in `.sr-only`.
3. **Slugs.** Add `shortSlug(feature, slug)` in `spine-model.ts`. It strips `<feature>-` when the slug
   starts with it, else returns the slug. Plan rows show the short slug, with the full slug in
   `title`.
4. **No Coverage or Findings rows.** If the merged tree has the `trace-web-maps` additions in
   `spine-model.ts` (`tracePageChild`, coverage and findings in `GROUP_OF_PAGE`), remove the child
   rows. Keep `GROUP_OF_PAGE` mapping coverage to product and findings to run, so those pages
   highlight their group.
5. **Current page.** Remove the gutter dot. The current row keeps `aria-current="page"`. V1 draws the
   highlight.
6. **SpineHead** (`trace/spine-head.tsx`) and `frontierHead(trace, feature)` in `trace/model.ts`.
   - `frontierHead` returns `{ sentence, detail: string | null, label, to } | null` for the first
     frontier gap. The copy:
     - Revised: "<Title> changed since you settled it." with "Review <title>". When a settled
       plan covering it reads `upstream_changed`, the detail is "Plan <short slug> implements it,
       so it is flagged too."
     - StubChanged: "<Title>'s stub changed since you settled it." with "Review <title>".
     - UpstreamChanged: "The spec under <short slug> changed since you settled it." with "Review
       <short slug>".
     - Revisit: "Revisit <entry> before writing its plan." with "Open the metaplan".
     - Settle: "Read <title> and settle it." with "Read <title>".
     - Uncovered: "<Heading> is not covered by any plan." with "Open coverage".
     - Triage: "<n in words, capitalised> findings from the last run are waiting for you." with
       "Triage findings", linking `/f/<feature>/findings`.
   - `SpineHead({ feature })` renders `div.plan-spine-next` holding a `p` with the sentence, an
     optional muted `p` with the detail, and one `Link` with the primary button classes the planner
     uses for a primary offer (read `OfferButton`'s `look="primary"` and reuse its class list,
     not the component, because this is a link).
   - It renders nothing when the frontier is empty or the trace is not loaded.
   - Mount it where `FrontierHints` is mounted in `spine.tsx`, not when `sample`. If the merged
     spine still renders a checklist `NextAction` at its head, render `SpineHead` in its place
     only when `frontierHead` is non-null.
7. **Delete** `frontier-hints.tsx`, `frontierLinks` and `FRONTIER_CAP`, and their tests. Also
   delete `chipsFor`, `ChipSentence`, `EdgeChip` and their tests: W2 deletes `edge-chips.tsx`, their
   only consumer. Keep every other `model.ts` export that some file still imports (check with `rg`
   after W2's report).
8. **Firefox screenshots.** Add `--browser=chromium|firefox` (default chromium) to
   `scripts/plan-screens.sh`, passed to `web/scripts/plan-screens.mjs`. The script's leading flags
   (`--worktree-doc`, `--themes=`, `--browser=`) must be accepted in any order before the binary. Firefox launches through
   `firefox` from `playwright-core`.
   - The host has a Playwright Firefox build in `~/.cache/ms-playwright/firefox-*`. If the
     installed `playwright-core` cannot launch it, report the exact error and leave the flag in.
   - Do not install browsers.
   - Prove the flag by shooting `/` with `--browser=firefox --themes=ledger` once.

## Tests

- `spine-model.test.ts`: glyph precedence, `shortSlug`, and no coverage or findings children.
- `model.test.ts`: every `frontierHead` kind and the empty frontier.
- `spine-marks.test.tsx`: one glyph per row, the amber kind on a revised row, the roundel on a
  running row, the short slug with the full slug in `title`, and `SpineHead`'s sentence and link.
  Remove its frontier-hints tests.
- `spine.test.tsx` is over 400 lines: add nothing to it.
- Never edit a `*.contracts.test.tsx`.

## Done

`cd web && node_modules/.bin/tsc -b --noEmit` may still fail only on `edge-chips.tsx` imports until
W2 finishes; say so in your report if it does.
- `node_modules/.bin/vitest run src/plan/feature src/plan/trace/model.test.ts
  src/plan/trace/spine-marks.test.tsx` passes.
- oxlint and oxfmt are clean on your files, and every file is under 400 lines.
- Report the `data-glyph` kinds, the class names V1 must style, and the Firefox flag result.
