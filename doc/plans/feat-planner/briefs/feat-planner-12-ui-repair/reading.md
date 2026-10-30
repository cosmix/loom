# Stage reading

Read `common.md` first. Working directory `web/`; paths are relative to `web/src/plan/`.

Targets, from the mockup's notes:
- Reading: "Each cap is a 1.3em disc in the document's colour with a heavy chevron. Hovering or
  focusing either cap tints the whole run it will fold, and a tooltip names the key."
- Headers: "One header grid for every document: breadcrumb, the title in Inter with its DocMark
  and right-aligned actions, one row of icon chips, and a quiet path line that carries
  'Uncommitted changes'."

Defects the browser tour found:
- **Keyboard and marks:**
  - Enter on a focused sentence opens nothing;
  - Shift+Esc does nothing while focus sits on a run's cap;
  - every source mark on the feat-planner product overview is the same grey;
  - the reading column has no key hint row.
- **Document key:** it shows all 12 chips over four rows plus a "Fold all" text button.
- **Caps and runs:**
  - the cap tooltip is clipped by the heading above it;
  - an opened source passage shows one cap on its own row above a grey block.
- **Document headers:**
  - breadcrumbs start with "Planner" and show slugs;
  - titles are light 24px and wrap beside the actions;
  - section documents have no chip row;
  - "Not pressure-tested" is a grey pill;
  - a done plan shows two dead buttons captioned "No offer is available";
  - file-tree rows wrap their state onto a second line.

## R1: reading surface (sonnet)

**Files owned:**
- `concept/concept.css`
- `concept/sentence.tsx`
- `concept/marks.tsx`
- `concept/document-key.tsx`
- `concept/concept-route.tsx`
- `markdown/markdown.css`
- their tests: `concept/document-key.test.tsx`, `concept/marks.test.tsx`,
  `concept/source-marks.test.tsx`, `concept/sentence*.test.tsx`, `concept/concept-route*.test.tsx`

**Workspace column** (`concept/concept-route.tsx`, `LoadedPage` `:202-221`):
- Padding `32px clamp(16px,4vw,56px) 48px`.
- The measure is `max-width: 68ch` and centred (`margin-inline: auto`); today it is
  left-aligned.
- A 1px `--hairline` right border on the reading `main` where the chat panel follows.
- Overview eyebrow: "<feature title> · product overview" (or "engineering overview").

**Headings inside the column:** Inter 650, 1.15em, line-height 1.3, letter-spacing -0.015em,
margin `36px 0 12px` (none on the first), `text-wrap: balance`.

**Inline code** in sentences and Markdown: 0.8em, padding `0.06em 0.32em`, radius 4px, `--muted`
fill.

**Document key** (`concept/document-key.tsx:120-137`):
- flex, wrap, gap 6px, margin `6px 0 30px`.
- **Chip:** 28px high, gap 7px, padding `0 10px 0 4px`, 1px `--hairline`, radius 8px, `--card`
  fill, 12.5px/500.
  - The DocMark is flush at the left padding.
  - The number is mono 11px muted, then the title with its leading "NN — " removed.
  - A stale chip keeps its dashed border (`data-dashed`).
  - Keep exactly one link per chip.
- Show the first four chips, then a "… <n> more" chip (`MoreHorizontal`) that expands the
  rest in place and becomes "Fewer".
