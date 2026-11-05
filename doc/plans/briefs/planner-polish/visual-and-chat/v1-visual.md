# V1: footer, spine, roundel and Graphite accent (issues 1, 2 look, 9, 10 spine half, 11)

Tier: fable (`loom-senior-software-engineer` with the model override `fable`).

Before anything else:
1. Invoke `frontend-design:frontend-design` with the Skill tool.
2. Read `common.md` (Design rules, D10-D13), `web/src/plan/ui/DESIGN.md`, and the "Header and dock" section of `doc/loom/knowledge/architecture/planner-web-shell.md`.

You own the planner's shell look in this stage, and you are the only editor of `ui/DESIGN.md`. Other workers in this wave:
- V3 (fable) redesigns the chat sidebar.
- C1 removes the chat collapse.
- V2 restyles Settings.

In a second wave, B1 puts your `BusyText` into the text-only busy messages, and D1 updates help and the product spec.

## Fixed targets

1. **Footer (issue 1, D13).** `.plan-footer` in `ui/design.css` takes the dashboard footer's grid (`web/src/routes/shell.tsx:61-85`): `QuotaMeters` in a `minmax(0, 1fr)` track, the "limits unavailable" notes in an `auto` track.
   - At 1440px, Claude and Codex sit side by side with the dashboard's gaps.
   - At 390px, the dashboard's own narrow layout applies.
   - Change `shell-footer.tsx` markup only if the grid needs a wrapper. Keep `<footer className="plan-footer">` and the frozen footer contract.
2. **Plan badges (issue 2).** Style W1's `span.plan-spine__badge[data-tested]` in `feature/spine.css`: a quiet glyph and word, per DESIGN.md "Status".
   - On a narrow spine it never wraps under the plan name; the date drops first.
   - Tested and untested must differ by more than colour.
3. **Roundel (issue 9, D10).**
   - `ui/activity-glyphs.tsx` and `.css`: the progress kinds draw `BusyRoundel` in an `aria-hidden` wrapper, sized to the glyph box they replace. Keep `ActivityGlyph`'s props and its `data-kind` attribute.
   - `ui/load-state.tsx`: loading draws the roundel with its title.
   - New `ui/busy-text.tsx` exports `BusyText`, with a unit test. Write it first: V2, V3 and B1 use it.
   - `help/legend-entries.tsx` shows the roundel as the one sign of work in progress.
   - Reduced motion stops the spin.
   - The chat's own state glyphs are V3's.
4. **Spine type (issue 10, D11).** `feature/spine.css` and `feature/feature.css`: one step up. Update DESIGN.md "Type" and "Page rhythm" for the spine and, from V3's report through the main agent, for the chat.
5. **Graphite accent (issue 11, D12).** In `web/src/aurora-ui/shared/styles/tokens.css`, change only the Graphite block's `--primary`, `--primary-foreground` and `--ring`.
   - Choose a bright accent that pairs with the cool neutral 270-hue greys.
   - Check it as the primary button fill, the spine's current-row mark, the focus ring and the arrival flash.
   - Record the choice and its contrast ratio in DESIGN.md "Colour".

## Shots

You cannot build or shoot. The main agent shoots after your first pass, at 1440px and 390px in Ledger and Graphite, and sends misses to a fresh fable worker (common.md loop). Your report lists every surface you changed and the state needed to see it.

## Files

Your row in the plan's worker table. Never edit V3's, C1's, V2's, B1's or D1's files.

## Run once at the end

`node_modules/.bin/vitest run src/plan/ui src/plan/feature src/plan/help/legend.test.tsx src/plan/shell-footer.test.tsx src/plan/shell-footer.contracts.test.tsx src/plan/header`, from `web/`.
