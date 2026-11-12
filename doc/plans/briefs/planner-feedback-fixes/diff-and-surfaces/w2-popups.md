# W2: popups that stand out (fable, frontend-design)

Read `common.md` beside this file first. Decision D9 is yours. Before designing, load the
`frontend-design:frontend-design` skill through the Skill tool and follow it.

## The problem

In several themes, popovers and dialogs blend into the page. Every theme sets `--popover` equal to
`--card`, only a few lightness steps from `--background`:

- ledger: background 0.925, card 0.955;
- obsidian: 0.13 against 0.16;
- Graphite: 0.20 against 0.23.

A popover over a card is the same colour as the card. The only separation is
`ring-1 ring-foreground/10` and `--dialog-shadow`. Popovers are non-modal and have no scrim. The
operator suggests a glow and/or a blurred background.

## Files you own

- `web/src/plan/header/popover.tsx` (`PopoverContent`, lines 15 to 35)
- `web/src/components/ui/dialog.tsx` (overlay `bg-black/10 backdrop-blur-xs`; content
  `bg-popover ring-1 ring-foreground/10 shadow-(--dialog-shadow)`)
- `web/src/aurora-ui/shared/styles/tokens-light.css` (`--popover` at lines 13, 64, 109 and 154)
  and `tokens-dark.css` (lines 22, 69, 115, 160 and 205)
- `web/src/index.css` (`--dialog-shadow` at lines 102 and 163; `--dialog-backdrop` at line 104)
- `web/src/plan/plan.css` (`--plan-shadow` and `--popover-lift`, lines 41 to 49)
- `web/src/plan/header/activity.tsx` (`POPOVER_SHADOW`, line 29)
- `web/src/plan/trace/maps-findings-forms.css` (`.maps-picker`, lines 7 to 95)

## What to deliver

1. An elevated surface for every floating layer in all nine themes. Give it its own token (for
   example `--popover` distinct from `--card`, or a new `--surface-raised`), plus a clear edge: a
   border token with enough contrast and a soft glow or shadow tuned per light and dark theme.
2. A decision on backdrops.
   - Modal dialogs already have one; make it readable in every theme, with blur strength and scrim
     opacity tuned so content behind recedes.
   - For non-modal popovers, decide between a light scrim with blur and none. Name the reason in
     your report. A scrim on a popover must not block the trigger's own toggle, and Escape and an
     outside click must still close it.
3. The Findings "Fold into entry" picker (`trace/findings-fold.tsx` `FoldPicker`) is an inline
   fieldset, not a popup. It gets the raised surface and edge, so it reads as a panel within the
   card. Its chosen option uses a fill that differs from the card in every theme; today it uses
   `--background`, which reads as sunken.
4. Leave every popover and dialog consumer's markup unchanged. The change lives in the primitives
   and tokens. `chat/pressure/pressure-dialog.tsx` overrides to `bg-card border ring-0`; check that
   it still looks right, and report if it needs a change (that file is not yours).

## Verify

Drive the real screens (the `run` skill; `scripts/plan-screens.sh`). In each of the nine themes,
screenshot:

- the activity popover in the header;
- the feature switcher;
- a settle popover (`trace/settle-surface.tsx`);
- one dialog (help or settings);
- the Findings page with a fold picker open.

Save the screenshots under the stage scratch directory (`${LOOM_SCRATCH_DIR}`), named
`<theme>-<surface>.png`. Run `cd web && node_modules/.bin/vitest run src/plan/header
src/components src/plan/ui src/plan/trace` and fix what your change breaks in your files. Report a
failing test outside your files with its output.

## Report

The tokens you added or changed, per theme. The backdrop decision and why. The screenshot paths.
Record each choice as a `design-record:` memory decision (see `common.md`).
