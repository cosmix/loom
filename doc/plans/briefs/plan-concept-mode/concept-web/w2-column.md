# concept-web W2: the reading column

Working dir `web/`. You own `src/plan/concept/{reading-column.tsx, reading-column.test.tsx,
sentence.tsx, marks.tsx, run.tsx, passage.tsx, fold-handle.tsx, fold-note.tsx, plan-glyph.tsx,
flash.ts, announce.tsx, concept.css}` and the `--doc-*` addition to `src/index.css`. W1's modules
exist; import them, do not edit them. The frozen contracts are
`src/plan/concept/concept.contracts.test.tsx`; its queries (roles, labels, `data-dashed`) are
binding.

## Tokens (`src/index.css`)

Add `--doc-1` to `--doc-8` beside the `--tone-*` tokens, in the `:root` block and the `.dark`
block, OKLCH like the tones. Hues spread over 150-360 and 0-40 (never 50-90: the amber
`--primary` sits near 80 and warnings at 50-58). Each value meets 3:1 contrast against
`--background` in its theme; write the eight pairs and their measured ratios in a CSS comment
(compute with the OKLCH-to-sRGB relative luminance formula in a scratch script; no dependency).
Expose them to Tailwind only if the file already exposes `--tone-*` that way.

## reading-column.tsx

`<ReadingColumn pageKey page passages />`: a `role="document"` element labelled "Reading
column", one tab stop (`tabIndex=0`), roving focus over `readingOrder(...)` with the keys of
section 05's table: `↓`/`j`, `↑`/`k`, `Enter`/`→` open, `Escape`/`←` fold innermost run around
the focus and focus the restored sentence, `f` navigate to `/files/<first rests_on path>#<anchor>`,
`d` navigate to the first mark's summary page, `a` call the `onAttach(target)` prop (W3 passes
plan 3's add-to-chat action; read-only browsers pass nothing and `a` does nothing). Keys are
ignored while typing (the same `isTyping` test as `web/src/routes/shell.tsx`). `g p`/`g e` are
W3's (global). On every page replacement (`page` prop identity changes) call
`carryOpenState(previous, page, open)` and store the result and its notes through W1's atoms
(the stage's wiring check greps `carryOpenState(` here). Scroll anchoring: before applying a
replacement record the focused sentence's (else the first visible sentence's) `top`, after
layout restore it with `scrollBy`, so content changing above never moves the text in view.
Opening scrolls only when the opened content would push the opened point off screen.

## sentence.tsx, marks.tsx, run.tsx, passage.tsx

- Sentence: inline `span` with `role="button"`, `aria-expanded`, `aria-controls` naming its run,
  hover `bg-muted`, pointer cursor; a click that ends a non-empty text selection does nothing.
  Text renders as a text node. A sentence with no citations is not a button and shows a muted
  "no source" mark. Data attributes for selection: `data-sentence-id`, `data-summary`.
- Marks: one small square per `Mark` after the final punctuation, `background:
  var(--doc-<colour>)`; out of date draws a dashed outline instead (`data-dashed="true"`).
  `aria-label` "<title>: <path>", plus ", out of date: <reason text>" when dashed; tooltip with
  the same text plus the reason (rebuilding, waiting for approval, cannot rebuild: <detail>,
  build failed). Click opens the document's summary page.
- Run: opened level-0 text replaces the sentence in the same paragraph with the cited level-1
  sentences, in citation order, on a faint `--muted` wash; `role="group"`, `aria-label` "Detail
  of " + first five words. Opened level-1 text breaks the paragraph: the passages render as
  blocks on `--card` with a hairline border, stacked in citation order, then the paragraph
  resumes. No coloured left border or spine.
- Passage: header line in muted chrome text `section_02.md › Port` linking to
  `/files/<path>#<anchor>` (with the line range so file mode highlights it), body through plan
  3's renderer, `data-passage="path#anchor"` on the block.
- `plan-glyph.tsx`: after the first sentence's marks of a product-section run whose `plans` is
  non-empty, a small glyph button opening a popover listing the plans (title, state) linking to
  `/engineering/<slug>`; the engineering side lists the page's `implements` sections linking to
  `/product/section_<nn>`.

