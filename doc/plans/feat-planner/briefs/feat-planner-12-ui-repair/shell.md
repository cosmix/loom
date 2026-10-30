# Stage shell

Read `common.md` first. Working directory `web/`; paths below are relative to `web/src/plan/`
unless they start with `web/`.

Target, from the mockup's Shell tab note: "The shell matches the dashboard. Loom's logo, the
eyebrow-and-title identity, the segmented view switch, and on the right the activity pill, theme
toggle, settings and help. The seven steps sit in the dashboard's counter cells, with the next
action as one card beside them."

Defects the browser tour found, which this stage closes:
- the header is two or three rows that shift between routes;
- the feature title is truncated to about 35px;
- a finished plan's run marquee ("DONE-PLAN-… · daemon stopped · 0/10 merged") sits in the header permanently;
- a "Changed since you looked · 314" control and a "0/12" rebuild counter sit beside it;
- there are no avatars;
- Settings and Help have no labels;
- rail cells show ordinals 1-7 and do nothing on click;
- the next-step button sits under its sentence;
- the activity popover is anchored over the logo;
- on a 390px phone the header takes about 39% of the screen;
- the chat panel shows on the home screen.

## H1: app bar, identity, right cluster, dock (sonnet)

**Files owned:**
- `header/header.tsx`
- `header/feature-switcher.tsx`
- `header/presence.tsx`
- `header/tabs.tsx`
- `header/header.css` (new; import it from `header/header.tsx`)
- `header-extras.tsx`
- `dock.tsx`
- `header/header.test.tsx`
- `header/feature-switcher.test.tsx`
- `header/presence.test.tsx`
- `header/tabs.test.tsx`
- `shell.test.tsx`

**App bar** (`header.tsx:92-115`):
- A grid `auto minmax(0,1fr) auto`, gap `14px 20px`, padding `14px 20px 16px`.
- Background `linear-gradient(to bottom, var(--card), var(--background))`, 1px `--hairline`
  bottom border.
- Drop the `max-w-[1920px]` wrapper's own padding.

Left to right:
1. `Logo` (`web/src/components/logo.tsx`) at 34px high in `--foreground`, inside the existing
   `<Link to="/">`. Today it is `h-5` (20px).
2. Identity block: a grid, gap 2px, `min-width: 150px`:
   - the eyebrow (common measures; copy per `common.md` decision 3, chosen from the route with
     `useMatch`);
   - the feature switcher button: 17px, weight 600, letter-spacing -0.01em, `ChevronsUpDown` in
     `--muted-foreground`, no truncation at desktop widths (`white-space: nowrap`);
   - the title is the feature's `title` from the checklist (`projectAtom.checklists`), the
     project name on `/`, or "Untitled" on `/new`;
   - keep the switcher's `aria-label` "Switch feature, current: <slug>" (pinned in
     `feature-switcher.test.tsx`), and show titles in its popover list with the slug in mono
     muted beside each.
3. Right cluster: flex, gap 8px, wrap, `justify-content: flex-end`, in this order:
   - activity pill (`ActivityPill`, owned by H3);
   - view switch;
   - presence avatars;
   - Share;
   - `ThemeToggle`;
   - Settings;
   - Help.

   Settings and Help are icon buttons (common measures) with the icon plus a visible text
   label from `sm` up; keep their `aria-label`s.

**Second row** (feature routes only):
- Spans the grid (`grid-column: 1 / -1`): flex, `align-items: stretch`, gap 12px, wrap.
- Holds the step rail (H2) and the next-step card (H2) as siblings; the card grows
  (`flex: 1 1 280px`).
- Remove the `max-w-md` cap on the card wrapper (`header.tsx:76`).
- Under the rows, mount `<PressureStrip />` from `header/pressure-strip.tsx` (H3 writes it;
  signature `export function PressureStrip(): JSX.Element | null`).

**View switch** (`header/tabs.tsx`):
- Restyle to the segmented control in `common.md`: 26px items, radius 9/7, `--card` fill,
  pressed tint primary 26%.
- Glyphs stay `House`, `Package`, `Cog`, `FolderTree`.
- Put the planner's rules in `header/header.css` under a planner-only class (`.plan-view-switch`).
  Do not edit `web/src/header.css`: the dashboard shares `.view-switch`.
