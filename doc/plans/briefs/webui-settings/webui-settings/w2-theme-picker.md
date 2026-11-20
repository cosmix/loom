# W2 — theme picker and its stage-graph swatches

Lane: codex `gpt-5.6-sol`, effort `xhigh`. Two units, run in order, one forward each. Your task text
names ONE unit: do only the section headed with that unit id and stop at its own "Done when" line.

You own these files and no others:

```text
web/src/components/theme-picker.tsx      (new)
web/src/components/theme-picker.test.tsx (new)
```

Read-only, for context: `web/src/aurora-ui/shared/atoms/theme.ts`,
`web/src/aurora-ui/theme/ThemeProvider.tsx`, `web/src/aurora-ui/theme/ThemeToggle.tsx`,
`web/src/aurora-ui/shared/styles/tokens.css`, `web/src/components/view-switch.tsx` (the button idiom
this dashboard uses), `web/src/test/settings-kit.tsx`.

Do not run `git` at all. Do not create a CSS file, do not edit `web/src/index.css`, and do not touch
anything under `web/src/aurora-ui/` or `web/dist`.

## What already works, and what is missing

The whole theme machine is already wired end to end except for a control that sets it.

- `web/src/aurora-ui/shared/atoms/theme.ts` exports `themeAtom` (`"light" | "dark"`),
  `lightVariantAtom` (`"ledger" | "gray" | "cool"`) and `darkVariantAtom`
  (`"aubergine" | "green" | "gray" | "blue" | "slate" | "sand"`), all three persisted with
  `atomWithStorage(..., { getOnInit: true })` under `loom:theme`, `loom:light-variant` and
  `loom:dark-variant`.
- `ThemeProvider` (`web/src/aurora-ui/theme/ThemeProvider.tsx`) writes them to `<html>` in one
  effect: the `dark` class, `dataset.lightTheme`, `dataset.darkTheme`.
- `tokens.css` already carries every matching selector. The two the user asked for are already
  vendored, byte-identical to the aurora-ui kit: `.dark[data-dark-theme="gray"]` at lines 321-362 and
  `.dark[data-dark-theme="blue"]` at 365-406. The base blocks are `:root` at 93-140 (Ledger) and
  `.dark` at 143-186 (Aubergine).
- `ThemeToggle` only ever flips `themeAtom`. Nothing in the dashboard writes either variant atom, so
  today the blue and gray blocks are unreachable.

Your job is the missing control. No CSS and no atom changes are needed.

## Unit w2a-theme-picker — `web/src/components/theme-picker.tsx`

### The public surface

Exact; do not rename, do not add exports:

```tsx
import { useAtom } from "jotai";
import type { ReactElement } from "react";
import {
  darkVariantAtom,
  lightVariantAtom,
  themeAtom,
  type ColorScheme,
  type DarkVariant,
  type LightVariant,
} from "@/aurora-ui/shared/atoms/theme";

export interface ThemePalette {
  background: string;
  card: string;
  foreground: string;
  mutedForeground: string;
  border: string;
  primary: string;
  primaryForeground: string;
}

export type ThemeId = "ledger" | "aubergine" | "pacific" | "graphite";

export interface ThemeOption {
  id: ThemeId;
  label: string;
  caption: string;
  scheme: ColorScheme;
  lightVariant: LightVariant | null;
  darkVariant: DarkVariant | null;
  palette: ThemePalette;
}

export const THEME_OPTIONS: readonly ThemeOption[];

export function ThemePicker(): ReactElement;
```

### The four options

Four, in this order. `tokens.css` defines nine combinations; the picker deliberately offers the two
loom has always shipped plus the two the user asked for, because loom's own `--tone-*`,
`--ledger-rule` and `--logo` tokens (`web/src/index.css:35-76`) have only ever been tuned against
Ledger and Aubergine, and the other five have had no visual pass. Do not add the missing five.