## fold-handle.tsx, fold-note.tsx

Fold handle: a thin muted bracket in the left margin spanning the run, shown on hover or focus
within, nested per run, button labelled "Fold"; folds that run. Below `lg` it sits inside the
16px gutter. Fold note: muted line in the run's place with the text from W1's note and a link to
the file in file mode; it disappears when the reader opens or folds in that paragraph.

## flash.ts, announce.tsx

Flash: when a page replacement brings a sentence id that was not on the previous page (or a
passage whose hash changed), add a class that animates a `--primary` wash fading over about one
second; under `prefers-reduced-motion: reduce` show the wash without animation and clear it
after three seconds (as `web/src/graph.css` disables `thread-run`). Unchanged sentences do not
flash. Announce: one polite live region announcing "<page title>: <n> sentences updated", at
most once every ten seconds.

## concept.css

Reading text Inter 16px, line-height 1.6, column `max-width: 70ch`, centred, 16px gutter below
`lg`; headings in `--font-display`; focus ring `--ring`. Tests in `reading-column.test.tsx`:
handle folding, `f` and `d` navigation targets, no flash on unchanged sentences, reduced motion.

## Required test cases (coverage audit; each a named test)

- `doc-tokens.test.ts` (yours; add it under `src/plan/concept/`): read `src/index.css` as text,
  parse the eight `--doc-*` OKLCH values in `:root` and in `.dark`; "each token defined in :root
  and .dark"; "no hue in 50-90"; "contrast at least 3:1 against --background in both themes"
  (OKLCH to sRGB to relative luminance in the test; `--background` read the same way from
  `src/aurora-ui/shared/styles/tokens.css`).
- `reading-column.test.tsx`: "level-0 source citation opens passages"; "level-0 sentence citing
  two documents shows two marks in order"; "rebuilding mark dashed with reason text"; "mark
  tooltip names the reason"; "click ending a selection does not open"; "mark click navigates";
  "passage header link carries the line range"; "no source mark, not a button"; "paragraph
  resumes after two stacked passages"; "inset has no border-left style other than the hairline";
  "opened run carries the wash class"; "opening scrolls only when the opened point leaves the
  viewport" (stub `getBoundingClientRect` and `window.innerHeight`); "handle folding"; "nested
  runs have nested handles"; "ArrowLeft folds like Escape"; "j and k move like the arrows";
  "ArrowRight opens"; "f and d navigation targets"; "keys ignored while an input has focus";
  "no flash on unchanged sentences"; "new sentence flashes"; "passage flashes on hash change";
  "reduced motion"; "anchors on the focused sentence across a replacement" (stubbed rects,
  assert the `scrollBy` delta); "note links to file mode"; "announce throttled to ten seconds"
  (fake timers); "plan glyph popover links".
- Components to reuse by name: `Tooltip` (`web/src/components/ui/tooltip.tsx`), tokens from
  `web/src/aurora-ui/shared/styles/tokens.css`. `concept.css` must contain `max-width: 70ch` (a
  wiring check greps it).

## Fix-round additions (pressure test px-browser-15, -21)

- Passages render through `<Markdown source path={path} sections lineOffset={startLine - 1}
  headingIds={false} ... />`, where `path` is the wire's repo-relative `doc/...` path (plan 3's
  `path` prop is repo-relative; never pass `docRel(path)` there), so a cut never takes another section's id and two passages of
  one document never duplicate ids.
- Focus: whenever the focused target leaves `readingOrder` (Fold all, a rebuild that folds or
  moves its run, a removed passage), move the roving focus to its nearest surviving ancestor
  sentence (Fold all: the level-0 sentence that held the run; a moved run: the sentence it moved
  to) and keep the column focused; never let focus fall to `body`.
- Tests: "a relative link in a passage opens file mode" (a passage of
  `doc/product/section_02.md` linking `section_03.md#port` renders a link to
  `/files/product/section_03.md#port`, not outside text); "two passages of one document produce no
  duplicate ids"; "passage heading carries no
  id"; "Fold all moves focus to the outer sentence"; "a rebuild that folds the focused run moves
  focus to the restored sentence".
