# 05 — Concept Mode

Part of the [Loom Plan product vision](index.md).

This section defines what a reader sees in concept mode: the product overview,
the document summaries under them, how a sentence opens down to the text it came from and folds
back, how product sections and plans link to each other, how the view behaves while summaries
are rebuilt, and the empty states. Section 04 owns how summaries are produced and stored; this
section states only what the reader sees and what the reader needs from the summary data.

## Levels

Concept mode has three levels of text, each a step closer to the files:

| Level | Text                                                                                          | A sentence opens to                                        |
| ----- | --------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| 0     | The product overview                                                                           | The level 1 sentences it cites, from one or more documents |
| 1     | A document summary: the vision, one per product section, the feature's metaplan               | The source passages it cites                               |
| 2     | A source passage: one heading section of a file under `doc/`                                  | Nothing further; it links to file mode                     |

The vision has a document summary of its own: at most 8 sentences in at most 3 paragraphs, short
enough to read in under a minute (vision, Decisions, "Summary levels"). The metaplan has a
document summary of its own, read on its own page. A product overview sentence that draws on the
vision opens straight to its source passage, one level down instead of two, and the vision's
summary is not one of the overview's members.

A plan's summary has two levels of its own: an opening level of about one sentence per topic,
and a detail level of short paragraphs. A plan page reads its opening sentences, then its detail
sentences, then the passages they cite. An opening sentence opens to the detail sentences it
cites, in place, and a detail sentence opens to its source passages.

Depth is fixed at these levels. A passage is the end of the chain; the only way further is file
mode. A reader starting from a vision, section or metaplan summary page (below) has two levels:
summary and source. A reader starting from a plan page has three: opening, detail and source.

## The overview and metaplan

Concept mode is the default way to read anything that has a summary. A feature's spine
(vision, "The screen") reaches it from its Product spec and Plans rows, and every row below them
opens its page in concept mode with **Source** on the same page. A feature opens on one of
these pages by section 01's landing rule. The routes:

| Route                            | Renders                                                                                                                                                       | Spine row              |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------- |
| `/f/<feature>/vision`            | the vision's summary, with **Source** on the same page                                                                                                        | Vision                 |
| `/f/<feature>/product`           | the product overview, with the pace and section queue above it while sections are missing                                                                     | Product spec, Overview |
| `/f/<feature>/product/<doc>`     | a section's summary                                                                                                                                           | that section           |
| `/f/<feature>/metaplan`          | the metaplan summary, Run panel, plan gaps, pace and queue, with **Source** one click away                                                                    | Metaplan and Plans     |
| `/f/<feature>/engineering/<doc>` | a plan's summary                                                                                                                                              | that plan              |
| `/f/<feature>/engineering` and `/f/<feature>/run` | redirect to the metaplan page                                                                                                              |                        |

The metaplan route renders the summary of the feature's metaplan, `doc/plans/<feature>/index.md`; its heading's **Source** link opens the metaplan's text. A metaplan whose summary is not
built yet shows the "not built yet" state with a Rebuild action, never the raw text. The vision's
summary reads like a section's: its sentences open to the vision's own passages. File mode is
section 06. Standalone plans keep `/plans/<slug>`. A feature with a product spec opens on
`/f/<feature>/product`; otherwise one with a metaplan opens on `/f/<feature>/metaplan`,
and any other opens on `/f/<feature>/vision`, which offers to start one. The feature switcher in the
header links each feature to `/f/<feature>`, which lands by this rule.

- **Product overview**: built from `doc/product/<feature>/index.md` and the sections it names.
  Sections of that feature the vision does not name are listed under its overview as unlisted
  sections, each linking to its document summary.
- **Metaplan page**: shows the Run panel, plan-scope gaps, pace and queue alongside the metaplan
  summary. Unlisted plans link to their document summaries. Standalone plans have their own pages.

The product overview renders the headings and paragraphs section 04 produces, in the reading column,
centred, at most `70ch` wide. Above the text sits the **document key**: one compact chip per
document the overview draws on, in the vision's table order (section 01), each chip showing the
document's colour and number
in mono, then its short title without the leading `NN —`, so the number appears once. A chip
opens that document's summary page. The key is also where the reader sees which summaries are
out of date, without scanning the text. The spine lists the same documents; the key stays as the
reader's legend, the place that says which colour is which document.

