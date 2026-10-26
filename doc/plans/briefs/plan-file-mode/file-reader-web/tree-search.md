# file-reader-web / tree-search: file mode layout, tree, outline, search, reading column

Tier: sonnet, Claude lane. Starts after codex wave A (R1-R6). Read `doc/plans/briefs/plan-file-mode/common.md` first.
Spec: `doc/product/section_06.md` "Layout and addresses", "The navigation tree", "Search",
"Diffs" (the "changed while open" behaviour). Design:
`doc/loom/knowledge/conventions/web-dashboard-typography.md`.

## Files you own

`web/src/plan/files/{file-mode.tsx, tree.tsx, tree-entry.tsx, tree-entry-extras.tsx, outline.tsx, search-box.tsx, search-results.tsx, reading-column.tsx, doc-header.tsx, lock-notice.tsx}`
and their `*.test.tsx`; `web/src/plan/tabs.ts` (append the Files tab entry only);
`web/src/plan/help-keys.ts` (append a "File mode" group only).

## Layout

- Tab `{ id: "files", label: "Files", path: "/files", element: FileMode }` appended to `PLAN_TABS`.
- `FileMode`: an 18rem tree pane left of the reading column; below 900px the tree is a drawer opened
  from a control at the top of the reading column. The route `/files/*` selects the document from
  `parseAddress(location)`; `/files` with no path shows the tree and a one-line hint.
- `ReadingColumn`: fetches `DocView` on path change, renders `DocHeader` (title, path, state word and
  lock glyph for plans, the uncommitted-diff control `openDiff("head", path)` from
  `@/plan/diff` (stage `file-editor-web` builds it; until then link to `?diff=head` with the
  address helper), `LockNotice` (verbatim notice from `DocView.lock`), then the plan view for plans
  (`<PlanView doc={doc} />` from `plan-view`) or `<Markdown .../>` from `markdown`. Scroll to the
  anchor after render; when the address's anchor is not in `DocView.sections`, show a muted line
  "Section #<anchor> is not in this document any more" above the text and stay at the top.
  Loading, failure and absence use plan 2's `<LoadState/>` (`@/plan/ui/load-state`): loading ->
  `loading`; 404 -> `not_found` "This document no longer exists" with "Back to the tree" and, when
  the tree holds a plan with the same slug at another path, a link to it; any other failure ->
  `error` with Retry. A `too_large` document (`DocView.truncated`) renders as plain monospace text
  (no Markdown parse) under the notice "This file is <size>; file mode shows the first 1 MiB and does
  not edit files over 1 MiB."; no Edit control. When `openDocIsStaleAtom` turns true and nothing is being edited, store the
  current text in `previousOpenTextAtom`, refetch, keep the heading in view where it was, and show "changed just now" linking to `?diff=open`.
  When a plan is renamed (`tree_changed` shows the same slug at a new path), follow it.
- `tree-entry-extras.tsx`: `export function TreeEntryExtras(props: { entry: TreeEntry }): JSX.Element | null`
  returning null, with a doc comment naming plan 9 (the "changed since you looked" dot); an extension
  slot, not a stub.

## Tree

Four groups in order (Product, Plans, Knowledge, Other) then Archive (collapsed by default);
knowledge categories collapsed by default; each entry shows the title, the file name on hover;
gap entries dimmed with the word "missing" (opening one shows the matching checklist item from plan
2's `checklistAtom` and, through plan 2's `<OfferButton offer />` from `@/plan/offers/offer-button`,
its offer to a browser that may author); plan entries show state glyph plus word, lock glyph, last
pressure date, error count from `validationAtom`; brief bundles nest under their plan; knowledge
topics show their blurb as a second line. Live: rebuilt from `treeAtom`.
`Outline`: under the open document's entry, its `##` and `###` headings from `DocView.sections`,
nested, each a link; the heading in view is marked (IntersectionObserver).

## Search

`SearchBox` at the top of the tree: typing filters the tree by title and file name in the browser;
Enter calls `searchDocs(q, scope)` (query trimmed, at most 256 chars, the box refuses more); a scope
switch Product / Plans / Knowledge / All. `SearchResults` lists title, heading path and snippet
with the terms marked (text only, `<mark>` built from `terms`, never HTML from the server); opening
one navigates to `toAddress(path, anchor)` and passes `marks` to the renderer. Show "50 results
shown; refine the search" when `truncated`.

## Help keys

Append `{ title: "File mode", rows: [["/", "Focus search"], ["Esc", "Clear search"]] }` (match
`HelpShortcutGroup`'s real fields) and implement both keys in `SearchBox`.

## Tests

Group order and Archive collapsed; knowledge categories collapsed; a gap entry renders "missing"
and opening it shows the checklist item and its offer; an entry shows its title with the file name
in `title`; a running plan shows its state word and lock glyph, a plan with errors its count; the
tree becomes a drawer below 900px (mock `matchMedia`); a stale open document refetches in place,
keeps the heading, and shows "changed just now" linking to `?diff=open`; filter by title; Enter calls the
search with the scope; a result's snippet with `<b>` renders as text; outline nests `###` under `##`;
lock notice wording; following a rename; a missing anchor shows its note; a 404 shows not_found
with the renamed plan's link; a failed fetch shows Retry; a `truncated` fixture renders monospace
with the size notice and no Markdown elements.

## Done when

`cd web && node_modules/.bin/tsc -b --noEmit`, `oxlint --deny-warnings`, `oxfmt --check`,
`vitest run src/plan/files` pass.
