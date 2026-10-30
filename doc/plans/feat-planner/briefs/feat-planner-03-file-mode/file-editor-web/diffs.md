# file-editor-web / diffs: the three diff kinds

Tier: sonnet, Claude lane. Wave 1, after the Claude `codemirror-core` worker and codex wave 0. Read
`doc/plans/briefs/feat-planner-03-file-mode/common.md` first. Spec: `doc/product/section_06.md` "Diffs",
"Taking a passage into chat" (selection in a diff).

## Files you own

`web/src/plan/diff/{index.ts, diff-view.tsx, diff-page.tsx}` and their tests (`diff-view.test.tsx`,
`diff-page.test.tsx`). Codex units own `open-diff.ts` (E7) and `diff-provenance.ts` (E8) with their
tests (specs below); `index.ts` re-exports them. Also yours: `web/src/plan/files/file-mode.tsx` (route `?diff=` to `DiffPage` only; it is
`tree-search`'s file from the previous stage).

## Shapes (binding; plan 9 uses them)

- `export function DiffView(props: { before: string | null; after: string | null; path: string }): JSX.Element`:
  loads CodeMirror through `loadCodeMirror()` and renders `createDiffView` read-only, side by side
  at 900px and wider, unified below (re-render on a media-query change); `before: null` diffs a
  created file against nothing, `after: null` shows a deleted file's last text as removed.
- `export function openDiff(kind: "turn" | "open" | "head", path: string, turn?: string): string`
  returns the address `/files/<path>?diff=<kind>[&turn=<id>]` (use `toAddress` from
  `@/plan/files/address`).
- `DiffPage` (rendered by `FileMode` when `parseAddress` has `diff`): `head` -> `fetchHeadDiff`;
  `turn` -> `fetchTurnDiff(turn, path)`; `open` -> before = `previousOpenTextAtom.text` (set by the
  reading column before each in-place update, stage `file-reader-web`), after = the current
  `DocView.text`; with no previous text for that path, show "Nothing changed while open".
  A 404 from either fetch renders plan 2's `<LoadState state="not_found"/>` with, for `turn`, "This
  turn's diff is gone: loom plan restarted. The uncommitted diff still shows the work." and a link
  to `openDiff("head", path)`; for `head`, "This document no longer exists"; other failures use the
  `error` state with Retry. `openDiff`'s `turn` argument is the full turn id (plan 7's
  `<session id>/t<n>`), encoded by `toAddress`. Every browser may open diffs (no holder check). A header names the kind and the path with a link
  back to the document.
- Add to chat in a diff: `diff-provenance.ts` exports `diffProvenance(path, sections)`, a provenance callback for selections in either
  side (exported: plan 9 reuses it): path, anchor by line, the selected source text, `lines`, `stageId: null`.

## Tests

`openDiff` addresses; `DiffPage` picks the fetcher by kind; a created file (`before: null`) and a
deleted file (`after: null`) render; a turn 404 shows the restart text and the head-diff link; a
turn id with `/` round-trips through `openDiff` and `parseAddress`; unified below 900px (mock `matchMedia`); diff provenance.
Mock `loadCodeMirror`.

## Done when

`cd web && node_modules/.bin/tsc -b --noEmit`, `oxlint --deny-warnings`, `oxfmt --check`,
`vitest run src/plan/diff` pass.