- Keep the 40px touch rule and `aria-current="page"`.

**Presence** (`header/presence.tsx:41-72`):
- Replace the `UsersIcon` plus count trigger with an avatar stack: inline-flex, gap 4px.
- Each avatar is a 26px circle, 10.5px/600, letter-spacing 0.02em, text
  `oklch(0.2 0.02 260)`, showing the viewer's initials from `name`. The background is
  `oklch(0.82 0.11 <hue>)`, where hue is a stable hash of `public_id` mod 360.
- Show at most three, then a `+N` circle; the authoring holder gets a 2px `--ring` ring.
- Keep the popover, its contents, and the `aria-label` "People here: N" (`presence.test.tsx:32,54`).

**Header extras** (`header-extras.tsx`):
- Reduce it to `<HandoffMounts />` and, when compact, `<ChatSheetButton />`.
- `RunStrip`, `ChangedSinceControl` and `SummaryStatus` leave the header; H3 renders them in the
  activity popover. Update `header.test.tsx` assertions that expected them in the header.

**Dock** (`dock.tsx:14-23`):
- Render `ChatPanel` only when `pathname` is neither `/` nor `/new`.
- `ChatRegistrations` stays on every route.
- Update `shell.test.tsx` and any test asserting the home chat panel.

**Narrow widths** (below `lg`):
- Row one keeps the logo, identity, activity pill (count only), and an overflow menu holding
  view switch, presence, Share, theme, Settings and Help.
- Row two shows the rail collapsed to "Step n of 7 · <label>" plus the next-step card's button.
- At 390px the header must stay within 160px on a feature route.
- Keep `header.test.tsx:175-202` behaviour (narrow collapse) or update it with this layout.

Proof: `node_modules/.bin/vitest run src/plan/header src/plan/shell.test.tsx`, once.

## H2: step rail and next-step card (sonnet)

**Files owned:**
- `header/step-tracker.tsx`
- `header/step-rail.css`
- `header/next-step.tsx`
- `header/step-tracker.test.tsx`
- `header/next-step.test.tsx`
- new `header/rail-popover.tsx` (the narrow rail list; do not reuse `feature/step-list.tsx`, which the features stage rebuilds)

**Rail container:** flex, wrap, 1px `--hairline` border, radius 10px, `overflow: hidden`, `--card` fill.

**Cell:** a `<Link to="/f/<feature>?step=<n>">` inside the existing `<li>`. It is a grid with
two auto columns, column-gap 7px, padding `7px 14px 8px`, `min-width: 94px`, left-aligned, and a
1px `--hairline` right border (none on the last cell).
- Glyph: `STEP_GLYPHS[n-1]` at 15px in `--muted-foreground`. A done cell shows the
  circle-check in `--tone-completed`.
- Counter: `step.count ?? "—"`, 17px/600, `tabular-nums`.
- Label: `step.label`, spanning both columns, 11.5px, `--muted-foreground`, `nowrap`.

States:
- done: as above.
- current (`aria-current="step"`): background
  `color-mix(in oklch, var(--primary) 16%, transparent)`, `box-shadow: inset 0 -2px 0 var(--primary)`,
  and the glyph in `--foreground`. Replace the `border-b-2`/`bg-muted` pair.
- live: step 7 while a plan runs. Show `<ActivityGlyph kind="running" />` in
  `--tone-executing` in place of the glyph.

Keep the sr-only ", done" suffix. Rewrite `step-tracker.test.tsx:50-85`, which pins the ordinal
counter and the border classes, to the new contract: the counter text is `count` or "—", and
the current cell has the inset-shadow class.

**Next-step card** (`next-step.tsx:51-71`):
- flex, `align-items: center`, gap 12px, `flex: 1 1 280px`, `min-width: 0`, padding
  `8px 10px 8px 12px`, 1px DASHED `--border`, radius 10px. No fill.
- Content: a large DocMark (30px) of the offer's document kind; a text block with a title
  (13px/600, `--foreground`) over one sub-line (12.5px, `--muted-foreground`); and the offer's
  button (primary, 30px) on the right with `margin-left: auto`.