### Document summary pages

Every document summary is also readable on its own: a section at
`/f/<feature>/product/<section stem>` (for example `/f/feat-planner/product/section_04`), a feature
plan at `/f/<feature>/engineering/<slug>`, and a standalone plan at `/plans/<slug>`.
The page header gives the breadcrumb, the title with its document mark in the document's
colour, a toolbar (the **Summary** / **Source** switch, whose Source segment opens the file's
text; the settle state and its action; Edit, Discuss and Open all), then a status area:
whether the summary is up to date with its Rebuild action, the open questions, and the cross
links below. Plan pages also show the plan's state (written, running, done, retired;
section 01) and the date of its last pressure test, if any. Running, done and retired plans show
the read-only notice of the vision's "Locked plans" decision.

A page's heading, and the heading of the same document's source page, also shows the document's
**settle state** (Draft, Settled, Revised, Stub changed or Upstream changed) next to the summary's
own state; the spine row of every written document carries the same state as a small glyph whose
word is its tooltip and accessible name. Settling is the reader's act, made from that heading
(section 03 owns the mark). Summaries are built for drafts and settled documents alike: a draft
plan is summarised so that it can be reviewed before it is settled, and settling does not start
or stop a rebuild. Under the heading a section or plan shows its **stub** (the vision row or
metaplan entry that set its scope) linked to its place, with a note when the stub changed after
the document was settled. A plan's pages also list its **brief files**, each linked to its source
page; the briefs are part of what the plan's settled mark covers.

The body is the summary's text, every sentence opening to its source passages exactly as level 1
sentences do inside an overview. A plan page shows its opening level; each opening sentence opens
to its detail sentences in place, and each detail sentence opens to its source passages.

## Sentences and their marks

Every summary sentence ends with its **marks**: one small square per document it draws on, after
the final punctuation, in that document's colour. A level 0 sentence citing sections 02 and 07
carries two marks; a level 1 sentence carries one, its own document's.

- **Colour.** Documents take colours from a palette of eight tokens, `--doc-1` to `--doc-8`,
  added to `web/src/index.css` beside the `--tone-*` tokens, with light and dark values in the
  dashboard's themes. The hues avoid the amber accent (`--primary`, hue ~80) and the orange
  warning hues (50-58) so a mark never reads as focus or warning
  (`conventions/web-dashboard-typography.md`). Colours are assigned in document key order per
  overview and repeat after eight. The same document keeps its colour at every level of that
  overview and on its own summary page. (My call.)
- **Out of date.** A mark is drawn as a dashed outline when that document's summary is out of
  date: rebuilding, waiting for the user to approve a rebuild above the budget (vision, "Rebuild
  budget"), or unable to rebuild (missing `claude` or `tmux`, section 02; a failed build, section
  04). The sentence stays readable and openable. The mark's tooltip says which of these holds.
- **Not colour alone.** Hovering or focusing a mark shows a tooltip with the document's title and
  path. Colour helps the eye group sentences; the tooltip, the key and the summary page name the
  document. Marks meet a 3:1 contrast with the background in every theme (WCAG 1.4.11).

## Opening and folding

### What is clickable

- **A sentence.** Clicking anywhere on a summary sentence opens it. Hover tints its background
  with an ink wash in its document's colour and a 2px underline in that colour; its marks scale
  to 1.35 and a small `ChevronDown` fades in after them. Keyboard focus looks the same. The
  cursor is a pointer. A click that ends a text selection does not open anything (selection is
  for chat, below).
- **A fragment.** Consecutive sentences of a paragraph or an opened run that open to the same
  content (a shared source passage, or the same cited sentences) form one fragment: one hover
  wash over all of its text, one chevron after its last mark, one button. Opening it shows what
  its sentences open to, each passage once.
- **A mark.** Clicking a mark opens that document's summary page.
- **A key chip.** Opens that document's summary page.
- **The passage header.** Each source passage starts with a small chip naming the file and
  heading (`section_02.md › Port`), which opens file mode at that heading (section 06), with
  the passage's lines highlighted.
- **A fold cap.** Described below.

Sentences that cite nothing are a defect (vision, principle 3) and are not clickable; section 04
must not produce them. If one appears, it shows a muted "no source" mark instead of colour marks
so the reviewer can see and report it. (My call.)

