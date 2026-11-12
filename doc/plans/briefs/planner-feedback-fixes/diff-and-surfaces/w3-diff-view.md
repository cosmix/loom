# W3: the rendered diff view (fable, frontend-design)

Read `common.md` beside this file first. It pins `DiffBlock`, `diffBlocks` and the `DiffView`
behaviour. W1 built `web/src/plan/diff/block-diff.ts`; its report is pasted below this brief.
Before designing, load the `frontend-design:frontend-design` skill through the Skill tool and
follow it. W2 is restyling popover and dialog surfaces in parallel. Two of the diff's four callers
are dialogs (`trace/settled-changes.tsx` and `review/built-marker.tsx`), so check the result inside
them too.

## Files you own

- `web/src/plan/diff/diff-view.tsx`, `diff-view.test.tsx`
- `web/src/plan/diff/rendered-diff.tsx` (new) and `rendered-diff.test.tsx` (new)
- `web/src/plan/diff/diff.css` (new)
- `web/src/plan/editor/codemirror.ts` and `codemirror.test.ts` (only the first-chunk scroll on the
  handle)

Record design choices as `design-record:` memory decisions (see `common.md`); never edit
`DESIGN.md`.

## What to deliver

1. **Rendered view (issue 11).** `RenderedDiff({ before, after })` maps `diffBlocks` to rendered
   blocks.
   - `same` blocks render plainly; long unchanged runs may collapse into an expandable "N unchanged
     blocks" row, kept only if it reads better.
   - `added` and `removed` blocks carry clear insert and delete treatment.
   - `changed` blocks show before and after together: stacked on narrow widths, side by side from
     900px, matching `DiffView`'s existing breakpoint.
   - Every block element carries `data-diff-block`. The Markdown renderer is safe for untrusted
     text; use it as `common.md` says and add no `dangerouslySetInnerHTML`.
2. **Toggle.** `DiffView` gets a `Rendered` / `Source` segmented control with `aria-pressed`;
   Rendered is the default. Source is today's CodeMirror merge view (`useDiffEditor`, lines 30 to
   61), mounted only when chosen, so the rendered default costs no editor. Add to chat stays in
   Source.
3. **Look (issue 12).** Design the whole diff surface: header (path, change counts, the toggle),
   insert and delete colours tuned for all nine themes, gutters or markers, spacing and typography
   consistent with the reading column (`files/reading-column.tsx`). Restyle the CodeMirror merge
   decorations in Source to match, via `diff.css` scoped under the view's root class. Use project
   tokens; add tokens only where none fit (in `diff.css`, scoped), and record them as
   `design-record:` decisions.
4. **Scroll to the first change (issue 13).**
   - Rendered: after the first paint, scroll the first block whose kind is not `same` with
     `scrollIntoView({ block: "center" })`.
   - Source: add `scrollToFirstChange()` to the `EditorHandle` in `codemirror.ts` (lines 30 to 44).
     It reads `getChunks(state)` on the after side (`merge.b` in split mode, the unified view
     otherwise) and calls the existing `scrollToLine` with the first chunk's line; call it once
     ready.
   - Both scroll on open only, never again on re-render. Respect `prefers-reduced-motion` (no
     smooth scrolling then).
5. **Tests.**
   - `diff-view.test.tsx`: its CodeMirror cases press Source first (an expected integrity edit).
     Keep every assertion about split, unified, created, deleted, Add to chat and anchors.
   - Add `rendered-diff.test.tsx` cases: kinds and attributes, a created file (empty before) and a
     deleted file (empty after).
   - Add a `codemirror.test.ts` case for `scrollToFirstChange`.

## Verify

Drive the real screens (the `run` skill; `scripts/plan-screens.sh`). Screenshot:

- the `/changes` page diff;
- the diff page (`?diff=head`);
- the Settled changes dialog.

Take each in ledger, harbor, obsidian and Graphite, at 1280px and 600px wide. Save them under
`${LOOM_SCRATCH_DIR}` as `<theme>-<surface>-<width>.png`. Run `cd web &&
node_modules/.bin/vitest run src/plan/diff src/plan/editor src/plan/review src/plan/trace`.

## Report

The files changed, the design decisions, the screenshot paths, and any caller that needed a change
you could not make (callers are not yours).
