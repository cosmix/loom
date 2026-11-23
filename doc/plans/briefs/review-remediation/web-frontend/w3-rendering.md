# web-frontend / w3-rendering: diagram safety, bidi controls, tray semantics, dialog tab order

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/web.md`, sections CR-62, CR-63, U6, CR-78.e.

All paths below are relative to `web/` unless they start with `doc/`. Run commands from `web/`.

## Findings covered

- CR-62: mermaid's own sanitiser allows HTML (`img`, `a`, `style`) inside `<foreignObject>` even at `securityLevel: "strict"`; `MermaidBlock` injects the result with `dangerouslySetInnerHTML`. The "race on `mermaid.initialize`" claim is not real (validation: `draw` initialises and renders in one tick); add no promise queue.
- CR-63: bidirectional override characters in body Markdown (text, inline code, raw HTML text, image alt), in plain code blocks and in the stage summary reach the DOM as live controls; only chat text and summary Markdown neutralise them. `routes/stage.tsx` is refuted: do not touch it.
- U6: the two attachment trays have `aria-label` on a role-less `div`, which assistive technology does not expose.
- CR-78.e: `DialogFooter` reverses visual order below the `sm` breakpoint (`flex-col-reverse`), so Tab runs bottom to top.

## Files

Own (write):
- `src/lib/visible-bidi.ts` (new), `src/lib/visible-bidi.test.ts` (new)
- `src/plan/chat/visible-controls.ts`
- `src/plan/markdown/sanitize-diagram.ts` (new)
- `src/plan/markdown/mermaid-block.tsx`, `src/plan/markdown/mermaid-block.test.tsx`
- `src/plan/markdown/inline.tsx`, `src/plan/markdown/render.tsx`, `src/plan/markdown/plain-code-block.tsx`, `src/plan/markdown/markdown.test.tsx`
- `src/components/stage-detail.tsx`, `src/components/stage-details.test.tsx`
- `src/plan/chat/chips.tsx`, `src/plan/chat/chips-labels.test.tsx`
- `src/components/ui/dialog.tsx`

Read only: `src/plan/markdown/context.ts`, `src/plan/markdown/render-test-utils.tsx`, `src/plan/chat/visible-controls.test.ts`.
Frozen (contract tests, never edit): `src/plan/markdown/mermaid-block.contract.test.tsx`, `src/plan/markdown/bidi.contract.test.tsx`.

## Changes

### 1. CR-62: post-sanitise the rendered diagram

Today `draw` in `src/plan/markdown/mermaid-block.tsx` returns `{ svg }` straight from `mermaid.render`. mermaid's output is HTML-serialised by DOMPurify (so `<br>` and `&nbsp;` appear), which means an XML parse of it can fail on legitimate diagrams. Parse it as HTML instead.

Create `src/plan/markdown/sanitize-diagram.ts` exporting `sanitizeDiagram(svg: string): string | null`:

1. `const doc = new DOMParser().parseFromString(svg, "text/html"); const root = doc.body.querySelector("svg"); if (!root) return null;`
2. For every element of `root.querySelectorAll("*")` whose `localName.toLowerCase() === "foreignobject"` (a case-insensitive match on purpose: the HTML parser keeps `foreignObject` camel-cased, mermaid's allow-list is lower-cased): remove each descendant matching `img, image, picture, video, audio, source, style, script, iframe, object, embed, link`; replace each descendant `a` with a text node holding its `textContent` (`a.replaceWith(doc.createTextNode(a.textContent ?? ""))`).
3. Return `root.outerHTML`.

Only content inside `foreignObject` is stripped: mermaid puts its own theme `<style>` at the top level of the svg and it must stay. Do not import `dompurify` (a transitive dependency only). Do not add a dependency.

In `draw`: after `const { svg } = await mermaid.render(id, source);` do `const safe = sanitizeDiagram(svg); if (safe === null) throw new Error("diagram is not an svg");` and return `{ svg: safe }`. The existing `catch` removes mermaid's temporary `d<id>` element and returns `{ error: true }`, so the block falls back to the source with its note. Update the comment above `dangerouslySetInnerHTML` to say the markup is mermaid's output (securityLevel `strict`) reduced by `sanitizeDiagram`, because mermaid's sanitiser allows HTML inside `foreignObject`. Leave `mermaid.initialize` as it is.

### 2. CR-63: a bidi-only neutraliser

Decision: not all of `\p{Cf}`. The full set rewrites legitimate text (U+200D joiner in emoji sequences, U+200C in Persian and Indic scripts, soft hyphen, word joiner). Body text gets only the bidirectional controls.

- Create `src/lib/visible-bidi.ts` (pure, no imports; `src/components` is shared with the dashboard entry, so the helper must not live under `src/plan`). It exports `codePointLabel(character: string): string` returning `⟨U+XXXX⟩` (upper-case hex, padded to 4 digits; the same format `visibleControls` uses today) and `visibleBidi(text: string): string` replacing every match of `/[\u061C\u200E\u200F\u202A-\u202E\u2066-\u2069]/gu` with `codePointLabel`.
- `src/plan/chat/visible-controls.ts`: import `codePointLabel` and make `visibleControls` `return text.replace(/\p{Cf}/gu, codePointLabel);`. Its output must stay byte-identical (`visible-controls.test.ts` pins it). `Untrusted` and `ISOLATED_LTR` are untouched.
- `src/plan/markdown/inline.tsx`, `renderInline`: `text` -> `markText(visibleBidi(node.value), context.marks)`; `html` -> `rawHtmlText(visibleBidi(node.value))`; `image` -> `visibleBidi(node.alt || node.url)` in both the `insideLink` return and the `SafeLink` child; `footnoteReference` -> `visibleBidi(node.label ?? node.identifier)` inside the `sup`. `CodeSpan`: keep `repoRel(value)` on the raw value for the path match, render `visibleBidi(value)` inside both `<code>` elements. `renderReference`: the `imageReference` label becomes `visibleBidi(node.alt || node.label || node.identifier)`. Leave `renderSummaryInline` on full `visibleControls`. Import `visibleBidi` from `@/lib/visible-bidi`.
- `src/plan/markdown/render.tsx`, `renderBlock`: case `html` -> `rawHtmlText(visibleBidi(node.value))`; case `footnoteDefinition` label -> `visibleBidi(node.label ?? node.identifier)`. Leave `headingText` raw (it feeds anchors and comparisons, not display).
- `src/plan/markdown/plain-code-block.tsx`: render `{visibleBidi(value)}` inside `<code ref={code}>`; keep `CopyButton text={value}` raw so a copy returns the true bytes; add a one-line comment saying so. `selectContents(code.current)` selects the displayed text; that is fine.
- `src/components/stage-detail.tsx`, `StageSummaryText`: render `{visibleBidi(summary)}` (import from `@/lib/visible-bidi`); the existing comment already says the text is untrusted plain text.

### 3. U6: `src/plan/chat/chips.tsx`

Add `role="group"` to the two divs: `<div aria-label="Pending attachments">` (in `PendingChips`) and `<div aria-label="Sent attachments" className="chat-chips">` (in `SentChips`). Keep each on one line (the file is 396 lines; do not add lines). No CSS keys off these being role-less.

### 4. CR-78.e: `src/components/ui/dialog.tsx`

In `DialogFooter`, change `flex-col-reverse` to `flex-col` in the class string (`... flex flex-col gap-2 ... sm:flex-row sm:justify-end`). The file is shadcn-generated, excluded from the linter and from `oxfmt`, and written without semicolons: edit that one token and do not reformat the file. The only consumer is `src/plan/run/abandon-dialog.tsx` (`ConfirmationActions`: Cancel first, then the destructive button), so on a narrow screen the order becomes Cancel above Abandon, matching Tab order.

## Interfaces shared with other workers

None. `visibleBidi` and `codePointLabel` are new in `src/lib/visible-bidi.ts` and used only by files this worker owns.

## Tests

1. CR-62, in `mermaid-block.test.tsx` (hoisted `mermaid` mock with `render`, `SOURCE`, `CodeBlock`):
   - `mermaid.render.mockResolvedValue({ svg: '<svg xmlns="http://www.w3.org/2000/svg"><style>.a{}</style><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><img src="/api/x"/><a href="https://evil.example">t</a><span>label</span></div></foreignObject></svg>' })`; after `waitFor` on `.md-diagram`: no `img`, no `a`, text still contains `t` and `label`, and the top-level `style` element is still there. Fails today (img and a present).
   - A mock returning `{ svg: "<p>not a diagram</p>" }` falls back to the source with the note `This diagram could not be drawn.`
   - The existing tests keep passing (`<svg data-testid="drawn"></svg>` must still produce `.md-diagram svg`).
2. CR-63, in `markdown.test.tsx` (write the control characters in the test source as `\uXXXX` escapes, never as raw characters) (`renderMarkdown`, `container.textContent`): source with `a\u202Eb`, inline code `` `c\u2066d` ``, and a fenced block containing `e\u202Ef`: `textContent` contains `⟨U+202E⟩` and `⟨U+2066⟩` and none of the raw characters; the fenced block's copy button still copies the raw text (stub `navigator.clipboard` with a `writeText` mock as `markdown-render.test.tsx` does in its `beforeEach`, click the `Copy code` button, assert `writeText` was called with the raw `e\u202Ef`); a ZWJ emoji sequence `👨\u200D👩\u200D👧` and a U+200C between two letters stay unchanged in `textContent`. Fails today (raw controls present).
   - In `src/lib/visible-bidi.test.ts`: every character of the set maps to `⟨U+XXXX⟩`; U+200B, U+200C, U+200D, U+00AD, U+2060, U+FEFF are untouched.
3. CR-63 summary, in `stage-details.test.tsx`: render `<StageSummaryText summary={"a\u202Eb"} />` (exported from `src/components/stage-detail.tsx`) and assert the region's text is `a⟨U+202E⟩b`.
4. U6, in `chips-labels.test.tsx`: next to the existing `getByLabelText("Pending attachments")` and `getByLabelText("Sent attachments")` lookups add `screen.getByRole("group", { name: "Pending attachments" })` and `screen.getByRole("group", { name: "Sent attachments" })`. Fails today.
5. CR-78.e: no test (a class change).
6. Existing tests that must keep passing unchanged: `src/plan/chat/visible-controls.test.ts`, `src/plan/markdown/*.test.tsx`, `src/plan/chat/chips*.test.tsx`, `src/plan/run/abandon-dialog*.test.tsx` (if present).

Contracts to turn green: `diagram-label-image-removed`, `body-markdown-bidi-visible`.

## Proof

```text
node_modules/.bin/vitest run src/lib src/plan/markdown src/plan/chat/visible-controls.test.ts src/plan/chat/chips-labels.test.tsx src/plan/chat/chips.test.tsx src/components/stage-details.test.tsx src/plan/run
node_modules/.bin/tsc -b --noEmit
node_modules/.bin/oxlint --deny-warnings
node_modules/.bin/oxfmt --check src/lib src/plan/markdown src/plan/chat src/components/stage-detail.tsx src/components/stage-details.test.tsx
```

Do not run `vite build`. Report that you saw each new test fail before and pass after.

## Traps

- Do not strip `style` or `a` outside `foreignObject`; mermaid's theme CSS and its own SVG links live there.
- The HTML parser, not the XML parser: `new DOMParser().parseFromString(svg, "image/svg+xml")` fails on the `<br>` and `&nbsp;` that mermaid's DOMPurify pass leaves, and the diagram would fall back to source.
- `visibleBidi` must run before `markText`, and `CodeSpan` must match the document path on the raw value, or a path containing a control would stop linking and a search highlight would be computed on the wrong string.
- Applying `visibleControls` to body text is the near-miss: it breaks emoji sequences. The ZWJ assertion exists to catch it.
- Copy text stays raw in `PlainCodeBlock`; do not "fix" that for consistency.
- `chips.tsx` is 396 lines: a reflow by the formatter can push it over 400; check `wc -l` after `oxfmt`.
- `src/components/ui/dialog.tsx` is excluded from both the linter and `oxfmt` (`oxfmt --check` on it answers "Expected at least one target file"): a one-token edit is all it needs.