Summary sentences render only inline Markdown code spans, emphasis and strong. Link labels
remain text; links stay marks, so navigation remains with the citations. Sentences with nothing
to open take no hover treatment.

### Opening

Opening a level 0 sentence replaces it, in place and in the same paragraph, with the level 1
sentences it cites. They are set in the same type and flow as running text; each carries its own
single mark, so a run that draws on two documents shows two colours in sequence. The order is
the order the overview sentence cites them in (section 04's data).

Opening a level 1 sentence replaces it with its source passages. Each passage flows with the
opened run after its file-and-heading chip, which opens file mode at that heading. Several
passages follow in citation order. Passages render with the same Markdown renderer file mode
uses (section 06); lists, tables and code keep their Markdown layout. No card, coloured left
border or spine marks the passage (`conventions/web-dashboard-typography.md`, "Stage thread
chips"). A passage never shows a file's YAML front matter (the `---` block at the top of a
summary or a knowledge topic): a file's opening passage starts after that block. File mode
shows the whole file, front matter included. A passage of a plan ends where the plan's
`<!-- loom METADATA -->` block begins, so the block never shows in an opened sentence; the plan page
draws it as the Stages panel.

A passage that an earlier fragment in the same view already shows appears as one line: its chip,
then a **Same passage as above** button. The button moves focus to the first showing, scrolling
it into view. The first showing in reading order keeps the full text.

Opened text sits on a 10% tint of its document's colour; a nested run uses its own document's
colour, so the reader can see what has been opened.
The page scrolls only when the opened text would push the opened point off screen; otherwise
nothing moves above it.

### Folding back

- **Fold caps.** Every opened run has an inline cap at both ends: a 1.3em disc in its document's
  colour holding a 0.78em `ChevronLeft` at the start or `ChevronRight` at the end, at stroke 2.5.
  Either cap folds the run back to the sentence it replaced. Both have the accessible name
  "Fold" and tooltip "Fold · Esc". Hovering or focusing a cap grows it 12% and deepens its run
  to a 22% tint with a hairline outline. Nested runs have their own caps; no gutter bracket
  marks a run.
- **Keyboard.** `Esc` folds the innermost run containing focus, or else the run opened last,
  and moves focus to the restored sentence. `←` folds only the innermost run around focus.
- **Fold all.** `Shift+Esc` or a button beside the document key folds every open run on the page.

There is no "open all"; a reader who wants a whole document summary opens its page. (My call:
opening everything rebuilds the full text in the overview's place, which concept mode exists to
avoid.)

### Keyboard

The reading column is one tab stop, with a roving focus over its sentences:

| Key                 | Action                                                                    |
| ------------------- | ------------------------------------------------------------------------- |
| `↓` / `j`, `↑` / `k` | Next / previous sentence in reading order, entering opened runs            |
| `Enter` / `→`       | Open the focused sentence                                                 |
| `Esc`               | Fold the innermost run around focus, or else the run opened last            |
| `←`                 | Fold the innermost run around focus                                        |
| `Shift+Esc`         | Fold every open run                                                        |
| `f`                 | Open file mode at the focused sentence's first source section               |
| `d`                 | Open the summary page of the focused sentence's first document              |
| `a`                 | Attach the focused sentence (or passage) to the next chat message           |
| `g p` / `g e`       | Go to the current feature's product overview / metaplan                  |

`Tab` leaves the column for the chat panel. The keys join the help dialog the `?` shortcut opens
(`web/src/routes/shell.tsx:26-31`), set with the existing `Kbd` component. Keys are ignored while
the user is typing in an input, as `shell.tsx` already does with `isTyping`.

### View state

Which runs are open is view state of one browser tab: a jotai atom per page, mirrored to
`sessionStorage` so a reload keeps it. It never reaches the server and is never shared with other
viewers. (My call.) The URL carries only the page (for example `/f/feat-planner/product` or
`/f/feat-planner/engineering/redesign`), so a copied link opens the page folded.

Concept page keys are `overview/<f>/product`, `<f>/product/<doc>`,
`<f>/engineering/<slug>` and `plans/<slug>` (`loom/src/plan_server/concept/page_key.rs`). Here
`<f>` is the feature name and `<doc>` the section stem; each page keeps its own view state.

## Rebuilds while reading

Summaries change under the reader when a refinement or an outside edit rebuilds them (vision,
Decisions, "Refinement"). The view updates where it stands:

1. **Out of date.** When a source file changes, the marks of every sentence built on it turn
   dashed at once, and the key chip for that document turns dashed. The text does not change yet.
2. **Arrival.** When the rebuilt summary arrives, changed sentences replace the old ones and
   flash once: a `--primary` wash fading over about one second. With `prefers-reduced-motion`,
   the wash shows without animation and clears after three seconds, matching the dashboard's
   reduced-motion rules (`web/src/graph.css:365`). Unchanged sentences do not flash.
3. **Open passages.** A source passage re-renders as soon as its file changes, before any
   summary rebuild, and flashes the same way.
4. **Position.** Content changing above the viewport never moves the text the reader is looking
   at: the column anchors on the focused sentence, or else the first visible one.

### What stays open

Open state is keyed to source sections (path plus heading anchor, section 03). Every sentence
resolves, through its citations, to the set of source sections it rests on.

- An open **passage** stays open across any rebuild while its source section exists.
- An open **level 0 or level 1 run** stays open if, after the rebuild, a sentence at the same
  level rests on any of the source sections the old one did; the new sentence with the largest
  overlap opens in its place (first in reading order on a tie). (My call: sentence text and
  identity change on every rebuild; the sources do not.)
- When no sentence qualifies, or a passage's section was deleted, the run folds back to its
  parent sentence and a muted note replaces the run's position: "Section *Port* was removed from
  `section_02.md`" or "This summary was rewritten and no longer draws on *Port*". The note has
  a link to the file in file mode and stays until the reader opens or folds something on that
  paragraph. A renamed heading counts as a deletion and an addition, per section 03's anchors.

A screen reader hears one polite live-region announcement per arrival, naming the page and the
number of sentences updated, at most once every ten seconds. (My call.)

## Product to plan links

Links between each feature's product sections and plans come only from `Implements:` lines in plans (vision, Decisions,
"Product-to-plan links"; the line format is section 03's). Concept mode never infers a link from
text.

- A **plan's** summary page header and its key chip tooltip list "Implements:" with a link to
  each named product section's summary page.
- A **product section's** summary page header lists "Implemented by:" with each plan naming it,
  with its plan state. In the product key, a section that no plan implements is marked "no plan"
  in muted text, shown once a metaplan exists.
- **Inside an overview**, a level 1 run from a product section ends its first sentence's marks
  with a small plan glyph when the section has implementing plans; the glyph's popover lists them
  and each opens the plan's summary page under the engineering route. Plan pages link back to their implementing sections. (My call: the link appears where the reader has drilled into one document, so
  level 0 prose stays uncluttered.)
