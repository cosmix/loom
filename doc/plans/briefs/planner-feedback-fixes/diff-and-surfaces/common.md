# diff-and-surfaces: shared context

Read this before your own brief. The plan's Decisions D8 and D9 are authoritative
(`doc/plans/PLAN-planner-feedback-fixes.md`, renamed `IN_PROGRESS-` once the run starts). Paths are
relative to `web/`.

## Pinned: the block diff (W1 builds, W3 renders)

```ts
export type DiffBlock = {
  kind: "same" | "added" | "removed" | "changed";
  before: string | null; // a top-level block's source text; null for added
  after: string | null; // null for removed
};
export function diffBlocks(before: string, after: string): DiffBlock[];
```

- Blocks are the top-level `root.children` of `parseMarkdown(source)` (`src/plan/markdown/markdown.tsx`),
  sliced from the source by each node's `position.start.offset` and `position.end.offset`.
- Blocks are matched by a longest common subsequence over a normalized key: trimmed text with runs
  of whitespace collapsed. An adjacent run of removed blocks followed by added blocks pairs up one
  to one into `changed`; the remainder stays added or removed. The output is in document order.
- No new dependency.

## Pinned: `DiffView` (W3)

- `export function DiffView({ before, after, path, sections? })` keeps its props. Its four callers
  stay untouched: `diff/diff-page.tsx`, `review/changed-since.tsx:94`,
  `trace/settled-changes.tsx:117` and `review/built-marker.tsx:54`.
- It opens on the rendered view. Each block renders through the planner `Markdown` component inside
  an element with `data-diff-block="<kind>"`. Pass `sections={[]}`, empty `docPaths` and a no-op
  `onNavigate`, as `files/plan/stage-row.tsx:92` does.
- A toggle with buttons named `Rendered` and `Source` switches to the existing CodeMirror merge view
  and back. Add to chat works in Source as today.
- On open, the rendered view calls `scrollIntoView({ block: "center" })` on the first block whose
  kind is not `same`. The source view scrolls to the first merge chunk (`getChunks` from
  `@codemirror/merge`, then the editor handle's `scrollToLine`).

## Rules for every worker

- Write only the files your brief owns. No git commands. Never touch any `.loom/` path.
- Size limits: file 400 lines, function 50 lines, with a 10% margin. Split a component file that
  grows.
- Keep accessibility: focus rings, contrast at least WCAG AA in every theme, `prefers-reduced-motion`
  respected, and toggle buttons with `aria-pressed`.
- Themes: nine. Light themes are ledger (the default), gray, cool and harbor
  (`src/aurora-ui/shared/styles/tokens-light.css`). Dark themes are aubergine, Graphite (`gray`),
  midnight, obsidian and indigo (`tokens-dark.css`). Every visual change is checked in all nine.
- Never edit `src/plan/ui/DESIGN.md`: the parallel docs-flow stage owns it. Record every design
  choice with `loom memory decision "design-record: <choice and why>"`: tokens per theme, backdrop,
  diff colours, layout and breakpoints. Integration-verify writes those entries into `DESIGN.md`.
- No mention of AI tools in code, comments or docs.
- Frozen contract files (`*.contracts.test.ts[x]` from the contract session) are the spec. Never
  edit them.
