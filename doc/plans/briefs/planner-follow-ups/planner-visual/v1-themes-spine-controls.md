# V1: themes, spine glyphs, fold chevrons, Add to chat, DESIGN.md (R9, R7, R11)

Tier: fable (`loom-senior-software-engineer` with the model override `fable`).

Before anything else:

1. Invoke `frontend-design:frontend-design` with the Skill tool. Its general advice never overrides
   DESIGN.md, this brief's fixed targets or the user's values below.
2. Read `../common.md`, `web/src/plan/ui/DESIGN.md` in full, and
   `doc/plans/mockups/chat/alt-d-hearth.html` lines 1-60 (the approved Walnut and Ledger tokens,
   `--accent-ink`). Serve the mockup folder over loopback if you want to look at it; `file://` is
   blocked.

You are the only editor of `web/src/plan/ui/DESIGN.md` in this stage. V2 (fable) redesigns
Settings beside you; M1 (haiku) moves the screenshot scripts and README to the new theme ids. The
frozen contracts of this stage (the stage description's PUBLIC SURFACE) are written before you
start: make them pass, never edit them.

## 1. Themes (R9)

Three themes: Ledger (light, unchanged, amber accent), Walnut (dark, the default dark theme),
Graphite (dark). Aubergine and Pacific go; the dark variants green, slate and sand go (nothing
selects them).

Settled ids (do not rename): the `DarkVariant` ids are `"walnut"` and `"gray"`. Graphite keeps the
id `gray` because the frozen contract `web/src/plan/ui/polish.contracts.test.tsx` ("the Graphite
accent is a bright colour") reads the block whose selector contains `[data-dark-theme="gray"]`; the
orange accent (chroma 0.15) passes it. Walnut is the plain `.dark` block, so a stored variant this
build no longer offers (an old `"aubergine"` or `"blue"` in `loom:dark-variant`) draws Walnut with
no migration code; the picker then presses no card, as the existing test "does not mark an
unoffered dark variant as active" pins.

### `web/src/aurora-ui/shared/styles/tokens.css`

- Header comment (lines 1-18): drop step 5 (`tokens-dark-variants.css`).
- `.dark` block (line 144, "Default dark theme (aubergine, hue 325)") becomes Walnut. Heading
  comment: `Default dark theme (walnut: warm brown-black, hue 55)`. Values:

| Token | Walnut | Graphite |
| --- | --- | --- |
| `--background` | `oklch(0.19 0.014 55)` | `oklch(0.20 0.003 270)` |
| `--card`, `--popover` | `oklch(0.22 0.015 55)` | `oklch(0.23 0.003 270)` |
| `--foreground`, `--card-foreground`, `--popover-foreground` | `oklch(0.92 0.012 80)` | `oklch(0.93 0.004 80)` |
| `--muted-foreground` | `oklch(0.71 0.014 60)` | `oklch(0.71 0.005 270)` |
| `--border` | `oklch(0.31 0.014 55)` | `oklch(0.32 0.004 270)` |
| `--muted` | `oklch(0.25 0.015 55)` | `oklch(0.26 0.004 270)` |
| `--secondary` | `oklch(0.255 0.015 55)` | `oklch(0.265 0.004 270)` |
| `--accent`, `--input` | `oklch(0.28 0.015 55)` | `oklch(0.29 0.004 270)` |
| `--secondary-foreground`, `--accent-foreground` | the foreground | the foreground |
| `--primary`, `--ring`, `--sidebar-primary`, `--sidebar-ring` | `oklch(0.74 0.15 55)` | `oklch(0.74 0.15 55)` |
| `--primary-foreground`, `--sidebar-primary-foreground` | `oklch(0.19 0.014 55)` | `oklch(0.20 0.003 270)` |
| `--sidebar` | the background | the background |
| `--sidebar-foreground`, `--sidebar-accent-foreground` | the foreground | the foreground |
| `--sidebar-accent` | `oklch(0.28 0.015 55)` | `oklch(0.29 0.004 270)` |
| `--sidebar-border` | the border | the border |

  The first five rows are the user's approved values (plan R9 table; the mockup's tokens). The
  rest keep the old dark block's ordering of lightness steps (card < muted < secondary < accent <
  border) at the user's chroma cap: no surface token may exceed chroma 0.015. Keep `--destructive`,
  `--hazard-error` and the `--chart-*` values of the old `.dark` block. `--hazard-warning` moves to
  amber (user decision): `oklch(0.78 0.14 80)` (in sRGB; 8.6:1 on the Walnut card, 8.3:1 on
  Graphite's); if the Graphite block declares its own, give it the same value. Retune
  `--ost-*` and `--postit-*` (unused in loom, defined for the kit's Tailwind colours) onto the same
  neutral axis with chroma at most 0.015. Measured (WCAG relative luminance from OKLCH): Walnut
  foreground on background 14.6:1, muted foreground on card 6.7:1, orange on background 7.7:1, the
  background on orange 7.7:1; Graphite 14.7:1, 6.6:1, 7.5:1, 7.5:1. You may tune a derived row by
  a step; the user's five rows and the accent are fixed and pinned by the frozen palette contract.
- `.dark[data-dark-theme="gray"]` (line 281) becomes the Graphite column above; replace its
  periwinkle comment with one line naming the orange lamp.
- Delete the whole `.dark[data-dark-theme="blue"]` block (lines 326-372).
- Ledger (`:root, .theme-preview`, line 94) is unchanged. The light variants `gray` and `cool` stay.

### Other files

- `web/src/aurora-ui/shared/styles/tokens-dark-variants.css`: delete the file.
- `web/src/index.css`:
  - delete `@import "./aurora-ui/shared/styles/tokens-dark-variants.css";` (line 10);
  - add `--accent-ink: var(--ring);` to the `:root` block (beside `--highlight`, ~line 82) and
    `--accent-ink: var(--primary);` to the `.dark` block (~line 130). Accent-coloured text and
    lines use it so they meet contrast on Ledger (`--ring` is 3.8:1 on the Ledger card; the amber
    `--primary` fill is 1.5:1). V2's tiles and specimen rule, and the later chat and `/new` stages,
    consume it; the frozen palette contract pins both declarations;
  - the doc-colour comment "Against the Aubergine --background" (line 106) becomes "Against the
    Walnut --background" with the measured ratios `--doc-1` 8.47, `--doc-2` 8.49, `--doc-3` 8.11,
    `--doc-4` 7.77, `--doc-5` 7.58, `--doc-6` 7.54, `--doc-7` 7.58, `--doc-8` 8.51.
  - The `.dark` `--tone-*` values stay, except `--tone-warning`, which moves from
    `oklch(0.77 0.15 58)` to amber `oklch(0.77 0.15 80)` (user decision: at hue 58 it read as the
    orange accent at hue 55; at 80 it is 25 degrees away, 8.2:1 on the Walnut card and 8.0:1 on
    Graphite's). The `:root` (Ledger) warning is unchanged. Every dark tone measures at least 3.4:1
    on the Walnut and the Graphite card (`--tone-completed` 8.8:1 and 8.6:1), and the Ledger tones
    at least 3.4:1 on the Ledger card. The frozen palette contract pins the dark warning hues at 70
    or more.
- `web/src/aurora-ui/shared/atoms/theme.ts`: `export type DarkVariant = "walnut" | "gray";`,
  `ALL_DARK_VARIANTS = ["walnut", "gray"]`, `darkVariantAtom` default `"walnut"`. `LightVariant`
  unchanged. `ThemeProvider.tsx` needs no change.
- `web/src/components/theme-picker.tsx`: `ThemeId = "ledger" | "walnut" | "graphite"`;
  `THEME_OPTIONS` in that order: Ledger (caption `light · stone paper`), Walnut (`dark · warm
  brown-black`, `darkVariant: "walnut"`), Graphite (`dark · neutral charcoal`, `darkVariant:
  "gray"`). The preview cards need no new selector: `.theme-preview.dark` takes `.dark`, and
  `.theme-preview.dark[data-dark-theme="gray"]` takes Graphite.
- `web/src/components/theme-picker.test.tsx`: three labels; `APPLIED_THEMES` Walnut (`walnut`)
  and Graphite (`gray`); the two Pacific cases become Walnut cases (clicking Walnut from light
  stores `"walnut"`); "renders four buttons" becomes three; the 28 palette values become 21; "does
  not mark an unoffered dark variant as active" seeds the store through
  `localStorage.setItem("loom:dark-variant", JSON.stringify("retired-variant"))` before rendering
  instead of `store.set(darkVariantAtom, "green")` (which no longer typechecks). Never write the
  words aubergine or pacific in any file: an after_stage check greps `web/src` for them.
- `web/src/components/theme-picker-palette.test.tsx`: drop the `tokens-dark-variants.css?raw`
  import; `OWNING_SELECTOR` gets `walnut: ".dark"`; the "moves the preview and the applied theme
  together" case edits Walnut's rule; `applyToRoot` falls back to `"walnut"`.
- `web/src/plan/ui/pacific-accent.test.tsx`: delete (Pacific is gone).

## 2. Spine glyphs light up (R7)

**Diagnosis, settled (do not re-investigate).** `Glyph` (`web/src/plan/feature/spine-row.tsx:23-32`)
renders `span.plan-spine__glyph` as the direct child of `span.plan-spine__badge`, React writes the
boolean as `data-tested="true"`, and the built bundle carries
`.plan-spine__badge[data-tested=true]>.plan-spine__glyph{color:var(--tone-completed)}`. The
selector matches. The user saw no lit flask because no plan of this repository's `feat-planner`
feature has a `## Pressure tests` record (`loom/src/planning/pressure.rs` `records`), so every row
draws the untested gauge, muted by design. Keep the flask rule. The missing piece is the done
glyph, which today has no tone at all.

- `spine-row.tsx`: `Glyph` takes an optional `state?: string` and writes `data-state={state}` on
  the `span.plan-spine__glyph`. `Note` (lines 50-56) passes `state={note}` when it draws a glyph
  (`icon` non-null, which only plan rows have, so the value is one of the `PlanState` names).
  `PressureBadge` passes none.
- `web/src/plan/feature/spine.css`, after the base rule at line 234, add
  `[data-loom-plan] .plan-spine__glyph[data-state="done"] { color: var(--tone-completed); }`
  (exactly this selector: an after_stage check greps it). Running (the roundel), written, retired
  and the untested gauge stay `--muted-foreground`. Rewrite the comment at lines 229-233: the done
  state glyph and the tested flask take `--tone-completed`, everything else stays muted.
- The frozen contract "a done plan glyph and a tested plan glyph take the completed tone" checks
  both through the real spine and `element.matches` against the rules of `spine.css`.

## 3. Fold chevrons (R11, item 7)

`web/src/plan/concept/concept-runs.css` lines 53-84 (`.concept-run-cap`, `.concept-run-cap-icon`,
hover, focus) and the header comment at line 12; `fold-handle.tsx` keeps its markup, class names,
`aria-label="Fold"`, `aria-describedby` and tooltip (`help/legend.test.tsx:148` and
`help/reading-scene.test.tsx:81` select `.concept-run-cap`).

- No background, no border, no radius box at rest or on hover. The chevron is the control:
  icon 1.1em, `color: var(--run-colour)`, `opacity: 0.7`.
- Hover and focus-visible: opacity 1 and the icon moves 2px the way it points
  (`[data-side="start"]` left, `[data-side="end"]` right) by `transform: translateX(...)` on the
  icon, 120ms; none under `prefers-reduced-motion` (the existing block at line ~201).
- Hit area at least 24x24px through padding, without moving the surrounding text more than the old
  1.5em square did (keep `vertical-align` so the line box does not grow).
- `:focus-visible`: `outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 999px`
  (a circle). Keep the shared `.concept-plan-glyph:focus-visible` selector working.
- The inline run's hairline recolour on cap hover (lines 36-44) stays.
- No other boxed chevron exists in `web/src/plan/concept/*.css` (`.concept-sentence-chevron` in
  `concept.css` is bare already); do not edit `concept.css` (another stage owns it).
- Add to `fold-handle.test.tsx` only if you change behaviour (you should not).

## 4. Add to chat (R11, item 8)

`web/src/plan/attachments/add-to-chat.tsx` (`AddToChat` line 24, `FloatingControl` lines 67-83);
styles move to a new `web/src/plan/attachments/add-to-chat.css`, imported by `add-to-chat.tsx` as
`import "./add-to-chat.css";` (a wiring check greps that line). No Tailwind class stays in the
file (an after_stage check greps `fixed z-50`).

Structure (keep the names tests read):

- The positioned element stays the `div role="group" aria-label="Selection actions"` that takes
  `style={placement}` and `onMouseDown` (`add-to-chat.placement.test.tsx:59` reads its style), class
  `add-to-chat`.
- Inside it, a pill `div.add-to-chat__pill`: `--popover` surface, 1px `--border`, `border-radius:
  999px`, 4px padding, a two-layer shadow from tokens (`var(--plan-shadow), var(--popover-lift)`,
  as the activity popover's `POPOVER_SHADOW`). It holds the primary, then (only when secondaries
  exist) a 1px vertical `--border` rule (`aria-hidden`), then the secondaries.
- The primary: a `<button type="button" className="add-to-chat__primary">` with
  `<MessageSquarePlus aria-hidden="true" />` (lucide) and the words `Add to chat`; `--primary`
  fill, `--primary-foreground` text, full radius, 28px high, `--type-ui` at 500. Its accessible
  name stays exactly "Add to chat". Hover darkens through `color-mix` with its own foreground, as
  DESIGN.md "Focus and hover" says; focus-visible draws the 2px `--ring` outline.
- Secondaries are ghost text buttons: in `web/src/plan/review/send-to-author.tsx` `SendButton`
  (line 222) pass `variant="ghost"` to the `Button`. Change nothing else there.
- Over 16 KiB: no primary; the note `<p role="note" className="add-to-chat__note">Selection is
  over 16 KiB; select less to add it to chat.</p>` sits under the pill (or alone when the pill has
  nothing in it), `--type-fine`, `--destructive` text, inside the group. Keep the text and the
  `role="note"`.
- Entrance: `@keyframes` rise and fade from 4px below over 120ms on `.add-to-chat`; `animation:
  none` under `prefers-reduced-motion: reduce`.
- `canAuthor` logic, `revealChat`, `onAdded`, the null return and `MAX_ATTACHMENT_BYTES` are
  unchanged.

Tests (`add-to-chat.test.tsx`): add one case that the oversize note is not inside
`.add-to-chat__pill`, and keep every existing assertion. The frozen
`web/src/plan/files/files.contracts.test.tsx` mounts `AddToChat` with a registered secondary
(`Send to author: ...`) and no author rights: the pill must then show the secondary alone.

## 5. DESIGN.md (all of this stage's rules, V2's included)

Rewrite to the new truth (no history words):

- Line 3: the three themes (Ledger, Walnut, Graphite).
- **Colour**, replacing lines 9-10 (the Graphite periwinkle and the Pacific paragraph):
  - The themes: Ledger (light, stone paper, amber `--primary` `oklch(0.84 0.155 80)`), Walnut
    (dark default, warm brown-black at hue 55), Graphite (dark, neutral at hue 270). Dark surfaces
    keep chroma at or under 0.015, so the only saturated colours are the accent and the status
    tones. State the measured ratios from section 1.
  - The dark accent is orange `oklch(0.74 0.15 55)` for `--primary` and `--ring`, with
    `--primary-foreground` the theme's background. The accent marks the primary button, the focus
    ring, the current spine row, the selection and the chosen option of a segmented choice, and
    nothing else.
  - `--accent-ink` (index.css: `--ring` on Ledger, `--primary` in the dark themes) is the token for
    accent-coloured text and lines, so they meet 3:1 on Ledger.
  - In the dark themes the warning tones (`--tone-warning`, `--hazard-warning`) are amber at hue 80,
    kept clear of the orange accent at hue 55; Ledger's warning stays orange beside its amber
    accent. As every status, a warning is a glyph and a word, never a hue alone.
  - Add `--accent-ink` to the token list on line 7.
- **Status** (line 75-79): the state glyph of a done plan in the spine takes `--tone-completed`,
  as the tested flask does; written, running, retired and the untested gauge stay
  `--muted-foreground`.
- **Surfaces and borders**, add these exceptions (one bullet each, in the file's
  `exception: <surface>: <rule>: <why>` voice):
  - the fold cap (`concept-runs.css`) is a bare chevron in the document hue with a circular focus
    ring: the square read as a button stuck in the text;
  - the selection actions (`attachments/add-to-chat.css`) float as a full-radius pill with a
    `--primary` pill button, though it is not the screen's one primary: it floats over the text the
    user selected and is the action the selection exists for; its oversize note is `--destructive`
    text;
  - Settings' reading-size tiles (`settings/`): three tiles, the chosen one edged and drawn in
    `--accent-ink` (a segmented choice's chosen option);
  - Settings' reading preview: a sunken `--background` specimen inside the `--card` dialog, 1px
    `--border`, 8px corners, a 3px `--accent-ink` rule on its start edge: it is a sample of the
    reading column, so it is set apart from the controls it reflects;
  - Settings' summary statistics: two figure cards (a box inside the dialog), totals at 1.75rem in
    the mono face with tabular numerals, a stacked bar per card in a monochrome ramp of
    `--foreground` mixed into `--muted`, with a `--foreground` tick for the 7-day daily average, a
    status dot in `--tone-completed` or `--tone-blocked`, and a 400ms count-up of the totals that
    `prefers-reduced-motion` skips: the user asked for an instrument panel.
- Remove the `.plan-spine__glyph` exception text that says only the flask takes a tone, if any.

## Shots

You cannot build or shoot. The main agent shoots after your pass at 1440x900 and 390x844 in
Ledger, Walnut and Graphite and judges these fixed targets:

1. Dark themes: page ground, cards and sidebars read as near-neutral brown-black (Walnut) and
   neutral charcoal (Graphite); no purple or navy anywhere; the only saturated colour on a page at
   rest is the orange on the primary button, the current spine mark and dot, and status glyphs.
2. Ledger looks as before (amber accent).
3. Settings theme cards: three cards, Ledger, Walnut, Graphite, each previewing its own palette.
4. Spine Plans group (fixture feature with a done tested plan, a done untested plan and a written
   plan): the done check and the tested flask are green; the written pencil and the untested gauge
   stay muted; nothing wraps at 390px.
5. An opened run: the fold caps are bare chevrons in the run's document hue, no square at rest or
   on hover; a keyboard focus draws a round ring.
6. A text selection on a Source page: a floating pill with an orange (Ledger: amber) "Add to chat"
   pill button with its icon, rising in.

Your report lists every surface you changed and the state needed to see it.

## Files

Your row in the stage's worker table. Never edit V2's or M1's files, `concept.css`,
`summaries/contracts.test.tsx` or any `*.contracts.test.tsx`.

## Run once at the end

From `web/`: `node_modules/.bin/vitest run src/components src/plan/feature src/plan/concept/fold-handle.test.tsx src/plan/concept/doc-tokens.test.ts src/plan/attachments src/plan/files/files.contracts.test.tsx src/plan/ui src/plan/help/legend.test.tsx src/plan/help/reading-scene.test.tsx src/plan/editor/theme.test.ts`

Report: files changed, the test run's result, every test you changed and why, anything unsettled.