- **Edge chips.** On a product page, a sentence also shows a small chip for each entry, plan and
  stage covering the headings it rests on (a whole-section source counts every heading of that
  section), and a count of the open findings on those headings. On a plan page, a sentence
  citing a stage (or the plan) shows a chip for each spec heading that stage implements, with
  the settle state of the heading's section. The chips are computed in the browser from the
  sentence's sources and the feature's trace; they sit beside the plan glyph, which stays.
- An `Implements:` line naming a section that does not exist shows on the plan page as a broken
  link with the name as written, so the reviewer sees it. It is a broken reference (section 03),
  never a checklist gap.
- A plan with no `Implements:` line shows "Implements: not stated".

Crossing between product and plan pages keeps each page's open runs; the back button
returns to the previous page.

## Selecting text for chat

Selecting text anywhere in the reading column, at any level, shows a small floating **Add to
chat** button by the selection; `a` does the same for the focused sentence. The attachment
carries the selected text and its provenance: for summary text, the sentence and the source
sections it rests on; for passage text, the path, heading anchor and line range. A selection
spanning levels carries each part's provenance. Section 08 renders attachments as chips in the
input box; section 10 defines the text delivered to the session and what the session does with
it.

When nobody holds authoring, any browser may attach passages and start a chat, taking authoring
as it sends (section 10, "Taking a passage into chat"). While another browser holds authoring, a
read-only browser has no chat input: it gets **Send to author** in place of Add to chat (section
10, "Read-only viewers"), `a` does nothing, and it can still select and copy.

## Empty and partial states

