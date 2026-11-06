# Planning trace UI: the approved design

Every worker reads this file in full. It describes the screens the user approved, the round-3 mockups. The mockups' HTML is lost. Their Firefox screenshots are the visual target, in
`doc/plans/mockups/trace-screens-redesign/`:
- `<screen>-<theme>-1440.png` in the Ledger (light) and Walnut (dark) themes
- `phone-<screen>-ledger.png` at 390px

Where this text and a screenshot differ, this text wins. It records the user's one change after
review, plus the cases the screenshots leave open.

The data model, the routes and the Settled line are unchanged from `PLAN-planning-trace`. Its
common brief, `doc/plans/briefs/planning-trace/common.md`, still describes them: read "Settled
marks", "Findings ledger", "Trace model" and "Routes". Its section "The browser" is superseded by
this file.

## The user's change after review: titles in Inter

No title or heading uses a serif. These are set in Inter (`--font-sans`, or `--font-heading`, which
resolves to it):
- page titles (the 40px document title with its muted number)
- the headings in a document's summary and source
- Coverage section headers and heading rows
- Findings group headers and finding titles
- card and panel headings
- the spine

The reading face (`--font-reading`, chosen per user in Settings; the default is Source Serif 4) is
for running prose only:
- the scope standfirst
- summary and source paragraphs
- quotes (vision row, stub, passage)
- finding text
- brief descriptions
- the spine's next-step sentence

The screenshots use Literata everywhere the reading face appears, and serif headings. Implement
Inter headings and `--font-reading` prose. Never hard-code Literata.

Most titles are already Inter: `.plan-page-title` is Inter 20px/650 (`ui/design.css`). The
screenshots set feature document titles much larger (about 40px), Coverage section headers at about
24px and finding titles at about 22px, and the user approved that scale.
- Add one step to DESIGN.md Type for feature document pages: the summary and source headings of
  feature documents, Coverage and Findings.
- Apply it there only. Other screens keep `--type-page`.
- Inter at these sizes needs weight 600 and tighter tracking (about -0.02em).
- Record the sizes you choose in `web/src/plan/ui/DESIGN.md` (Type).

## Screens and states (screenshot names in brackets)

### Spine (every feature page) [doc-settled, phone-spine]

- **Groups:** Vision, Product spec ("n of m settled"), Metaplan, Plans ("n done"), Run ("n running").
  There are no Coverage or Findings rows. Remove the rows `trace-web-maps` added.
- **One glyph per row, at the row's start:**
  - settled: a green check-seal
  - draft: an empty seal
  - not written: a dashed circle
  - done plan: a green check
  - running plan: the aurora activity roundel, the same component and size the app bar's
    "n working" indicator uses (`BusyRoundel`, as `p/header/activity.tsx` draws it), in place of
    the `LiveDot`. DESIGN.md "Status sign" currently says the roundel is drawn only in the header
    pill; rewrite that rule to allow the spine's running plan row, and nothing else.
  - pressure-tested plan: no flask in the spine (the flask moves to the plan page)
- **Attention turns the row's glyph amber.** A document whose settle state is `revised`,
  `stub_changed` or `upstream_changed` gets an amber seal with an exclamation. No extra dot, chip
  or badge is added. The state word is in the row's `title` and in `.sr-only` text, so the
  accessible name still says "Settled", "Draft", "Changed since you settled it" and so on.
- **Current page:** the row's muted highlight at weight 600. No dot.
- **Plan slugs** drop the feature prefix inside that feature's spine: `feat-planner-05-trace` shows
  as `05-trace`, with the full slug in `title`. The rule is: strip `<feature>-` when the slug starts
  with it, else show the slug whole.