- Split `offerSentence` (`next-step.tsx:9-45`) into `{ title, detail }` per offer kind. For
  example, the pressure-test offer gives "Pressure-test the next plan" over "3 plans are
  untested. You can also start a run.", using the checklist's `untested` count. Every existing
  kind gets a short imperative title and a one-line detail.
- Keep the `OfferButton` behaviour.

Proof: `node_modules/.bin/vitest run src/plan/header/step-tracker.test.tsx src/plan/header/next-step.test.tsx`, once.

## H3: activity pill and popover, glyphs, DocMark, pressure strip (sonnet)

**Files owned:**
- `header/activity.tsx`
- `header/use-activities.ts`
- `header/popover.tsx`
- `header/pressure-strip.tsx` (new)
- `ui/activity-glyphs.tsx`
- `ui/activity-glyphs.css`
- `ui/doc-mark.tsx`
- `run/run-strip.tsx`
- `review/changed-since.tsx`
- `summaries/summary-status.tsx`
- their tests: `header/activity.test.tsx`, `header/use-activities.test.tsx`,
  `ui/activity-glyphs.test.tsx`, `ui/doc-mark.test.tsx`, `run/run-strip.test.tsx`,
  `review/changed-since*.test.tsx`, `summaries/summary-status*.test.tsx`

**Pill** (`activity.tsx:47-75`):
- A bordered pill (common measures) with the most urgent operation's glyph, "N in progress",
  then `ChevronDown` (`ChevronUp` while open).
- With no operation it reads "Idle" with a still `CircleCheck` in `--tone-completed`. The pill
  stays mounted because the popover holds the run, review and summary rows.
- Keep the `aria-label` pattern and the dialog and list names "In progress"
  (`activity.test.tsx:85-115`).

**Popover:**
- Aligned to the pill's right edge (`align="end"`), `top` 8px below it.
- Width `min(360px, 86vw)`, padding 6px, 1px `--border`, radius 12px, `--card` fill.
- Shadow: `--plan-shadow` plus `0 18px 50px oklch(0 0 0 / 0.18)`.
- Pass a `className` through `header/popover.tsx` so other popovers keep their size.

**Rows:**
- Grid `26px minmax(0,1fr) auto`, gap 10px, padding 8px, radius 8px, 12.5px, hover `--muted`.
- A 26px glyph tile (radius 7px, `--muted` fill, 15px glyph) in the operation's tone:

  | Operation | Tone |
  | --- | --- |
  | running, summaries, codex, starting | `--tone-executing` |
  | pressure, drafting | `--ring` |
  | waiting | `--tone-completed` |
  | permission | `--tone-warning` |

- The title is 13px/600; the subject is `--muted-foreground`; the time is mono 11.5px,
  right-aligned, in the form `m:ss` under an hour and `Nm` above it.
- Titles carry context (`use-activities.ts:31-120`):
  - "Pressure test · round r of R" (from `PressureRunView`);
  - "N summaries rebuilding";
  - "Drafting <document>";
  - "Plan running" with subject "<plan> · M of N stages merged" (`runStripAtom`) and its
    `since`;
  - "Waiting for your answer" with the tool as subject.

Below the operations, a hairline-separated footer:
- the `loom run` row (the former `RunStrip` content, compact, with its "Open dashboard" link) when
  a run is known;
- `ChangedSinceControl` in its compact form;
- `SummaryStatus` in its compact form.

Each footer item may need a `variant="row"` prop; add it, and keep their existing behaviour.

**Glyphs** (`ui/activity-glyphs.*`):
- `.ag` size 1.15em; waiting LED in `--tone-completed`; permission rotation origin `50% 10%`.
- Gauge SVG: an arc with ticks and a needle on `transform-origin: 12px 15px` sweeping ±55deg over
  1.8s alternate.
- Running ring: `inset: -2px`, 1.5px border, scale 0.7 to 1.5 over 1.6s.
- Keep the reduced-motion block.

**DocMark** (`ui/doc-mark.tsx:18-50`): radius 5px (8px large) and fill 16% in light, 20% in dark
(`color-mix(in oklch, var(--doc-N) 16%, transparent)`, with a `.dark` override). Sizes are
unchanged.

