# Common brief: planner UI repair

Every stage of `PLAN-feat-planner-12-ui-repair.md` reads this file first. It carries the target
the earlier redesign missed, the decisions settled for this repair, and the rules every worker
follows.

## Why this plan exists

`PLAN-planner-ui-redesign` (archived) linked its mockups as a claude.ai artifact ("Loom Plan
Redesign", `https://claude.ai/artifact/2T7SurQrDQJhCX4yyKi9iu`). Its executors could not open
it, and no criterion rendered a page, so the shipped planner diverges from the mockup on nearly
every screen and has functional defects ([the mistake](../../../../loom/knowledge/mistakes/plan-server-delivery.md)).
Stage sessions cannot open claude.ai either: plugin MCP servers and connectors are not loaded
in a stage. **The briefs of this plan are the mockup**, transcribed into structure, CSS values
and copy. Build to them literally. Where a brief gives a value, use that value; where it is
silent, match the nearest stated pattern and record the choice with `loom memory decision`.

## Token map (mockup name to planner token)

The mockup was drawn in the planner's real theme tokens under shorter names:

| Mockup | Planner token |
| --- | --- |
| `--bg` | `--background` |
| `--card` | `--card` |
| `--fg` | `--foreground` |
| `--muted` | `--muted` |
| `--muted-fg` | `--muted-foreground` |
| `--border` | `--border` |
| `--hairline` | `--hairline` |
| `--primary` / `--primary-fg` | `--primary` / `--primary-foreground` |
| `--ring` (accent) | `--ring` |
| `--ok` | `--tone-completed` |
| `--warn` | `--tone-warning` |
| `--run` | `--tone-executing` |
| document hue (`.dm`, runs, marks) | `--doc-1` .. `--doc-8` |

Shadow: `--shadow` is `0 1px 2px oklch(0.2 0.02 100 / 0.06), 0 8px 24px oklch(0.2 0.02 100 / 0.08)`
in light themes and `0 1px 2px oklch(0 0 0 / 0.4), 0 10px 30px oklch(0 0 0 / 0.45)` in dark ones;
use the existing `--dialog-shadow` where one exists, otherwise add `--plan-shadow` to
`web/src/plan/plan.css` with those two values (light under `:root`, dark under `.dark`).

Recurring measures:

- Eyebrow: 10.5px, weight 600, letter-spacing 0.12em, uppercase, `--muted-foreground`.
- Icon button (`.ibtn`): 28px high, min-width 28px, padding `0 8px`, radius 7px, 12.5px/500,
  hover `--muted`; `.outline` adds a 1px `--border`.
- Pill (`.pill`): 28px high, padding `0 10px`, gap 7px, 1px `--hairline`, radius 999px,
  `--card` fill, 12.5px/500.
- Segmented control (`.seg`): padding 2px, 1px `--hairline`, radius 9px, `--card` fill; items
  26px high, padding `0 10px`, radius 7px, 12.5px/500, gap 6px, `--muted-foreground`; pressed
  item `color-mix(in oklch, var(--primary) 26%, transparent)` with `--foreground` text.
- Primary button (`.btn.primary`): 30px high, padding `0 12px`, radius 8px, 13px/600,
  `--primary` fill, `--primary-foreground` text; `.lg` is 38px, padding `0 16px`, 14px, radius 10px.
  Ghost (`.btn.ghost`): transparent, 1px `--border`, weight 500.
- Status chip (`.m`): 24px high, padding `0 8px`, radius 6px, 12px/500, `--muted` fill, icon in
  `--muted-foreground`; `go` = fill `--tone-executing` 18% and icon in that tone; `ok` = icon in
  `--tone-completed`; `warn` = fill `--tone-warning` 18% and icon in that tone.
- DocMark: 20px square, radius 5px, glyph 12px; large 30px, radius 8px, glyph 16px; fill the
  document colour at 16% (light) or 20% (dark), glyph in the full colour.
- Fonts: Inter for chrome and every title; `--font-reading` (Source Serif 4 by default) for body
  prose, pitches, transcripts and quoted passages; `--font-mono` (Commit Mono Cosmix) for code,
  paths, ids and counters in mono.

## Decisions settled for this repair

1. **No chat panel on the home screen.** The dock hides `ChatPanel` on `/` as it does on `/new`;
   `ChatRegistrations` stays mounted on every route.
2. **One app bar.** The header holds, left to right: Loom's logo (34px high), the identity block
   (eyebrow plus the feature switcher), then the right cluster: activity pill, view switch,
   presence avatars, Share, theme toggle, Settings (icon and label), Help (icon and label). On
   feature routes a second row holds the step rail and, beside it, the next-step card.
   Nothing else sits in the header: the `loom run` strip, the changed-since control and the
   summary status move into the activity popover (shell stage).
3. **Eyebrow by route.** `/f/<feature>/*`: "Planning · feature"; `/`: "Planning · project";
   `/new`: "Planning · new feature" over the title "Untitled"; `/files/*` and `/plans/*`:
   "Planning · files".
4. **Feature title.** Every place that names a feature shows its title, never its slug, except
   mono path lines and `aria-label`s that tests pin. The title comes from the server
   (`ChecklistView.title`, card-payload stage): the vision's first H1 without a trailing
   "— Product Vision", "Product Vision" or "Vision"; else the first metaplan's H1 without a
   trailing "Engineering Metaplan" or "Metaplan"; else the slug with `-` turned to spaces and
   each word capitalised. Backticks are stripped from the heading text.
5. **Step rail cells are counters and links.** Each cell shows the server's `count` (or "—"
   when null) over the server's `label`, and links to `/f/<feature>?step=<n>`.
