# V2: the summary statistics in Settings (issues 12, 13; D14)

Tier: opus at effort medium (`loom-senior-software-engineer`, `effort: medium`).

Before anything else:
1. Invoke `frontend-design:frontend-design` with the Skill tool.
2. Read `common.md` (Design rules, D14, the frozen table row that is yours), `web/src/plan/ui/DESIGN.md`, and the footer and Settings lines in `doc/loom/knowledge/architecture/planner-web-shell.md` "Header and dock".

## The complaint

In Settings → Summaries, the statistics look poor:
- `SummaryPanel` (`web/src/plan/summaries/summary-panel.tsx`) is a hard-coded 13px stack;
- "Last rebuild" is one run-on `fresh N · cache reads N · output N` line;
- "Token totals" is a `dl` of Today and 7 days with the same run-on lines;
- the provider limit meters sit underneath.

## Fixed targets

1. Remove `QuotaMeters` from `SummaryPanel` (issue 13). The `SummariesGroup` hint in `settings/settings-sections.tsx` drops "provider limits". The footer is the only place the limits show.
2. Redesign the statistics as a compact figure layout that reads at a glance:
   - Last rebuild, Today and 7 days as columns or rows of labelled figures (fresh, cache reads, output), tabular numerals aligned on the digit;
   - one hairline at most, per DESIGN.md "Surfaces";
   - the running rebuild ("Rebuilding N of M") and the read-only note keep their places above.
   - The 390px dialog must not scroll sideways.
3. Keep what tests read:
   - the `section` regions named by their `h3` ("Last rebuild", "Token totals");
   - the `dt` "Today" and "7 days" paired with their figures;
   - "unknown" for a missing figure and 0 for zero.
   - `summary-panel.test.tsx` reads `getByText("fresh 1,840 · cache reads 9,200 · output 760")` and `Today`'s parent text. When the layout splits a line into figures, rewrite those assertions to read each labelled figure, and name them in your report.
4. The loading state in `SummariesGroup` ("Loading summaries") uses V1's `BusyText` (`web/src/plan/ui/busy-text.tsx`, D10). If V1 has not created it when you reach this step, leave the text as is and report it; the main agent wires it after the wave.

## Planned frozen edit (yours, exactly as common.md words it)

`web/src/plan/summaries/contracts.test.tsx`, "panel shows running rebuild, last cost, totals and quota": drop the three quota `img` assertions and add the `queryByRole("img", { name: /window .*% used/ })` null check. Nothing else in that file changes; it is at 390 lines, so add no other line.

## Contract (written before you start; make it pass, do not edit it)

`web/src/plan/settings/polish.contracts.test.tsx`, "Settings shows the summary statistics and no provider limits".

## Files

Your row in the plan's worker table. Report each deliberate exception to DESIGN.md as `exception: <surface>: <rule>: <why>`; V1 owns DESIGN.md.

## Run once at the end

`node_modules/.bin/vitest run src/plan/summaries src/plan/settings`, from `web/`.