- Remove "Fold all" (Shift+Esc and the header's "Open all" cover bulk control). Update
  `document-key.test.tsx` and the help scene note in `memory` for the chat stage.

**Sentence hover, the ink wash** (`concept.css:50-123`, `concept/sentence.tsx:32-77`):
- On hover and focus-visible of an openable sentence:
  - background `color-mix(in oklch, var(--sentence-colour) 20%, transparent)`;
  - `box-shadow: inset 0 -2px 0 color-mix(in oklch, var(--sentence-colour) 85%, transparent)`;
  - radius 3px, `box-decoration-break: clone`, transition 120ms;
  - no `text-decoration` underline.
- Extend the wash over the marks: put the hover on `.concept-sentence-wrap`, using `:has()` as
  `concept.css:83-95` already does.
- Update `source-marks.test.tsx:58`, which pins `text-decoration: underline 2px`, to the
  box-shadow rule in the same edit.
- Marks: 0.5em squares, radius 2px, `margin-inline-end: 0.18em`, `vertical-align: 0.08em`, in the
  document colour. A dashed mark is transparent with `outline: 1.5px dashed` in the colour and
  `outline-offset: -1.5px`. On sentence hover they scale to 1.35.
- The `ChevronDown` hint is absolutely placed after the marks (`left: 100%; top: 50%;
  translateY(-50%)`), 0.85em, in the document colour, opacity 0 → 1. It takes no inline width.
- Keep `MarkSquare` a button with its tooltip, and the inline `style.background`
  (`marks.test.tsx:40-42`).

**Hint row** under the column: flex, wrap, gap `8px 16px`, 12px, `--muted-foreground`, margin-top
6px, using `Kbd` (`web/src/components/ui/kbd.tsx`; mono 11px, padding `1px 5px`, 1px border with
a 2px bottom border, radius 4px, `--card` fill):
- `Enter` opens the focused sentence;
- `Esc` folds one level;
- `⇧ Esc` folds all;
- `a` adds the sentence to chat.

Proof: `node_modules/.bin/vitest run src/plan/concept/document-key.test.tsx src/plan/concept/marks.test.tsx src/plan/concept/source-marks.test.tsx`, once.

## R2: runs, caps, keyboard and colours (opus, loom-senior-software-engineer)

**Files owned:**
- `concept/run.tsx`
- `concept/fold-handle.tsx`
- `concept/column-keys.ts`
- `concept/reading-column*.tsx`
- `concept/reading-column*.ts`
- new `concept/open-all.ts`
- every non-JSX module under `concept/` that assigns document colours, which you locate first
- their tests: `concept/fold-handle.test.tsx`, `concept/fold-caps.test.tsx`,
  `concept/reading-column*.test.tsx`, `concept/chain.test.tsx`, `concept/concept.contracts-runs.test.tsx`

This is debugging work, so it goes to opus. Reproduce each defect in a test before fixing it.

1. **Enter opens nothing.** In the browser, `j` focused a sentence and Enter did nothing; arrow
   keys reached `.concept-passage` elements that open nothing either.
   - Read `column-keys.ts:200-225` and the roving-focus code in `reading-column.tsx`.
   - Make Enter, Space and ArrowRight open the focused openable sentence.
   - Make `j`/`k` and ArrowDown/ArrowUp move only between openable sentences and caps, never
     onto passage bodies.
2. **Shift+Esc from a cap does nothing.**
   - `columnKeyHandler` matches `button.concept-run-cap` for Esc.
   - Make Shift+Esc fold every open run from any focus inside the column, caps included.
   - After folding, focus returns to the sentence that opened the outermost run.
3. **Every mark is grey.** On `/f/feat-planner/product`, all marks rendered
   `var(--muted-foreground)` (colour 0).
   - Find where chips and marks get their colour index, and why the overview's cited documents
     get 0.
   - Give every document in the key its own `--doc-n` (1-8, cycling in key order).
   - Colour 0 stays muted only for a citation whose document is not in the key
     (`marks.test.tsx:40-42` pins the 0 → muted mapping).
4. **Caps and runs.**
   - Run (`.concept-run`): fill `color-mix(in oklch, var(--run-colour) 10%, transparent)`, radius
     5px, padding `0.14em 0.1em`, `box-decoration-break: clone`.
   - When a direct cap is hovered or focused, the run turns 22% and gains
     `box-shadow: 0 0 0 1px` at 45%, in place of today's outline at 55%.
   - Cap (`.concept-run-cap`): a 1.3em disc in the document colour, margin `0 0.22em`,
     `vertical-align: -0.24em`, `box-shadow: 0 1px 0 oklch(0 0 0 / 0.12)`, icon 0.78em at stroke
     2.5, text in `--background`. Hover and focus scale it to 1.12.
   - Tooltip "Fold · Esc": place it above, or below when it would collide with the content
     above. It must never be clipped by a heading (use the Radix tooltip's collision handling,
     `avoidCollisions` and `collisionPadding`).
   - Accessible names stay exactly "Fold" on both caps, with `data-side` start and end and
     `aria-describedby` on the run group (`conventions/plan-web.md`, "Concept Mode Test
     Rules").
5. **Passage runs** (`run.tsx:185-220`):
   - Remove the two `.concept-run-edge` rows.
   - The start cap sits inline at the start of the first passage's chip line, then the source
     chip, then the passage text.
   - The end cap sits inline after the last passage's last line (inside its final block,
     appended to the last paragraph when the block is a paragraph, otherwise on a trailing line).
   - A single-paragraph passage flows inline (`.concept-passage .md > p:only-child { display:
     inline }`); tables, lists and code stay block.
   - Source chip: inline-flex, height 1.35em, padding `0 0.45em`, radius 5px, `--card` fill, 1px
     `--hairline`, Inter 0.6em/500, muted, a `FileText` icon and "<file> › <heading>". It opens
     the file.
6. **Open all.** New `concept/open-all.ts` exports `useOpenAll(): () => void`, which opens every
   openable sentence of the current page through the open-runs state. R3 mounts it as the
   header's "Open all".

Proof: `node_modules/.bin/vitest run src/plan/concept`, once, after R1 has returned. You share
the directory: run only after the orchestrator says R1 is done, or run your own files only.

## R3: document headers and the file view (sonnet)

**Files owned:**
- `ui/doc-heading.tsx`
- `ui/doc-heading.test.tsx`
- `files/doc-heading-view.tsx`
- `files/doc-heading-view.test.tsx`
- `files/lock-notice.tsx`
- `files/plan/**`
- `files/tree*.tsx`
- `files/tree*.test.tsx`
- `concept/page-heading.tsx`
- `concept/page-heading.test.tsx`
- `concept/page-header-extras.tsx`
- `concept/page-header-extras*.test.tsx`
- `concept/concept-route-headers.test.tsx`

**Header** (`ui/doc-heading.tsx:255-278`):
- `--card` fill, 1px `--hairline` bottom border, padding `18px 22px 14px`, grid gap 10px.

**Breadcrumb:** flex, wrap, gap 6px, 12.5px muted.
- The first crumb is the feature title with a `Layers` icon, linking to `/f/<feature>`. The
  "Planner" root crumb goes.
- Then the area ("Product spec", "Plans", "Engineering"), then the document. A plan's crumb is
  its slug in mono. A section's reads "Section 05 of 12".
- Standalone plans start at "Plans".
- Update `files/doc-heading-view.test.tsx:67-91` and `concept/page-heading.test.tsx:164-212`.

**Title row:**
- A grid `auto minmax(0,1fr) auto`, gap 12px: the large DocMark, then the `h1` (Inter 22px/650,
  line-height 1.2, letter-spacing -0.02em, `text-wrap: balance`), then the actions.
- The actions do not shrink the title below one line at 1280px. Make the actions wrap under the
  title below 900px.

**Actions:**
- View switch: the segmented control from `common.md`, with `Sparkles` "Summary" and `FileText`
  "Source".
- Edit: an outline icon button with `Pencil`. While the document is locked, show it disabled,
  with a `Lock` icon and the title "Read-only while running" (or the lock's reason). Today it is
  hidden.
- Discuss: `MessageSquarePlus`.
- Summary pages add an outline "Open all" (`UnfoldVertical`) that calls `useOpenAll()` from
  `concept/open-all.ts` (R2 writes it).

**Chip row:** one flex, wrapping row, gap 6px, of status chips (`common.md` `.m` measures, with
`go`, `ok` and `warn` tones). Fold the extra rows of `page-header-extras.tsx` (open questions,
"Built by") into this row as chips.
- **Plan:**
  - the state (Running is `go` with the animated running glyph; Done is `ok`);
  - "Implements §NN, §MM" as links;
  - "Not pressure-tested" (`warn`, `Gauge`) or "Pressure-tested <date>" (`ok`,
    `FlaskConical`);
  - "Valid · N warnings" (`ok`, `ShieldCheck`, or `warn` with warnings);
  - "Changed <ago>" (`Clock`).
- **Section:**
  - "Summary current" (`ok`) or "Summary rebuilding" (`go`);
  - "<n> open questions" (`warn`);
  - "Built by 05 · 09";
  - "Changed <ago>".
- **Summary page:** the state, a pressure chip, Implements and the summary state.
- Implements targets that do not resolve render plain, not struck through. The strike-through
  came from the old feature split.

**Path row:**
- flex, gap 8px, 12px muted: a `File` icon, the mono path, and the copy button.
- At `margin-left: auto`, a 7px `--tone-warning` dot as a sibling `span aria-hidden` outside
  the link, then the "Uncommitted changes" link. `ui/doc-heading.test.tsx:288-296` pins that the
  link contains only its text.
- Summary pages have no path row.

**Lock notice** (`files/lock-notice.tsx` and the lock branch):
- A box with flex, gap 8px, padding `8px 10px`, radius 8px, `--muted` fill, 12.5px muted, and a
  `Lock` icon.
- Copy: "This plan is running. `loom run` works from the committed copy, so the app keeps it
  read-only." Done plans: "This plan is done. Record changes in a new plan."

**Done plans** (`files/plan/**`): do not render an offer button whose only content is "No offer
is available". When a plan has no offer, render nothing there.

**File tree** (`files/tree*.tsx`): a row keeps its lock icon and state label on the same line as
the name. The name truncates with an ellipsis, and the state is a fixed-width right-aligned
label.

Proof: `node_modules/.bin/vitest run src/plan/ui/doc-heading.test.tsx src/plan/files src/plan/concept/page-heading.test.tsx`, once.
