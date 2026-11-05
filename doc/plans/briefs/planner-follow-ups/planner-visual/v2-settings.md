# V2: the Settings dialog: size pictures, the preview specimen, the statistics panel (R10)

Tier: fable (`loom-senior-software-engineer` with the model override `fable`).

Before anything else:

1. Invoke `frontend-design:frontend-design` with the Skill tool. Its general advice never overrides
   DESIGN.md or this brief's fixed targets.
2. Read `../common.md` and `web/src/plan/ui/DESIGN.md` in full, and
   `doc/loom/knowledge/architecture/planner-web-shell.md` lines 7-22 ("Header and dock": Settings,
   the popover, `SummaryPanel`, the unlayered `settings.css`).

V1 (fable) changes the themes beside you (Walnut, Graphite; `--accent-ink` in `web/src/index.css`:
`--ring` on Ledger, `--primary` in the dark themes) and is the only editor of DESIGN.md: V1's brief
already records your exceptions (tiles, specimen, statistics). Report any further deviation as
`exception: <surface>: <rule>: <why>` and the main agent adds it. The frozen contracts of this
stage (the stage description's PUBLIC SURFACE) are written before you start: make them pass, never
edit them.

## Frozen contracts you must keep green (written before this plan; never edit)

- `web/src/plan/summaries/contracts.test.tsx` "panel shows running rebuild, last cost, totals and
  quota" (lines 178-211). It reads `SummaryPanel`'s text:
  - `Rebuilding 1 of 3`, the running ids, the failure text `no seal in 10 minutes`;
  - the smallest element holding the last rebuild's `1,840` and `9,200` must read the labels
    `fresh`, `cache reads`, `output` each directly before its figure with at most 40 non-digit
    characters between (`fresh 1,840 · cache reads 9,200 · output 760` passes);
  - with the last rebuild's output absent, that element shows `unknown` and no standalone `0`;
  - the text holds `today`, `7 day`, `12,400` (today's fresh) and `74,600` (week's fresh).
  So the last rebuild keeps its labelled fresh / cache reads / output line as its own element, and
  the cards print every component figure in their legend.
- Same file, "panel shows the read-only reason instead of controls": with `read_only` set, no
  enabled button whose name matches `/rebuild/i`.
- `web/src/plan/settings/polish.contracts.test.tsx`: the region "Token totals" exists in the
  dialog; no `img` named `/window .*% used/`.
- `web/src/plan/header/popover-stats.contracts.test.tsx`: the activity popover (which renders
  `RebuildActivity` through `summary-status.tsx` `StatusContent`) shows no region "Token totals".

Tests that are not frozen but read names you keep: `summary-status.test.tsx` (heading
`Rebuilding 1 of 3` in the popover), `summary-panel.test.tsx` (regions "Running rebuild", "Queue",
"Failed rebuilds", "Last rebuild", "Token totals", "Terminal claims"; "None yet"),
`activity-popover.test.tsx` (region "Queue"), `settings-dialog.test.tsx` (headings "Theme",
"Reading face", "Reading size", "Reading", h2 "Summaries"; radiogroups "Reading face", "Reading
size"; radio names `Compact 17px`, `Comfortable 19px`, `Large 21px`; the preview text's parent
element carries `fontFamily`, `fontSize` and `lineHeight: 1.6`).

## 1. Reading size with pictures

`web/src/plan/settings/settings-sections.tsx` `ReadingSizeGroup` (lines 96-120). Move
`ReadingFaceGroup`, `ReadingSizeGroup` and `ReadingPreview` into a new
`web/src/plan/settings/reading-sections.tsx` (settings-sections.tsx is at 242 lines and grows) and
update the import in `settings-dialog.tsx`. New `web/src/plan/settings/size-picture.tsx` exports
`SizePicture({ size }: { size: ReadingSize })` (a reachable check finds it from `SettingsDialog`).

- Each option is a tile: the `label` holds the hidden radio (unchanged: `sr-only`, `name`,
  `aria-label={`${label} ${size}px`}`, `checked`, `onChange`), then `<SizePicture>`, the label and
  the px value. The three tiles sit in one row (`ps-seg` stays on the radiogroup, `ps-seg-item` on
  the tiles, so existing selectors hold). Keyboard behaviour of the native radio group is unchanged.
- `SizePicture` draws `<svg width="40" height="32" viewBox="0 0 40 32" aria-hidden="true">` with
  one page outline `<rect data-page ... rx="3" fill="none" stroke="currentColor">` and the text
  lines as `<rect data-line ... rx=...>` bars filled `currentColor`:
  - 17 (Compact): five thin lines, tight spacing;
  - 19 (Comfortable): four lines, medium spacing;
  - 21 (Large): three thick lines, wide spacing, the first bar shorter than the second (a heading).
  The frozen contract counts `[data-line]` (5, 4, 3), finds one `[data-page]` and compares the
  Large picture's first two `width` attributes.
- Look: unchecked tiles in `--muted-foreground` with a hairline border; the checked tile's border
  and drawing in `--accent-ink` and its label in `--foreground` at 600; hover lifts the drawing to
  `--foreground`; `:focus-visible` on the radio draws the 2px `--ring` outline on the tile (the
  existing `.ps-seg-item:has(input:focus-visible)` rule).

## 2. The preview as a specimen

- `ReadingPreview` renders
  `<figure aria-label="Reading preview" className="ps-preview">` holding an eyebrow
  `<figcaption className="ps-preview-eyebrow" aria-hidden="true">Preview</figcaption>` (small caps,
  `--muted-foreground`; hidden from assistive technology because the label already names the
  figure) and a `div` carrying the style `{ fontFamily, fontSize: size, lineHeight: 1.6 }` around the
  two paragraphs (English, then the Greek line in `--muted-foreground`). The paragraphs' parent is
  that `div`, as `settings-dialog.test.tsx:174` expects.
- Placement: the figure is the first child of the "Reading face" group's body, above the faces
  radiogroup, so it sits above both controls it reflects (face, then size). The group headings
  stay as they are.
- Look: `--background` surface (one step below the `--card` dialog), 1px `--border`, 8px radius,
  `--space-4` padding, a 3px `--accent-ink` rule on its start edge. Remove the old hairline
  treatment of `.ps-preview` (`settings.css` lines 207-218).
- Put the tile and specimen CSS in a new `web/src/plan/settings/reading.css` imported by
  `reading-sections.tsx` if `settings.css` (316 lines) would pass 400.

## 3. Summary statistics as an instrument panel

`web/src/plan/summaries/summary-panel.tsx` (252 lines). Move its Tailwind classes into a new
`web/src/plan/summaries/summary-panel.css`, imported as `import "./summary-panel.css";` (a wiring
check greps it; an after_stage check requires no `space-y-` left in the file). New
`web/src/plan/summaries/token-figures.tsx` holds the cards, the bar and the count-up; export
`TokenBar` from it (a reachable check finds it from `SettingsDialog`). Keep every file under 400
lines and every function under 50.

Wire facts (`web/src/plan/api/schema-summaries.ts`): `TokenVector` has optional `fresh_input`,
`cache_write`, `cache_read`, `output`; `costLine` has `time`, `trigger`, `summaries` (ids),
`level`, optional `model` and `source_bytes`, and `tokens`. There is no duration and no success
flag on the wire: the last-rebuild row shows no duration (user decision), and its status
dot is "failed" when `panel.failed` is non-empty, else "ok".

Component order and labels, used everywhere: `fresh` (fresh_input), `cache reads` (cache_read),
`cache writes` (cache_write), `output`. Figures print with `toLocaleString("en-US")`; an absent
component prints `unknown` in muted text; a recorded zero prints `0`.

- **Totals** (`Section title="Token totals"`, keeps its region name): two cards side by side, each
  a `div role="group"` labelled by its caption (`aria-labelledby` on the caption element "Today",
  "7 days"). A card shows:
  - the total (sum of the recorded components; `unknown` when none is recorded) in the mono face,
    `font-variant-numeric: tabular-nums`, 1.75rem, then `tokens` in small caps under it;
  - one stacked bar: a track holding one segment per component whose value is greater than 0,
    each `<span role="img" aria-label="<label> <figure>">` (for example `fresh 12,400`), widths in
    proportion; segment colours are a monochrome ramp (`color-mix(in oklch, var(--foreground)
    <p>%, var(--muted))` at four distinct strengths), no hue. Nothing else inside the card has
    `role="img"` (the frozen contract counts them);
  - a legend line of all four components with a matching swatch (`aria-hidden`) and its figure
    (`unknown` / `0` as above).
  - Scale: the 7-day bar fills its track. Today's track stands for `max(todayTotal, weekTotal / 7)`;
    today's segments fill their share of it, and a thin `--foreground` tick marks `weekTotal / 7`
    (`aria-hidden`; absent when the week total is 0), so the user sees whether today runs above or
    below the week's daily average.
  - At 390px the two cards stack; nothing scrolls sideways.
- **Count-up**: the two totals count from 0 to their value over 400ms with `requestAnimationFrame`
  (ease-out) once per mount, only when `useMediaQuery("(prefers-reduced-motion: no-preference)")`
  (`web/src/lib/use-media-query.ts`) is true; otherwise, and in every test environment whose
  `matchMedia` stub answers false, the final figure renders on the first paint. Legend and
  last-rebuild figures never animate. Cancel the frame on unmount.
- **Last rebuild** (`Section title="Last rebuild"`, region name kept; "None yet" when `last` is
  null): one row: the status dot (`aria-hidden`, `--tone-completed` ok, `--tone-blocked` failed,
  with the word "Rebuilt" or "Failed" beside it as text), the relative time in
  `<time dateTime={time} title={absolute local time}>` (`relativeTime` from
  `web/src/plan/files/heading-parts.tsx`), the documents rebuilt (`summaries.length`, "1 document",
  "3 documents"), and the labelled `fresh … · cache reads … · output …` line as its own element
  (the frozen contract's cost line).
- **Activity**: `RebuildActivity` stays exported with its current props and keeps its contents and
  names (it is also the activity popover's body): the "Running rebuild" region with its `h3`
  `Rebuilding {done} of {total}`, now with a slim progress line under it
  (`role="progressbar"`, `aria-label="Rebuild progress"`, `aria-valuemin=0`,
  `aria-valuemax={total}`, `aria-valuenow={done}`) and a dot that pulses gently (none under reduced
  motion); the running ids and the log path; "Queue"; "Failed rebuilds" as compact rows (id in mono,
  the error under it) each with a `Retry` button (`DisabledReason` with `authorBlockReasonAtom`,
  `size="sm"`, `onClick` calling `useSummaryActions().rebuild([id])`, its error shown as an alert
  as `RebuildButton` does), drawn only while `panel.read_only === null`; the start error alert.
  `SummaryPanel` adds, when `panel.running === null`, one quiet line `Idle · last run 12 min ago`
  (`Idle · no runs yet` when `last` is null), relative time from `relativeTime`.
- **Terminal claims** keep their content and Release behaviour, styled as compact rows.
- The read-only note keeps its place at the top.

## Tests

- `summary-panel.test.tsx`: rewrite the figure helpers for the new layout (each labelled figure),
  keep every behaviour it checks, and add: the count-up runs with `(prefers-reduced-motion:
  no-preference)` matching (first paint `0`, final figure after fake timers advance 400ms of
  frames); a failed row's Retry posts that id; the Idle line.
- New `token-figures.test.tsx` for `TokenBar` scale (today's tick position for a fixture where
  today is under and over the daily average).
- `settings-dialog.test.tsx`: the theme labels become `["Ledger", "Walnut", "Graphite"]` and the
  swatch count 3 (line 92-103, V1's theme change); adjust the reading-size label test for the
  tiles (text stays `Compact17px` etc. when the SVG has no text).
- `planner-swatch.test.tsx`: rename the local variable `pacific` (lines 55-75) to `other`: an
  after_stage check greps `web/src` for the word.

## Shots

You cannot build or shoot. The main agent opens Settings at 1440x900 and 390x844 in Ledger,
Walnut and Graphite, with the summary panel fed from `web/src/plan/api/fixtures/summary-panel.json`,
and judges:

1. Three reading-size tiles in a row, each with its little page picture (5, 4, 3 lines; Large's
   first line short), the chosen tile edged and drawn in the accent ink.
2. The preview above the face cards: a sunken specimen card with the "PREVIEW" eyebrow and the
   accent rule on its left edge, text in the chosen face and size.
3. Statistics: two figure cards with large tabular totals, a stacked monochrome bar each, a legend
   of four figures, the daily-average tick on Today's bar; one last-rebuild row with its dot; the
   running progress line; failures as compact rows with Retry.
4. At 390px the dialog does not scroll sideways and the cards stack.

## Files

Your row in the stage's worker table. Never edit V1's or M1's files, DESIGN.md, or any
`*.contracts.test.tsx`.

## Run once at the end

From `web/`: `node_modules/.bin/vitest run src/plan/settings src/plan/summaries src/plan/header`

Report: files changed, the test run's result, every test you changed and why, each DESIGN.md
exception beyond V1's list, and anything unsettled.