**Pressure strip** (new `header/pressure-strip.tsx`, rendered by H1). Returns null unless a
pressure run is active (`usePressureRun()` in `chat/pressure/use-pressure.ts`). Otherwise a
full-width bar:
- flex, gap 10px, padding `8px 20px`, 1px `--hairline` bottom border, fill
  `color-mix(in oklch, var(--tone-executing) 8%, transparent)`.
- Contents, in order:
  - the gauge glyph;
  - bold "Pressure-testing <plan title>";
  - muted "round r of R · <step>";
  - a 5px progress bar (radius 99, max 260px wide, fill `--tone-executing` at the share of
    finished steps);
  - a mono live elapsed (`LiveElapsed`).

Proof: `node_modules/.bin/vitest run src/plan/header src/plan/ui src/plan/run src/plan/review src/plan/summaries`, once.

## H4: settings dialog (sonnet)

**Files owned:**
- `settings/**`

Do not change `web/src/components/theme-picker.tsx` (the dashboard shares it).

**Dialog** (`settings/settings-dialog.tsx:238-277`):
- Width `min(860px, 100%)`, radius 14px, `--card` fill, max height 90dvh with the body
  scrolling.
- Head: padding `14px 18px`, 1px `--hairline` bottom border, the `SlidersHorizontal` icon,
  "Settings" at 15px/600, and the close button.
- Body: padding 18px, gap 22px.

Four groups. Each header is a bold 13.5px title plus a muted 12.5px hint on the same line:

| Title | Hint |
| --- | --- |
| Theme | Remembered in this browser |
| Reading face | Document text only; titles and controls stay Inter |
| Reading size | none |
| Reading | none |

"Reading" replaces "Reading toggles". Drop the dialog description "Saved in this browser" and
keep a visually hidden `DialogDescription` with the same text.

**Theme:**
- `ThemePicker` with `swatch={PlannerThemeSwatch}`, cards in a wrap, gap `14px 12px`.
- Selected card: `--primary` border plus `0 0 0 2px var(--primary)`.
- The caption under each name reads "light · hue 102" style text, from the palette data.
- **`PlannerThemeSwatch`** (`settings/planner-swatch.tsx`, 120x76 viewBox) draws, in the
  palette's colours:
  - a header bar with three logo strokes, an identity bar, a segmented control and two
    avatar dots;
  - a 7-cell rail with four done dots and one current cell in the accent;
  - a dashed next-step box with a primary button;
  - a reading column with a title bar, three text lines, one ink-washed sentence span with a
    cap at each end, and two marks;
  - a chat card on the right with a bubble and a composer.

  Keep `data-region="steps"` on the rail group.

**Reading face:** three face cards (`button aria-pressed`, or keep the radio inputs visually
hidden inside them; keep the radiogroup "Reading face"). Each card has:
- a 66px specimen tile with "Aa" at 36px over "Ωω" at 20px muted, in the face;
- the face name, and the caption "Latin, Greek, Cyrillic";
- on Source Serif 4 only, a "Default" pill flag placed at top -10px, right 10px, with a
  `--primary` fill.

**Reading size:**
- A segmented control "Compact 17px | Comfortable 19px | Large 21px". Keep a radiogroup
  "Reading size" (visually hidden radios inside the segments).
- A preview paragraph in the reading face at the chosen size, reading: "The app stores no
  workflow state and works out the current step from the earliest gap in the doc/ tree." Then a
  Greek line: "Η εφαρμογή δεν αποθηκεύει κατάσταση ροής εργασίας."

**Reading:** three switch rows, each a title plus a muted subtitle, and a 34x20 switch
(`role="switch"`, checked fill `--primary`, 16px knob):
- "Show source marks after sentences" / "The coloured squares that say which document a sentence draws on"
- "Rebuild summaries automatically" / "Up to 5 at a time; above that the app asks first"
- "Notify when a session needs me" / "A browser notification for permission prompts and finished runs"

Update `settings/settings-dialog.test.tsx:70-180` and `planner-swatch.test.tsx` for the new
copy and controls. Their behavioural assertions stay: preferences persist, the size applies,
and the toggles flip their atoms.

Proof: `node_modules/.bin/vitest run src/plan/settings`, once.
