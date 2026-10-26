# file-reader-web / markdown: the safe Markdown renderer

Tier: opus, Claude lane (the app's main XSS surface). Starts after codex wave A (R1-R6). Read
`doc/plans/briefs/feat-planner-03-file-mode/common.md` first. Spec: `doc/product/section_06.md` "Rendering
Markdown"; `doc/product/section_12.md` "Untrusted content in the browser". Design:
`doc/loom/knowledge/conventions/web-dashboard-typography.md`.

## Files you own

`web/src/plan/markdown/{markdown.tsx, render.tsx, inline.tsx, table.tsx, code-block.tsx, heading.tsx, markdown.css, index.ts}`,
`web/src/plan/markdown/*.test.tsx`, `web/src/plan/markdown/fixtures/*.md`, `web/package.json` and
`web/bun.lock` (through `bun add` only).

## Dependencies

`cd web && bun add mdast-util-from-markdown micromark-extension-gfm mdast-util-gfm` (CommonMark
plus GFM tables, task lists, strikethrough, autolinks, as an mdast tree; nothing renders HTML
strings). They are imported only by `web/src/plan/markdown/`, so they enter the planning bundle
and never the dashboard's (plan 2's bundle check forbids `src/plan/**` in `index.html`'s closure).

## Component

`export function Markdown(props: { source: string; path: string /* repo-relative */; sections: SectionView[]; onNavigate: (address: string) => void; marks?: string[]; docPaths: ReadonlySet<string>; anchorsOf: (path: string) => ReadonlySet<string> | null; lineOffset?: number; headingIds?: boolean }): JSX.Element`

`lineOffset` (default 0) is added to each node's `position.start.line` before any lookup in
`sections`, so an excerpt (the plan view's prose after the metadata block, its pressure record,
plan 5's passages) maps to its real lines. `headingIds` (default true): when false, headings get
no `id` and no copy control (embedded excerpts and chat text, so the DOM never holds duplicate
ids).

- Parse with `fromMarkdown(source, { extensions: [gfm()], mdastExtensions: [gfmFromMarkdown()] })`
  and render each mdast node to React elements in `render.tsx`. Never `dangerouslySetInnerHTML`.
- `html` nodes: an HTML comment (starts `<!--`) is dropped (hides the `<!-- loom METADATA -->`
  markers); any other raw HTML renders as escaped text (a text node).
- Headings: `id` = the anchor from `sections` whose `start_line` equals the node's
  `position.start.line + lineOffset` (the server owns anchors; never compute them here); a hover control copies
  `location.origin + toAddress(path, anchor)`.
- Links: `resolveLink(path, url)` from `@/plan/files/address`: `doc` -> an `<a href={toAddress(..)}>`
  whose click calls `onNavigate` (and struck through with title "not found" when the path is not in
  `docPaths` or the anchor not in `anchorsOf(path)`); `anchor` -> in-page scroll; `external` ->
  `target="_blank" rel="noopener noreferrer"`; `outside` -> plain text with the target in `title`;
  `text` -> the link's text as plain text. Autolinks follow the same rules.
- Images: never an `<img>`; render the alt text as a link to the target, through the same link rule.
- Inline code exactly equal to a document path (`doc/...` form, or the path relative to `doc/`) in
  `docPaths` renders as a link to that document.
- Tables: inside a horizontally scrolling box; ledger styling from tokens `--ledger-rule`,
  `--ledger-head-rule` (`web/src/index.css`) in `markdown.css`; the page never scrolls sideways.
- Code blocks: monospace on a muted ground, the fence language as a small label, a copy control,
  horizontal scroll; no highlighting; `mermaid` blocks render as code.
- Task list items render a disabled checkbox; strikethrough as `<del>`.
- `marks`: case-insensitive whole-term highlights in text nodes (search results), as `<mark>`.

## Adversarial fixtures and tests (`markdown.test.tsx`, fixtures under `markdown/fixtures/`)

`<script>alert(1)</script>` as a block and inline; `<img src=x onerror=alert(1)>`;
`<a href="javascript:alert(1)">x</a>` (raw HTML: text); `[x](javascript:alert(1))`,
`[x](JaVaScRiPt:alert(1))`, `[x](data:text/html,<script>)`, `[x](vbscript:msgbox)`,
`[x](file:///etc/passwd)`, `[x](java&#x09;script:alert(1))`, `[x](&#32;javascript:alert(1))`,
`[x](&#x6A;avascript:alert(1))`, `[x](javascript&#58;alert(1))` (all text); `[x](//evil.example)`
(external, `rel="noopener noreferrer"`); `<https://x.example>` autolink (external); `![t](https://tracker.example/p.gif)` (a link, no img, no
request); HTML inside a table cell and inside a code span; `<!-- loom METADATA -->` dropped;
autolink `https://example.com` external with `rel`; a relative link to a missing doc struck through;
a GFM document with a table, a task list, strikethrough, an autolink and a fenced block renders
each (table inside the scroll box, disabled checkbox, `<del>`, external link, code block with its
language label and copy control); a `mermaid` fence renders as a code block; the heading link
control copies the heading's address; inline code naming a listed document path (build the string at runtime from `"doc/"` plus `"plans/briefs/x/common.md"`, so this brief names no path loom reads as a plan input) becomes a link. Assert on the rendered DOM:
no `script`, `img`, `iframe`, `object`, `on*` attribute, or `href` whose parsed protocol is outside
the allowlist anywhere in the container. Excerpts: a source whose `## Pressure tests` heading sits
at excerpt line 1 with `lineOffset: 289` gets the anchor of the section starting at file line 290;
with `path="doc/product/section_02.md"` (repo-relative, the only form callers pass; chat
replies pass `doc/README.md`), `[p](section_03.md#port)` is a `doc` link to `doc/product/section_03.md#port`
and `[q](../plans/PLAN-x.md)` to `doc/plans/PLAN-x.md`, while `[r](../../README.md)` is `outside`;
two `headingIds: false` renders of the same text in one container produce no element `id`.

## Done when

`cd web && node_modules/.bin/tsc -b --noEmit`, `oxlint --deny-warnings`, `oxfmt --check`,
`vitest run src/plan/markdown` pass.
