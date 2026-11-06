# W2: document heading, plan page, opened passage and turn footer (trace-ui, wave 1)

Read `../design.md` in full: its "Document heading", "Plan page", "Opened sentence" and "Chat after
Discuss" sections are your specification. You write markup and behaviour. V1 sets the look after
you, so add class names and data attributes and leave CSS to it, apart from deleting rules for
markup you remove (in `trace.css`, coordinate through your report: V1 owns the file).

## Files you own

- `web/src/plan/trace/`: `settle-control.tsx`, `settle-flow.ts`, `settled-changes.tsx`,
  `stub-panel.tsx` (delete or rename to `scope-line.tsx`), `stub-text.ts`, `edge-chips.tsx`
  (delete), `impact-rows.tsx`, `plan-extras.tsx` (new), `passage-coverage.tsx` (new),
  `doc-model.ts` (new), `doc-model.test.ts` (new), `components.test.tsx`, `settle-flow.test.tsx`
- `web/src/plan/concept/page-heading.tsx`, `concept/sentence.tsx`, `concept/passage.tsx`
- `web/src/plan/feature/document-page.tsx`
- `web/src/plan/chat/turn-footer.tsx`

If the merged tree holds one of these seams in a differently named file, edit the file that holds
it and name it in your report.

## Tasks

1. **Settle control** (`settle-control.tsx`). The button's label is the state, as design.md lists:
   - Mark settled
   - Settled <d Mon> ⌄, with a menu holding Unsettle and a note
   - Changed since you settled it ⌄, with a menu holding Settle this version and Unsettle, plus a
     separate See what changed button
   - Spec changed since you settled this plan, plus See what changed
   - locked: the word only

   Build the menus on `web/src/plan/header/popover.tsx`. There is no menu primitive in
   `components/ui`.
   - The menu needs keyboard access: Escape closes it and focus returns to the trigger.
   - The settle date comes from `/api/settled-version`'s commit if the route gives one. Otherwise
     show "Settled" without a date. Do not add a wire field.
2. **Confirm.** Replace the centred `ConfirmDialog` with a popover anchored under the button.
   - It has `role="dialog"` and the title "Settle with open items?".
   - Show the counts in words ("1 My call", "2 open questions").
   - List the items as quotes from the opened document text: lines holding `(My call.)`, and the
     items under an Open Questions heading. Each quote says where it is.
   - Buttons: "Settle anyway" and "Not yet". Confirming resends with `confirm=1`.
   - The frozen contract "an unresolved answer asks before settling with confirm=1" must keep
     passing unchanged.
   - Below 760px the menus and the confirm render as bottom sheets: Radix `Dialog` with
     `data-sheet`, following the chat sheet's pattern in `chat/panel.tsx`.
3. **See what changed.** Rename the "Changes since settled" link to "See what changed" in
   `settled-changes.tsx`. Its behaviour is unchanged: the compare view at the settled commit, and
   "The settled version is not committed" when the commit is null.
4. **Scope standfirst** (`scope-line.tsx`, replacing `StubPanel`).
   - A `p.trace-scope` holding the stub text from `stub-text.ts`, then a small link "from the vision
     ↗" or "from the metaplan ↗" to the stub's place.
   - When the settle state is `stub_changed`, add the amber note "The vision row changed since you
     settled this" (or "The metaplan entry changed…") with See what changed.
   - Render it in the heading between the title and the chips. Read how `DocHeading`
     (`web/src/plan/ui/doc-heading.tsx`) places `children` and chips; if the standfirst cannot sit
     there, add a `standfirst` prop to `DocHeading` and report that edit for the orchestrator (you
     do not own `ui/doc-heading.tsx`).
5. **Chip row** (`page-heading.tsx`).
   - Add the settle-state chip (glyph plus word) to `pageChips`.
   - For sections, add the coverage chip "n of m headings covered" from the trace headings of the
     section, linking `/f/<feature>/coverage#<section anchor>`.
   - Move the actions to the order Edit · Discuss · settle control.
   - The pressure chip leaves the chip row on plan pages (the card below replaces it).
   - Remove the separate settle `ToolbarGroup`.
6. **Plan page** (`plan-extras.tsx`), mounted under the heading of a plan's summary page:
   - the Pressure record card, from `page.last_pressure`, shown only when it is non-null
   - What it implements: the trace headings whose `covered_by` holds a `plan` CoverRef for this
     plan's path, each with its section number, title and the section doc's settle glyph, linked
     to the heading
   - Briefs from `bundleLinks`, grouped by directory
7. **Feature source page** (`document-page.tsx` `SourceTrace`): the same settle control and scope
   standfirst, with no `StubPanel` box.
8. **No sentence chips.** Remove `<EdgeChips` from `concept/sentence.tsx` and delete
   `edge-chips.tsx`.
9. **Passage coverage** (`passage-coverage.tsx`, `doc-model.ts`).
   - `coverageOfHeading(trace, path, anchor)` returns the covering plans and stages, with short
     slugs, and the open finding count.
   - `PassageCoverage` renders one line after the passage body: "Covered by <plan> › <stage> · n
     open finding(s)", with the plan linked to its summary page and the count linked to Findings.
   - It renders "Not covered by any plan", linked to Coverage at the heading, when nothing covers
     the heading. It renders nothing when the trace has no such heading (non-spec files).
   - Mount it through `PassageBlock`'s `trailing` or after the body in `concept/passage.tsx`,
     keeping the existing props.
10. **Turn footer** (`impact-rows.tsx`, `turn-footer.tsx`): under each changed file, a block
    headed "Now out of step with what you settled", then one row per affected settled document,
    each with its glyph and linked title. Use `impactFor` as is.

## Tests

- `components.test.tsx`: each settle label and its menu items, the confirm popover's quotes and
  resend, See what changed, the scope standfirst and the stub-changed note, the plan page's three
  blocks, passage coverage (covered, not covered, absent), and the turn footer block. Split the file
  if it passes 400 lines.
- `doc-model.test.ts`: `coverageOfHeading`.
- Never edit a `*.contracts.test.tsx`. The orchestrator handles superseded contract tests.

## Done

`cd web && node_modules/.bin/vitest run src/plan/trace src/plan/concept src/plan/chat` passes,
apart from tests the orchestrator lists as superseded. tsc, oxlint and oxfmt are clean on your
files, and every file is under 400 lines.

Report:
- the class names and data attributes V1 must style
- the `trace.css` rules whose markup you removed
- any edit you need in a file you do not own
