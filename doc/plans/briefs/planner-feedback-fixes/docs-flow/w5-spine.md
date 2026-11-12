# W5: spine heads, running dot and design record (codex sol, high)

Read `common.md` beside this file first. Decisions D2 (the spine part), D3 and D4 are authoritative.

## Goal

1. The Product spec and Plans heads stop showing a green check while any of their documents is
   unsettled. They show the amber attention glyph instead.
2. A running plan's row pulses a green dot (`LiveDot`); the roundel stays only on the header pill.
3. The Plans group has no Overview child and links to the metaplan page.

## Files you own

- `web/src/plan/feature/spine-model.ts`, `spine-row.tsx`, `spine.css`, `spine.tsx`
- Spine tests: `web/src/plan/feature/spine.test.tsx`, `spine-groups.test.tsx`,
  `spine.contracts.test.tsx`, `spine-model.test.ts`, `web/src/plan/trace/spine-marks.test.tsx`
  and `web/src/plan/trace/trace-ui.contracts.test.tsx`
- `web/src/plan/ui/DESIGN.md` (the sign section only), `web/src/plan/ui/activity-glyphs.css` and
  `web/src/plan/ui/activity-glyphs.tsx` (comments only)
- `web/src/plan/help/legend-entries.tsx` (only if the legend describes the spine heads or the
  running row)

## Steps

1. `spine-model.ts` `spineGroups` (about lines 328 to 351) and `groupGlyph` (about line 318). For
   the product and plans groups, after `children` are built: if `mark === "done"` and any child's
   `glyph` is non-null with a kind outside settled, done and retired, the group's `glyph` is
   `{ kind: "attention", word: "Not all settled" }`; otherwise null. Children with a null glyph
   (no trace yet) do not count. Vision and metaplan keep `groupGlyph`.
2. `spine-row.tsx` group head (about line 139): when `group.glyph` is non-null, draw the row glyph
   the vision and metaplan heads draw, the amber `BadgeAlert` for attention. Otherwise draw
   `STEP_MARK_GLYPHS[group.mark]` as today. Keep the `data-mark` attribute. Add `data-glyph`
   for the attention case, matching how the vision and metaplan heads expose theirs.
3. Plans group, D2. In `GROUPS` (about line 109), the plans page is `metaplan`. Remove the plans
   overview child (`overviewChild`, about lines 172 to 185, 307 to 313 and 348). Set `overview` to
   true only for product. Check `GROUP_OF_PAGE` and `groupOfPathname`: `/f/<f>/metaplan` still
   belongs to the metaplan group. A pathname under `engineering/<doc>` or `plans/<slug>` belongs
   to plans. Read `spine-row.tsx:187` (`selected={pathname === group.to && !group.overview}`) and
   keep the selection correct for both heads that point at the metaplan page: only one row may
   show selected. The metaplan row wins.
4. Running dot, D4. In `spine-row.tsx` `GlyphMark` (lines 43 to 55), replace the `BusyRoundel`
   branch with `<LiveDot />` from `@/plan/ui/activity-glyphs`. Delete the `BusyRoundel` import
   (line 5) and the CSS rule `.plan-spine__glyph[data-glyph="running"] > .aurora-roundel-wrap`
   (`spine.css` lines 226 to 231). Size the dot to the 14px glyph box with a rule on
   `.plan-spine__glyph[data-glyph="running"] > .activity-glyph`. `LiveDot` is green through
   `--tone-completed` and pulses through `activity-glyph-waiting`, already reduced under
   `prefers-reduced-motion`.
5. `DESIGN.md`: sign 1 ("drawn in two places") becomes one place, the header pill. Remove the
   parenthetical in sign 3 about the spine's roundel. In the paragraph after sign 5, the roundel
   marks only the header activity pill. Add one sentence on the attention head. Fix the comments in
   `activity-glyphs.css:19` and `activity-glyphs.tsx:117`.
6. Tests: flip `spine-marks.test.tsx` lines 180 to 193 and `trace-ui.contracts.test.tsx` lines 227
   to 234 so they assert `.activity-glyph__led` is present and `.aurora-roundel` is absent. Add
   cases for the amber head with one revised section, the green head with all settled, the same
   two for plans, and the plans head linking to metaplan with no overview child. Remove assertions
   about the plans Overview child and the `engineering` page.

## Traps

- `header/activity.tsx` and its tests are not yours and must keep the roundel.
- The frozen `spine-settle.contracts.test.ts` is the spec. Do not edit it.

## Done

`cd web && node_modules/.bin/tsc -b --noEmit` is clean, and `node_modules/.bin/vitest run
src/plan/feature src/plan/trace` passes if vitest runs for you. Report the files changed.