| id | label | caption | scheme | lightVariant | darkVariant |
| --- | --- | --- | --- | --- | --- |
| `ledger` | `Ledger` | `light · hue 102` | `light` | `"ledger"` | `null` |
| `aubergine` | `Aubergine` | `dark · hue 325` | `dark` | `null` | `"aubergine"` |
| `pacific` | `Pacific` | `dark blue · hue 258` | `dark` | `null` | `"blue"` |
| `graphite` | `Graphite` | `dark gray · hue 270` | `dark` | `null` | `"gray"` |

The palettes, copied from the token blocks named above. These are literal strings in your source, not
`var(--…)` lookups — see "why literals" below.

| id | background | card | foreground | mutedForeground | border | primary | primaryForeground |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `ledger` | `oklch(0.925 0.009 102)` | `oklch(0.955 0.008 102)` | `oklch(0.21 0.012 262)` | `oklch(0.48 0.02 250)` | `oklch(0.84 0.012 100)` | `oklch(0.84 0.155 80)` | `oklch(0.21 0.012 262)` |
| `aubergine` | `oklch(0.16 0.035 325)` | `oklch(0.195 0.04 325)` | `oklch(0.94 0.012 325)` | `oklch(0.73 0.03 325)` | `oklch(0.31 0.045 325)` | `oklch(0.83 0.165 82)` | `oklch(0.2 0.04 325)` |
| `pacific` | `oklch(0.18 0.045 258)` | `oklch(0.225 0.045 258)` | `oklch(0.95 0.018 258)` | `oklch(0.76 0.04 258)` | `oklch(0.42 0.06 258)` | `oklch(0.78 0.135 258)` | `oklch(0.17 0.045 258)` |
| `graphite` | `oklch(0.18 0.005 270)` | `oklch(0.225 0.005 270)` | `oklch(0.95 0.005 270)` | `oklch(0.74 0.008 270)` | `oklch(0.42 0.006 270)` | `oklch(0.78 0.015 270)` | `oklch(0.18 0.005 270)` |

Why literals: a swatch shows a theme the viewer is NOT currently in. `var(--background)` resolves
against the active theme, so four swatches built from CSS variables would render as four identical
copies of the current one. Every colour inside the SVG therefore comes from `option.palette`.

HARD CONSTRAINT: the string `var(--` must not appear ANYWHERE in `theme-picker.tsx` — not in the
swatch, not in a `className` arbitrary value, not in a comment. A plan gate greps the file for it and
fails the stage on a match, because a variable-built swatch passes every test while showing four
identical pictures. Style the picker chrome with the named token utilities listed under "The picker"
below; they need no `var(`.

All 28 literals were checked character by character against `tokens.css` (`:root` 93-140, `.dark`
143-186, `.dark[data-dark-theme="gray"]` 321-362, `.dark[data-dark-theme="blue"]` 365-406), and
`web/src/index.css` redefines none of these seven tokens, so they are the effective values. Copy them
from the table; do not re-derive them.

### The swatch

This art is approved as drawn — reproduce the geometry exactly rather than designing your own. It
replaces aurora-ui's generic sidebar-and-card thumbnail with loom's own subject: a header strip over
five stage cards on two ranks, joined by orthogonal edges with arrowheads, the middle stage carrying
the accent the way an executing stage does.

A module-local component, not exported:

```tsx
function ThemeSwatch({ palette }: { palette: ThemePalette }): ReactElement
```

It renders `<svg viewBox="0 0 120 76" width="120" height="76" aria-hidden="true">` with
`style={{ display: "block", borderRadius: "5px" }}`, containing exactly these children in this order.
`P.x` means the matching field of `palette`; put every colour in an inline `style` object (SVG
presentation attributes are not what the `oklch()` path is guaranteed through).