- **The head** keeps the next-step sentence (reading face) and one primary button, naming the top
  attention item. Examples:
  - "Section 04 changed since you settled it." with the button "Review section 04", plus a muted
    second line when a plan is flagged with it ("Plan 05-trace implements it, so it is flagged
    too.")
  - "Three findings from the last run are waiting for you." with "Triage findings"
- **The frontier hints list is removed.** Only the top frontier item reaches the head, as a new
  `SpineHead` fed by the trace frontier. With an empty frontier, `SpineHead` renders nothing, and
  whatever the merged tree draws at the spine head otherwise stays as it is.
- **Entry points:** Coverage and Findings are reached from:
  - the spine head
  - the heading's coverage chip
  - the opened passage's "Covered by" line
  - a "Coverage" link on the Product spec overview page (beside its gaps block)
  - a "Findings · n open" link on the Run page
- **Phone:** the spine sits behind the product's existing small-screen menu bar ("☰ feat-planner ·
  <here> ⌄") and opens as a full-height overlay.

### Document heading (summary pages and the feature source page) [doc-settled, doc-changed, doc-confirm, doc-settled-menu]

- **Order:**
  - breadcrumb, with Summary | Source on the right
  - the title: the muted number plus the name, in Inter
  - the scope standfirst
  - the chip row
  - the action row
  - a hairline
- **Scope standfirst:** the stub text (the vision row, or the metaplan entry's first paragraph), in
  the reading face at about 21px. It ends in a small muted "from the vision ↗" or "from the metaplan
  ↗" link to the stub's place. No disclosure and no collapse. This replaces `StubPanel`'s boxed
  panel. When the stub changed since settling, the standfirst carries the amber note "The vision
  row changed since you settled this" with a See what changed link.
- **Chip row:** the summary freshness ("Summary up to date · Rebuild", unchanged), the settle state as
  glyph plus word ("Settled 2 Nov", "Changed since you settled it", or "Spec changed since you
  settled it" on a plan), and the coverage chip ("3 of 4 headings covered", linking to Coverage at
  this section). The chip row stays on one line at 1440px.
- **Action row:** Edit · Discuss · the settle control, all real 32px buttons in one style.
  - Discuss attaches the whole document to the chat and opens the chat panel (the existing
    behaviour; keep it).
  - The settle control is a button whose label is the state:
    - draft: "Mark settled", which posts at once (or opens the confirm)
    - settled: "Settled 2 Nov ⌄", a menu holding Unsettle with a short note
    - changed: "Changed since you settled it ⌄", a menu holding Settle this version and
      Unsettle, plus a separate "See what changed" button
    - plan upstream: the button reads "Spec changed since you settled this plan", with "See what
      changed" beside it
    - locked: the state word, no buttons
- **Settle confirm** (the server answers `unresolved`):
  - a small popover anchored under Mark settled, on the existing popover or dialog primitive
  - its role is `dialog`
  - title: "Settle with open items?"
  - shows the counts in words ("1 My call", "2 open questions"; "My call" matches the document
    marker)
  - lists each My call item and each open question as a short quote with where it is (the
    heading)
  - buttons: "Settle anyway" (primary) and "Not yet"
  - confirming resends with `confirm=1`
  - the counts come from the 409 body; the quotes come from the document text the page already
    holds (lines with `(My call.)` and the Open Questions section). When the text is not loaded,
    show the counts alone.
- **Phone:** the action row wraps; menus and the confirm rise as bottom sheets.
- **"See what changed"** opens the review module's commit comparison at the settled commit (the
  existing `Changes since settled` behaviour). The link name changes to "See what changed" everywhere.

### Plan page (a plan's summary page) [doc-plan, doc-plan-briefs]

Below the heading, before the Summary:
1. **Pressure record card:** the flask, the last pressure-test date and its verdict, and a link to the
   record. It shows only when the plan has a record. Read how the spine knows a plan was
   pressure-tested today and reuse that source.
2. **What it implements:** each spec heading the plan implements, with its section number and the
   heading's own settle glyph, linked to the heading.
3. **Briefs:** every brief in the bundle (`bundleLinks`), in a two-column list grouped by the brief's
   directory (the stage). Each shows its mono file name, linked to the feature source page for that
   file. Show no descriptions: the trace does not carry brief text, and fetching every brief is not
   worth it. This replaces the brief list inside `StubPanel`.

### Opened sentence [doc-opened]

Reading text carries no marks at rest. Remove the `EdgeChips` that render after each sentence.

When a sentence is opened, its passage block (the existing one, with the 2px document-hue edge, the
source line and the quote) gains one line: "Covered by 03-summaries › fragments · 1 open
finding".
- The plan and stage names link to the plan.
- The finding count links to Findings.
- With no plan covering it: "Not covered by any plan", linking to Coverage at that heading.

### Chat after Discuss [doc-changed, phone-doc-changed]

The Hearth chat, unchanged, except the turn footer:
- the changed-file chip (`section_04.md +4 −2`, with Diff and Restore)
- then "Now out of step with what you settled", listing each affected settled document with its
  amber glyph and title, linked

This replaces `ImpactRows`' current look. The composer's context chip ("About: Section 04") is
existing behaviour. On a phone the chat is the existing sheet.

### Coverage page `/f/<feature>/coverage` [coverage, coverage-s04, phone-coverage]

- **Header:** the title "Coverage", a standfirst in the reading face ("Which parts of the spec are
  covered by planned work, and what is uncovered or changed."), then four count tiles that are also
  the filter:
  - All headings
  - Not covered
  - Changed since covered
  - With open findings

  The chosen tile is filled and underlined in the accent. The filter is real, and the tiles are
  buttons with `aria-pressed`.
- **Body:** the spec grouped by section.
  - Each section header carries: the muted number, the title (Inter, about 24px), the section's settle
    state as glyph plus word, and "n of m covered" on the right.
  - A section that is not written shows as one quiet line.
- **Heading rows** share one CSS grid with a column header row (Heading · Covered by · Findings ·
  State), so the columns align. The cells are:
  - Covered by: `<plan> › <stage>` pairs, one per line, or "Nothing yet" in italic muted
  - Findings: "n open", or "–"
  - State, in words with a glyph: Built, Planned, Changed since covered (amber) or Not covered
    (dashed)
- **Every row expands** (chevron, `aria-expanded`) to three columns:
  - Why: the vision row quote, with links to the vision row and the section
  - How: one Entry / Plan / Stage block per covering chain, with each plan's state
  - Findings: the open findings about the heading, or "None open."

  A "Changed since covered" row also shows an amber note naming when it changed, with See what
  changed.
- **Phone:** the tiles go 2×2 and the row cells stack.
- **Drop:** the lane/focus-panel layout and the h/l focus keys of `trace-web-maps`. Up and down
  arrows plus Enter on rows are enough. Keyboard access to every row and its expansion is
  required.

### Findings page `/f/<feature>/findings` [findings, findings-run03, phone-findings]

- **Header:**
  - the title "Findings"
  - a standfirst in the reading face
  - a one-line tally ("3 open · 2 planned · 1 folded · 1 dismissed")
  - a "Review the feature" button (the existing `review_feature` offer)
- **Groups:** one per run review, newest first, headed "Run of 03-summaries" (the plan slug with
  the feature prefix stripped). Then "Reviews" for findings from Review the feature. Newest means the
  run whose first finding comes last in the ledger, because the import appends; the `Source:` line
  carries no date, so show none.
- **An open finding** is a soft card with no edge. It shows:
  - the id and status
  - the title, in Inter at about 22px
  - About pills linking the headings
  - the text in the reading face, as plain text, never Markdown or HTML
  - then Plan this · Fold into entry · Dismiss
- **Fold into entry** opens an inline picker: radio rows of the metaplan entries whose plan has not
  started, the suggested entry first, a note naming what is left out (entries already running or
  done), and a confirm button naming the chosen entry.
- **Dismiss** opens an inline reason field. Its confirm is disabled, with "Write a reason first",
  until text is typed.
- **Plan this** keeps the behaviour of `trace-web-maps`: it runs the plan-follow-ups offer seeded
  with the finding.
- **Decided findings** (planned, folded, dismissed) fold to one line each, with the outcome in words.
- **Phone:** the action buttons wrap to full width.

## Look

- **Tokens:** the existing ones (`aurora-ui/shared/styles/tokens.css`, `web/src/index.css`, the
  document hues, the tones). Amber attention is the existing warning tone, and green settled is the
  existing success tone. No new colour literals.
- **Sizes:**
  - content type at reading sizes: body 17 to 18px in the reading face, row headings 17 to 18px
    in Inter
  - nothing in content below 13px
  - mono only for ids, slugs and file names
- **Spacing and surfaces:** generous rhythm, as the screenshots show. Hierarchy comes from type,
  space and hairlines, with no nested boxes.
- **Themes:** both Ledger and Walnut must hold. Check both.
- **Layout safety:** `min-width: 0` on flex and grid tracks, ellipsis where text can run long, no
  horizontal scroll at 390px.
- **Firefox is the user's browser.** Every visual check is made in Firefox, at 1440 and at 390, in
  both themes. Chromium shots alone do not count.