6. **The feature screen is the step flow.** `/f/<feature>` renders a 220px step navigation and
   a step body; `?step=<n>` selects a step, default the current one. Product and Engineering
   views stay at `/f/<feature>/product` and `/engineering`.
7. **Document colours stay `--doc-1..8`.** No new hues: `concept/doc-tokens.test.ts` pins eight
   OKLCH tokens per theme with no hue between 50 and 90. Tints change (DocMark 16%/20%, run 10%,
   hovered run 22%), hues do not.
8. **Help type stays at 13px or more** (`conventions/web-dashboard-typography.md`, "Type in help
   is at least 13px"), where the mockup uses 11-11.5px for sub-lines.
9. **The pressure dialog states no minute figures** (`conventions/plan-web.md`); the time fact
   keeps its existing sentence.
10. **Add to chat stays on `a`;** `c` focuses the composer. The reading hint row says so.
11. **The pressure dialog offers 1, 2 or 3 rounds** as a segmented control; the free number input
    goes. Model and effort stay two selects per node, visually one `.pick2` row.
12. **Native `window.confirm` stays** where it guards an irreversible action (Stop the team,
    Stop the run, end the open chat). The chat stage fixes the case where it fires with no chat
    open.

## Rules for every worker

- **Visual work loads `frontend-design` first.** Every worker whose brief changes layout, styling,
  markup structure or copy on screen invokes the `frontend-design:frontend-design` skill
  (`Skill(skill="frontend-design:frontend-design")`) before writing code, and applies it within
  this plan's fixed targets. The brief's values and copy win over the skill's suggestions; the
  skill governs everything the brief leaves open (spacing rhythm, states, motion, polish). If the
  skill is not available in the session, record `loom memory note "frontend-design unavailable"`
  and continue.
- Read the ranges a brief names before editing them; line numbers are from `fcd5c7b1` and may
  have moved, so anchor by symbol (`loom map --outline <file>`).
- Every server or model string renders through `Untrusted`; storage keys through `storageKey`;
  no `dangerouslySetInnerHTML`; colours only from CSS variables; `prefers-reduced-motion`
  freezes every animation (`conventions/plan-web.md`).
- Tests that pin replaced copy or structure change in the same edit, in place. Do not move tests
  between files (that raises a test-integrity event). Files stay under 400 lines, functions under
  50.
- No `@codemirror` import outside the lazy loader; plan code never imports
  `components/graph/stage-graph` or the terminal (`web/scripts/bundle-rules.json`).
- Do not run git, `loom stage`, or the full test suite; run only the filtered command your brief
  names, once.
- Record surprises and decisions with `loom memory note` / `loom memory decision`; never Claude
  Code auto-memory.

## Visual check (orchestrators of web stages)

Jsdom tests do not show layout. Before the final review round, each web stage renders its
screens in a real browser with `scripts/plan-screens.sh` (written by the card-payload stage) and
reads every screenshot against its stage brief:

```bash
cd web && bun run build && cd ../loom && cargo build && cd ..
scripts/plan-screens.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens" <route> [<route> ...]
git checkout -- web/dist
```

The script shoots each route at 1440x900 and 390x844 in Ledger and Aubergine, and exits
non-zero on a console error or a failed navigation. Read each PNG with the Read tool, list every
deviation from the brief, fix it, and shoot again. Record `loom memory note "visual-check:
<route>: <matches | deviations fixed>"` per route, and pass the PNG paths to the final
`loom-code-reviewer`, whose brief includes this stage's brief so it can compare.