```text
rect  0,0   120x76                     fill P.background
rect  0,0   120x13                     fill P.card
rect  7,5   16x3   rx 1.5              fill P.foreground        opacity 0.8
rect  95,5  8x3    rx 1.5              fill P.mutedForeground   opacity 0.55
rect  106,5 8x3    rx 1.5              fill P.mutedForeground   opacity 0.55

group: fill none, stroke P.mutedForeground, strokeWidth 1.1, opacity 0.7
  path d="M36 27 H43 Q46 27 46 30 V39 Q46 42 49 42"
  path d="M36 57 H43 Q46 57 46 54 V45 Q46 42 49 42"
  path d="M76 42 H81 Q84 42 84 39 V30 Q84 27 87 27"
  path d="M76 42 H81 Q84 42 84 45 V54 Q84 57 87 57"

group: fill P.mutedForeground, opacity 0.7
  path d="M50 42 L46 40.2 L46 43.8 Z"
  path d="M88 27 L84 25.2 L84 28.8 Z"
  path d="M88 57 L84 55.2 L84 58.8 Z"

rect 10,21 26x12 rx 3   fill P.card     stroke P.border
rect 14,25.5 14x3 rx 1.5 fill P.foreground opacity 0.55
rect 10,51 26x12 rx 3   fill P.card     stroke P.border
rect 14,55.5 14x3 rx 1.5 fill P.foreground opacity 0.55
rect 50,36 26x12 rx 3   fill P.primary
rect 54,40.5 14x3 rx 1.5 fill P.primaryForeground opacity 0.8
rect 88,21 22x12 rx 3   fill P.card     stroke P.border
rect 92,25.5 12x3 rx 1.5 fill P.foreground opacity 0.4
rect 88,51 22x12 rx 3   fill P.card     stroke P.border
rect 92,55.5 12x3 rx 1.5 fill P.foreground opacity 0.4
```

### The picker

`ThemePicker` reads all three atoms with `useAtom`, derives the active id, and renders a
`<div role="group" aria-label="theme">` laying the four options out in a row with Tailwind utilities
(`flex flex-wrap gap-3`). Each option is:

```tsx
<button
  type="button"
  aria-pressed={active}
  aria-label={`${option.label} theme`}
  onClick={…}
  className={…}
>
  <ThemeSwatch palette={option.palette} />
  <span>{option.label}</span>
  <span>{option.caption}</span>
</button>
```

HARD CONSTRAINT: `<button aria-pressed>`, never `role="radio"` and never `role="radiogroup"`. This
picker renders inside the settings dialog, and `web/src/components/settings-dialog.test.tsx:30`
asserts `screen.queryAllByRole("radio")` is empty across the whole dialog. A radio group there turns
an unrelated, deliberate test red.

Active derivation: the option is active when `option.scheme === scheme` and, for a light option,
`option.lightVariant === lightVariant`, or for a dark option `option.darkVariant === darkVariant`.

Click: set `themeAtom` to `option.scheme`; if `option.lightVariant` is non-null set `lightVariantAtom`
to it; if `option.darkVariant` is non-null set `darkVariantAtom` to it. Setting the variant even when
the scheme does not change is what makes the existing sun/moon `ThemeToggle` come back to the dark
theme the user last picked.

Styling: Tailwind utilities only, using the dashboard's own token classes (`border`, `bg-card`,
`text-muted-foreground`, `rounded-lg`, `ring-primary`), so the picker itself follows the ACTIVE theme
while each swatch shows its own. The active button takes a visible accent border plus a ring; the
others a plain `border-border`. Labels are Inter at 12-13px in sentence case, captions
`text-muted-foreground` — never all-caps monospace, which this dashboard reserves for identifiers
(`doc/loom/knowledge/conventions/web-dashboard-typography.md`). Keep the file under 400 lines.

## Unit w2b-picker-test — `web/src/components/theme-picker.test.tsx`

Render inside a jotai `Provider` with a fresh `createStore()`, the way `web/src/test/settings-kit.tsx:174-184`
does. `ThemePicker` needs no router. `afterEach(() => cleanup())`, as every other test file here does.
Clear `localStorage` between cases so a persisted atom does not leak across them.

Cases:

1. four buttons render, one per option, found by their accessible names
2. clicking `Pacific theme` leaves `store.get(themeAtom) === "dark"` and
   `store.get(darkVariantAtom) === "blue"`
