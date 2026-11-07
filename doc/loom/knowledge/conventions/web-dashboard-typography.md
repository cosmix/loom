# Web Dashboard Typography
>

>

## Where the Dashboard's Theme Tokens Live

`web/src/index.css` defines the `--tone-*` state tones and `--hairline`, and also the accent helpers `--highlight` / `--highlight-foreground` (active view tab), `--logo` (header logo colour: the foreground ink in light, plain white `oklch(1 0 0)` in dark), the ledger ruling `--ledger-rule` / `--ledger-head-rule`, and `--dialog-shadow`, the drop shadow every dialog takes through `shadow-(--dialog-shadow)` on `DialogContent` (`web/src/components/ui/dialog.tsx`). The dark `--dialog-shadow` is much denser than the light one because black at the light opacity barely darkens the near-black ground; both were tuned by measuring screenshot luminance around an open dialog. The ruling rules at the end of `index.css` are unlayered on purpose: the table's `border-b` utilities sit in Tailwind's utilities layer, which outranks the components layer.

Surface tokens (`--background`, `--card`, `--foreground`, `--muted`, `--muted-foreground`, `--border`, `--ring`, `--primary`) live in `web/src/aurora-ui/shared/styles/tokens.css`, with the planner's overrides in `web/src/index.css`. The bare `:root` block is the Ledger light theme (stone stock, hue ~100, blue-black ink) with an amber accent, `--primary` `oklch(0.84 0.155 80)`; the first `.dark` block is Walnut, the dark default (hue 55, ground `oklch(0.19 0.014 55)`) with an orange `--primary` and `--ring` `oklch(0.74 0.15 55)`; `.dark[data-dark-theme="gray"]` is Graphite (id `gray`, hue 270) with the same orange accent. On the light ground amber is only legible as a fill under dark ink, so the light `--ring` is a darker ochre. In the dark themes `--tone-warning` and `--hazard-warning` are amber at hue 80, 25 degrees off the orange accent; Ledger's warning stays orange (hue 50). The atoms in `web/src/aurora-ui/shared/atoms/theme.ts` default to the `ledger` / `walnut` variants. The full theme record, its contrast floor and the accent tokens `--accent-ink` and `--accent-text` are in [Planner Web Shell](../architecture/planner-web-shell.md#themes). `ThemePicker` in `web/src/components/theme-picker.tsx` sets scheme and variant from the Settings dialog; `ThemeProvider` applies the matching theme attributes. The header also offers a light/dark toggle.

The terminal well is `--well: #0a111f` in `web/src/components/terminal/terminal.css` and must equal `WELL` in `emulator.ts`, since xterm's canvas needs hex; the xterm cursor and selection are the dark `--primary` as hex (`#fcbb20` and `#fcbb2040`), pinned by `emulator.test.ts`. The terminal view has no Esc-to-release: in control mode Esc reaches the agent, and only the release control stops sending (`web/src/components/terminal/terminal-view.tsx`).

## Dashboard chrome uses the body face, never all-caps monospace

The web dashboard's controls, cues and status words are set in the body face (Alegreya Sans, see [the UI sans face](#the-ui-sans-face-is-alegreya-sans-metric-matched-to-inter)) at 12-13px, weight 500, sentence case. The `font-display` class (headings such as the title in `EmptyState.tsx` and `HazardPanel.tsx`) resolves to the same face: the kit's Fraunces value of `--font-display` is overridden to `var(--font-sans)` by the `@theme inline` block in `web/src/index.css`, so headings and chrome in both apps render in the sans face. Document prose in the planner is the exception: see [the planner reading face](#planner-reading-face). Monospace is reserved for values that are literally code or identifiers: stage ids, branch names, screen sizes, and the terminal well itself. Uppercase with wide tracking is reserved for the three existing label utilities (`eyebrow`, `.stage-tag`, `.rank-caption`); do not coin new all-caps monospaced chips for buttons or indicators (`web/src/components/terminal/terminal-controls.css`, `terminal.css` follow this).

## A Read-Only Slot's Text Needs an Explicit Line-Height, Not `inline-flex`, to Centre

`.settings-ctl-readonly` is a `<span>` blockified as a flex item of `.settings-slot` (28px tall); with `line-height: normal` its text hugs the top instead of centring. Fix with an explicit `line-height: 26px` (the 28px border-box height minus two 1px borders) on the span itself, not `inline-flex`, which centres the text but disables `text-overflow: ellipsis` on the phone-card layout. `overflow: hidden` + `text-overflow: ellipsis` is scoped to `.settings-tc .settings-ctl` (the phone-card selector) rather than the shared `.settings-ctl` rule, so the table layout's cells never truncate.

## Help dialog content

The Web UI help dialog must preserve the complete stage-state legend as a dedicated topic, alongside the broader operating guide. The operator reviews the rendered desktop and mobile dialog before commits.

## Help uses operational illustrations

Each help topic is a set of scenes: a real-component illustration with numbered pins on the controls it explains, and one short sentence per pin below it (`HelpScene` in `web/src/components/help-topics.ts`, rendered by `SceneFigure` in `help-scene.tsx`). Pins anchor to CSS selectors inside the illustration and are measured at runtime; hovering a note rings its target. Facts that have no control to point at go in a short "Good to know" tip list, never in prose sections: the operator rejected paragraphs of text set apart from the graphics they describe. Illustrations carry no static highlights that imply state (no permanently active step), and the graph example shows no dimmed nodes. Type in help is at least 13px. Avoid decorative taglines. Preserve the complete Legend topic. Preview the rendered changes before committing.

## Help illustrations use real components

Help examples use production stage nodes, controls, and resource indicators with isolated sample state. `help-example.tsx` supplies a separate Jotai store. The graph is a non-animated, inert illustration; the terminal switch changes only example state and drives the production well background and striped perimeter. [Implementation pitfall](../mistakes/web-dashboard-server.md#help-illustrations-must-match-production-ui).

A planner Help scene renders production components too, never a hand-copied lookalike: a component that cannot render without a fetch or an unexported symbol is left out and reported. `PlanHeader` takes `sample` to skip `HeaderExtras` (which fetches `/api/run/state` and `/api/summaries/panel`), and scenes carry no router of their own (`scene-router.tsx`, `help-graph-demo.tsx` reads `useInRouterContext`).

## Terminal help control fidelity

The terminal help example uses the production ModeKey from components/terminal/mode-key.tsx, including its sliding take-control/release behavior. A generic replacement button is not an acceptable illustration.

## Resource help uses production indicators

Token and quota illustrations use the production ContextMeter, token-flow components and QuotaMeters with isolated sample state. Recreated bars, charts, and generic control graphics fail the required visual fidelity. This applies across all help illustrations.

## Help graph is static and terminal control is complete

The help dependency graph is a static illustration: no animation, panning, zooming, or navigation. Terminal help may offer the production mode switch only when it also renders the production control-mode well background and striped perimeter. Commit the help source and task memory separately from the rebuilt embedded bundle.

## Stage thread chips use horizontal space and no accent spines

The stage dialog and stage page show "depends on" and "feeds" as one wrapping row of chips under the heading (`ThreadChips` in `web/src/components/stage-heading.tsx`; styles `.thread-chips`, `.thread-chip`, `.thread-end` in `web/src/components/stage-report.css`), so the NOW card stays above the fold. Each side has an eyebrow and a chip per stage (arrow, name, mono id, toned state label, chevron); an empty side is an icon sentence. Long names and ids wrap and nothing truncates. State is carried by the toned label, never a coloured left border or inset spine: the operator rejected a vertical rail with left-spine chips as wasting width. The dialog body publishes its scrollbar width as `--gutter` (`use-scroll-gutter.ts`) so scrolled content keeps the header's right edge.

## Planner reading face

Body prose of rendered documents is set in a serif reading face; chrome, titles and Markdown headings stay in the sans face (`--font-sans`, Alegreya Sans), and code, paths and ids in `--font-mono` (Commit Mono Cosmix).

- `web/src/plan/plan.css` defines `--font-reading`, `--reading-size` (19px default) and `--reading-line` (1.6) on `[data-loom-plan]`; Settings overrides them through `data-reading-face` (`source-serif` default, `literata`, `piazzolla`) and `data-reading-size` (17, 19, 21) on the shell root. The choices live in `web/src/plan/settings/preferences.ts` (`READING_FACES`, `READING_SIZES`), stored per project through `projectPref` (`plan/chat/prefs.ts`); the fonts come from `@fontsource-variable/*` imports in `web/src/plan/main.tsx`.
- `web/src/plan/markdown/markdown.css` applies the face to `[data-doc-container] .md`, `.concept-passage .md`, `.feature-pitch` and `.chat-passage`, never to bare `.md`: chat Markdown renders `.md` inside `font-sans text-sm` and stays in the sans face, so a new document surface opts in by one of those selectors. `.md h1`-`h6` keep `--font-sans` at weight 600, so reading text is serif while its headings are sans.
- The help dialog sets the same two attributes on its content (`help/help-dialog.tsx`), so help prose follows the reader's choice.
- `loom/src/plan_server/router.rs` serves `/fonts/` from the embedded bundle beside `/assets/`; a font path outside the bundle answers 404, never the app page.

## The UI sans face is Alegreya Sans, metric-matched to Inter

`--font-sans` is `"Alegreya Sans", "Loom Sans Fallback", ui-sans-serif, system-ui, ...`, set in the `@theme inline` block of `web/src/index.css` (the aurora-ui copy in `src/aurora-ui/shared/styles/tokens.css` still names Inter and is overridden, not edited). The faces are hand-declared in `web/src/fonts.css` from `@fontsource/alegreya-sans` (static family: 400, 500, and the 700 file declared as `font-weight: 600 700` because the family has no 600), one `@font-face` per subset (latin, latin-ext, greek, greek-ext, cyrillic, cyrillic-ext, vietnamese) with the package's `unicode-range`, so a page fetches only the scripts it shows.

Alegreya Sans is a small face: x-height 0.458 em against Inter's 0.546 em. Every face carries `size-adjust: 119%` so that the type scale (`--type-*` in `web/src/plan/ui/design.css`, Tailwind text classes) renders at the same visual size and nearly the same line length (adjusted average width 0.615 em against Inter's 0.610 em) without any token changing. Never scale `html` or the `--type-*` tokens to compensate for a face; change the `size-adjust` descriptor. `"Loom Sans Fallback"` is a `local()` face (Segoe UI, Roboto, Helvetica Neue, Arial) with `size-adjust: 105%` so a system font used for a script Alegreya lacks stays level; em-box scripts (CJK, Arabic) fall through to `system-ui` uncompensated on purpose. There is no Inter `font-feature-settings` on `body` any more.

Trap: `themes.contracts` and `doc-tokens` tests parse `index.css` with a `/([^{}]+)\{/` regex, so a braced block must sit between the `@import` lines and `:root` or the imports get merged into the `:root` selector text; the `@theme inline` block serves that purpose and must stay above `:root`.
