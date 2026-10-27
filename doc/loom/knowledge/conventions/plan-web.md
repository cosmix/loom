# Plan Web

> Plan web rules

## Markdown renderer rules

`web/src/plan/markdown/` turns a document into React elements from an mdast tree (`mdast-util-from-markdown` with the GFM extension, `markdown.tsx`; `render.tsx` and `inline.tsx` walk the nodes). It is the planning app's main XSS surface, so these rules hold and `markdown.test.tsx` plus `markdown.contracts.test.tsx` pin each with adversarial inputs (scripts, event handlers, `javascript:` links, images, HTML in tables and code):

- **No raw HTML, no `dangerouslySetInnerHTML`.** An `html` node renders as inert text (`rawHtmlText`, class `md-raw`), never markup.
- **One link path.** Every link, autolink and image target goes through `resolveLink(fromPath, href)` (`files/address.ts`) and renders through `SafeLink`; nothing else builds an `href`. The allowlist is `http:`, `https:` and `mailto:` to an external host (`external`), a `#anchor`, or a path that stays inside `doc/` after decoding and resolving `..` (`doc`); a target that leaves `doc/` is `outside` and anything else, `javascript:` and `data:` included, is `text`.
- **Images are links.** An `image` node never becomes an `<img>`: the app loads nothing a document points at. It renders its alt text (or URL) as a `SafeLink`, or as plain text inside a link.
- **Entity-sensitive cases live in test strings.** `oxfmt` rewrites character references in `fixtures/*.md`, so those adversarial inputs stay inline in the test file ([why](../mistakes/plan-server-delivery.md#web-test-tooling-hid-or-created-defects-in-file-mode-2026-10-27)).
- The reading column is capped at 76ch and tables scroll in their own region; open accessibility and layout gaps are in [the file-mode follow-ups](../architecture/plan-server.md#file-mode-follow-ups).

## CodeMirror lazy boundary

The editor, merge view and diff views use CodeMirror 6 (`@codemirror/{state,view,commands,language,search,lang-markdown,lang-yaml,merge}`, `@lezer/highlight`). It must stay out of both initial bundles:

- Only `web/src/plan/editor/codemirror.ts` imports `@codemirror/*` or `@lezer/*` statically. Everything else reaches it through `loadCodeMirror()` in `editor/load.ts`, one `import("./codemirror")` per page whose rejected promise is cleared so the next call retries.
- `web/scripts/bundle-rules.json` states the rule for `scripts/bundle-check.mjs`: `index.html` and `plan.html` forbid `node_modules/@codemirror/**`, `node_modules/@lezer/**` and `src/plan/editor/codemirror.ts` in their chunks, and forbid the text `cm-editor`; `index.html` also forbids `src/plan/**`. `plan.html` forbids the terminal (`@xterm`, `components/terminal`), so plan code imports only leaf graph modules (`thread-edge`, `lib/graph`) and never `components/graph/stage-graph`.
- `load.test.ts` and `editor.contracts.test.tsx` scan sources for static imports of `@codemirror/`; the contract uses `import.meta.glob ?raw` and requires `codemirror.ts` to import statically and `load.ts` to import dynamically, so it cannot pass vacuously. `@lezer/` is covered by the bundle rules and a `load.test.ts` scan.
- `EditMode` renders through a data router (`createMemoryRouter`) on the default Jotai store from `@/state/store`, because `saveDoc` and the drafts read that store and `useBlocker` needs a data router. Drafts are written through `useDraftWriter` (`editor/draft-writer.ts`): 500 ms debounce, flushed on `beforeunload`, hidden visibility, blur and unmount, cancelled on discard. `saveFlow` is pure; draft clearing is signalled through state, and drafts are kept when the text changes during an in-flight save.
- Every editor, merge and diff view holds a colour-scheme compartment (`EditorView.darkTheme`) reconfigured by a `MutationObserver` on the `html` class. `theme.ts` uses only CSS variables (`theme.test.ts` checks each `var(--x)` exists in `index.css` or `tokens.css`, read as `?raw` with vitest `css.include`) and no colour literal.
- Selections in the editor and in diffs carry provenance for Add to chat (`editor-provenance.ts`, `diff/diff-provenance.ts`); a side-by-side selection carries a `side`, and the before side gets a null anchor. `AddToChat` clamps to the viewport, flips above, hides off-viewport and re-measures on capture-phase scroll and resize (`attachments/floating-position.ts`).