3. clicking `Ledger theme` leaves `store.get(themeAtom) === "light"` and
   `store.get(lightVariantAtom) === "ledger"`
4. exactly one button has `aria-pressed="true"` after a click, and it is the one clicked
5. `screen.queryAllByRole("radio")` is empty — the guard for the dialog assertion above
6. every one of the four buttons contains an `<svg>` whose `viewBox` is exactly `0 0 120 76`, so a
   button that lost its swatch fails rather than passing as a bare label
7. over `THEME_OPTIONS`: all 28 palette values start with `oklch(`, and the four `palette.background`
   values are pairwise distinct. Assert on the exported data, not on computed SVG styles — jsdom's
   style parser is not guaranteed to keep an `oklch()` value it does not understand.
8. with `darkVariantAtom` set to a variant the picker does not offer (`store.set(darkVariantAtom, "green")`
   and `themeAtom` `"dark"`), NO button has `aria-pressed="true"` — five of the nine token
   combinations have no option, and the picker must not claim one of its four is active
9. `every theme applies` (the name must contain those exact words — a plan gate greps for them): an
   `it.each` over a table written OUT IN THE TEST FILE, never derived from `THEME_OPTIONS` — a test
   that reads its expectations from the module under test cannot catch a wrong row:

   ```text
   label        scheme   dark class   dataset             localStorage after the click
   Ledger       light    absent       lightTheme ledger   loom:theme "light", loom:light-variant "ledger"
   Aubergine    dark     present      darkTheme aubergine loom:theme "dark",  loom:dark-variant "aubergine"
   Pacific      dark     present      darkTheme blue      loom:theme "dark",  loom:dark-variant "blue"
   Graphite     dark     present      darkTheme gray      loom:theme "dark",  loom:dark-variant "gray"
   ```

   Render `<ThemePicker />` inside the REAL `ThemeProvider` (`@/aurora-ui/theme/ThemeProvider`),
   itself inside the jotai `Provider`. For Ledger, first set the store to `"dark"` so the click is a
   real change. Click the row's button, then assert with `waitFor`:
   `document.documentElement.classList.contains("dark")`, the named `document.documentElement.dataset`
   field, and the two `localStorage` values — stored as JSON, so `"\"blue\""`, i.e. compare against
   `JSON.stringify("blue")`. Then unmount, render the picker again inside a FRESH `createStore()` WITHOUT
   clearing `localStorage`, and assert with `waitFor` that the same button reads
   `aria-pressed="true"` (jotai's `onMount` re-reads storage in a new store). Reset
   `document.documentElement.className` and its `data-light-theme` / `data-dark-theme` attributes in
   `afterEach`.
10. palettes are the four the brief names: an `it.each` over the same four labels asserting the
    option's `palette.background` and `palette.primary` equal these literals, again written out in the
    test and not read from the module:

    ```text
    Ledger     background oklch(0.925 0.009 102)   primary oklch(0.84 0.155 80)
    Aubergine  background oklch(0.16 0.035 325)    primary oklch(0.83 0.165 82)
    Pacific    background oklch(0.18 0.045 258)    primary oklch(0.78 0.135 258)
    Graphite   background oklch(0.18 0.005 270)    primary oklch(0.78 0.015 270)
    ```

11. every swatch draws the graph: inside each of the four buttons the `<svg>` holds exactly 15
    `rect` and 7 `path` elements (the header strip, five cards with their text bars, four edges and
    three arrowheads above). An empty SVG with the right `viewBox`, or a bare card, fails. Colour is
    asserted on the exported data (cases 7 and 10), never on computed SVG styles, for the jsdom reason
    given in case 7.

### Done when (w2b)

`bun run --cwd web test src/components/theme-picker.test.tsx` passes. Run it once if you want.

## Report

Unit w2a has no test of its own; it is done when the file matches this brief. The orchestrator runs
typecheck, lint, format and the full suite; you do not. Report files changed, anything you assumed,
anything unresolved.