A page that is loading, not found or failed to load is one centred state, `LoadState`
(`web/src/plan/ui/load-state.tsx`): a muted disc holding an icon (a 40px icon in a 72px disc on
a page, 24px in 44px when compact), a title, an optional detail and an optional action. It is
centred on both axes in its container. Loading shows the ring spinner and fades in after 160ms,
so a load that answers at once never flashes. The partial states below are a quiet sentence in
the page with the offer under it. The states below apply to the selected feature. Offers in them are section 01's; read-only
browsers see the same text without the offer button.

| State                                                   | Product page                                                                                       | Metaplan page                                                                                    |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| No vision, no sections                                  | "No product vision yet", offer to start one                                                        | as below                                                                                          |
| Sections, no vision (section 01)                        | No overview; list of section summaries in file-name order                                          | as below                                                                                          |
| No metaplan, no plans                                   | as above                                                                                           | "No metaplan yet", offer to start one from the spec                                               |
| Plans, no metaplan                                      | as above                                                                                           | List of plan summaries in file-name order, offer to start a metaplan                 |
| Documents exist, summaries never built                  | "Summaries not built", the count, and the cost line section 04 provides; documents listed with file mode links | same                                                                                 |
| Summaries being built                                   | "Building 4 of 12"; document summaries appear in the key as they finish and open from it; the overview appears when built | same                                                       |
| Rebuild above the budget waiting for approval           | Overview readable with dashed marks; a notice in the key row with the stale count and, for the author, an approve button | same                                                        |

An overview never renders half-built: it shows its last complete version with dashed marks, or the
building state when there is none. (My call.)

## Several viewers

Read-only browsers see concept mode exactly as the author does: same overviews, same marks,
same rebuild flashes. Each browser opens and folds on its own. The presence list in the header
(section 02) shows, for each browser in concept mode, the page it is on ("Product overview",
"section_04 summary"). There is no follow mode. (My call.) Approving an above-budget rebuild is
an authoring act, available only to the browser holding authoring.

## Visual design

Concept mode follows the dashboard's conventions:

- **Type.** Body text uses Source Serif 4 through `--font-reading`, with Literata and Piazzolla
  available in Settings. Reading size is 17, 19 (default) or 21px, with a 1.6 line height,
  because it is prose to read. (My call.) Inter sets every title, document heading and chrome
  element. Commit Mono Cosmix sets code, paths, ids, slugs, heading anchors and key numbers.
  The column label above the key uses the existing `eyebrow` utility
  (`web/src/index.css:125`); no new all-caps chips.
- **Colour.** Surfaces from `tokens.css`, dark mode through the `.dark` class, document colours
  from the new `--doc-*` tokens, flash from `--primary`, focus ring from `--ring`. The planner
  has seven themes: Ledger and Harbor (light); Aubergine, Graphite, Midnight, Obsidian and Indigo Night (dark). Light or dark starts from the
  desktop's setting, and the dark theme is Graphite until the reader picks another in Settings.
- **Components.** Tooltips and `Kbd` reused from `web/`, and `LoadState` for the loading,
  empty, not-found and error states.
- **Narrow screens.** Below the `lg` breakpoint the column takes the full width with a 16px
  gutter; fold caps stay inline and marks keep their size. The chat panel's
  narrow layout is section 08's.

## Accessibility

- Every expandable sentence is `role="button"` with `aria-expanded` and `aria-controls` naming
  its run; a run is `role="group"` labelled "Detail of" plus the sentence's first words.
- Marks have `aria-label`s naming their documents and their out-of-date state.
- The roving focus shows the `--ring` outline; focus never lands inside a folded run.
- The column width is in `ch`, so browser zoom to 200% reflows without horizontal scrolling.
- Flashes respect `prefers-reduced-motion` (above); colour is never the only signal.

## What concept mode needs from the summary data

Section 04 owns the format. Concept mode needs, per sentence: a stable id within its summary, its
text, its citations (level 0: the level 1 sentence ids, or source anchors for the vision and
metaplan; level 1: source anchors), and per summary: its document, whether it is up to date, and
why not. Without per-sentence citations neither opening nor the open-state rule above works.

## Out of scope for this section

- Document paths, heading anchors and the `Implements:` line format: section 03.
- Summary formats, the manifest, rebuild triggers, the budget and cost display: section 04.
- File mode, the Markdown renderer and the editor: section 06.
- The chat panel, attachment rendering and the presence list's chat side: sections 07 and 08.
- What happens to a passage taken into chat, and showing what a refinement changed: section 10.
