# V1: the reading surface (#2, #8, #9, #10)

Tier: opus at effort medium (`loom-senior-software-engineer`, spawned with `effort: medium`). This is visual work.

**First action, before reading code:** invoke the `frontend-design:frontend-design` skill with the Skill tool. Then read `../common.md` (its "Design direction" section is the brief for the skill) and `web/src/plan/ui/DESIGN.md`. Where the skill's general advice and DESIGN.md disagree, DESIGN.md wins.

The values below are starting points that satisfy DESIGN.md. Refine them where the result reads better, within DESIGN.md. The operator-fixed requirements are marked **fixed** and do not move.

## #2: fold caps and the left rule of an open block

Current state:

- `web/src/plan/concept/fold-handle.tsx` `FoldCap` is a `<button class="concept-run-cap">` holding lucide `ChevronLeft` or `ChevronRight` (`strokeWidth={2.5}`, `concept-run-cap-icon`).
- `concept/concept-runs.css`:
  - `.concept-run-cap` (~L57-72): `1.3em` square, transparent.
  - The icon (~L74-77): `0.78em`.
  - Hover: `--muted`.
  - The open block's edge `.concept-passages` (~L42-48): `margin-inline-start: -12px; padding-inline-start: 11px; border-inline-start: 1px solid var(--run-edge)`. `--run-edge` is `--hairline` and turns the run's hue only while one of its caps is hovered.
- `--run-colour` is set inline in `concept/run.tsx` (O2's file), on the inline run and again on the `PassageRun` element that carries `.concept-passages`. The edge therefore needs CSS only; never edit `run.tsx`.

Target:

- **Fixed:** an open block always shows a clearly visible edge in its document's hue. Starting value: `border-inline-start: 2px solid var(--run-colour); padding-inline-start: 10px`, which keeps the text where it is.
- **Fixed:** the caps are clearly visible at rest. Starting values:
  - cap `1.5em` square on `background: var(--muted)`, `background: var(--border)` on hover or focus;
  - icon `1em`.
- Rewrite the file's header comment (~L1-12) to the new rule. Delete the sentence that calls the quiet edge deliberate.
- Inline runs keep their hover recolouring.
- Keep the class `concept-run-cap`, its `data-side` attribute, its "Fold" accessible name, `.concept-passage` and `.concept-mark[data-dashed]`. Help scenes (`help/reading-scene.tsx`, `help/legend-entries.tsx`, `help/legend.test.tsx`) and the browser journeys (`web/scripts/plan-journeys.mjs`) select by them.
- Frozen contract (stage's reading-fixes contracts): the `.concept-passages` rule declares exactly `border-inline-start: 2px solid var(--run-colour)`.

## #8: Open all becomes Fold all

- Today `OpenAllButton` (`concept/page-heading.tsx`) is a static "Open all" (`UnfoldVertical`), drawn when `!page.empty`. `useOpenAll` (`concept/open-all.ts`) opens every closed openable sentence through `openRunsAtom(pageKey)` (defined in `concept/view-state.ts`). `closedSites` (`open-all.ts`) knows what is still closed. `useOpenAll` reads its atoms through `store.get` inside the callback, keyed by the route pathname.
- **Fixed:**
  - While any openable sentence is closed, the button reads **Open all** (`UnfoldVertical`).
  - Once none is closed, it reads **Fold all** (`FoldVertical`).
  - Fold all clears the page's open runs at every level (an overview's nested member runs included), keeps focus on the button, and the label returns to Open all.
  - Opening or folding sentences by hand updates the label.
  - A page with no openable sentence draws no button. This changes the gate from `!page.empty` to "the page has at least one openable sentence" (`canOpen` over `pageSentences`); update `page-heading-actions.test.tsx`'s "no Open all over an empty page" and "keeps Open all" cases for it.
- Add `useAllOpen(page): boolean` and `useFoldAll(page)` to `open-all.ts`. Derive both from the `page` prop `PageHeading` already receives plus `useAtomValue(openRunsAtom(page.key))`, never from the pathname, so the label is right while `conceptPageAtom` still holds the previous page during navigation. `page-heading.tsx` calls `useAllOpen(` and `useFoldAll(` (the plan's wiring checks grep both).
- `open-state.ts` belongs to O2: import from it, never edit it. `view-state.ts` is in neither worker's list and needs no edit.
- `concept-route.test.tsx`'s "the key has no Fold all button" still passes; rename it if its name now misleads (V1 owns that file).
- Frozen contract: "Open all becomes Fold all and folds the page" (the plan's stage YAML describes its summary and overview steps).

## #10: header controls aligned

- `ui/doc-heading.tsx` `TitleRow`: the title cluster sits left, and the actions sit right (`ml-auto ... gap-2`). `ViewSwitch` (Summary and Source as `span`/`Link` `.plan-button`) sits beside Edit, Discuss and Open all, which are shadcn `Button`s styled by `HEADING_BUTTON`. `HEADING_BUTTON` and `ViewSwitch` already set `data-size="sm"` and `data-look="quiet"`, so the misalignment is not a size mismatch: the shadcn base adds `h-7 px-2.5 rounded-lg`, which fights `.plan-button` (`ui/design.css`, `.plan-button` and its `data-size="sm"`, `data-look="quiet"`, `data-look="secondary"` rules).
- **Fixed:** every control in the row has one height, one corner and one centre line, level with the title's first line. One gap value separates the controls.
- **Fixed:** Source, Summary and Edit stay links (role link, exact names "Source", "Summary", "Edit"), and a disabled Summary stays a `span` with `aria-disabled="true"`. Restyle them with `.plan-button`; never swap a link for a `<button>`. `web/scripts/plan-journeys.mjs` clicks them by role, and `files/doc-heading-view.test.tsx` pins `aria-current` and `aria-disabled` on them.
- Starting values:
  - all five controls `.plan-button` with `data-size="sm"`; if the row uses `sm`, add "the document heading's action row" to DESIGN.md's Buttons sentence that names where `sm` is used;
  - Summary and Source joined as a segmented pair, the current one marked by the `aria-current` rule in `ui/doc-heading.css`;
  - Edit, Discuss and Open all as `data-look="quiet"` buttons;
  - a plain `<button className="plan-button">` only for a control that is a button today (Discuss, Open all, Fold all) wherever the shadcn `Button` brings conflicting utilities.
- When the row wraps (≤900px), the actions start at the title's left edge.
- `files/doc-heading-view.tsx` (the source view's header) gets the same alignment.
- `HEADING_BUTTON` is imported by `files/doc-heading-kit.tsx` and `files/plan/stages-panel.tsx`; if you rename or remove it, update both (you own them).

## #9: the document key and the Rebuild control

Current state:

- `concept/document-key.tsx` `DocumentKey` lists chips with mark, label and full title, a "N more" chip after `VISIBLE_CHIPS` (4), a legend paragraph (`p.plan-fine.concept-key-legend`), and `KeyRowExtras`. `KeyRowExtras` (`concept/key-row-extras.tsx`) returns the `PendingApprovalNotice` (`summaries/approval-notice.tsx`), with "Rebuild all" in a `section.w-full.space-y-2`. CSS is in `concept/concept-page.css:8-125`.
- The header's summary status comes from `files/heading-parts.tsx` (`summaryChips`, `summaryLabel`). Below it, `concept/page-header-extras.tsx` stacks `PlanRunActions`, `BuiltMarker` and `summaries/rebuild-button.tsx` in a `flex-col`. `RebuildButton` wraps `ui/disabled-reason.tsx`, another `flex-col`.

Target:

- **Fixed:** the key is one quiet row of at most two lines at 1440px. Full document titles are not drawn inline: they go in each chip's accessible name and `title`.
- Starting design: a `.plan-fine` lead word "Sources", then the mark plus a short label per document (`§05`, a plan slug), then "+n more". The mark legend is one `.plan-fine` line, drawn only when the page has marks.
- **Fixed by frozen contracts from earlier plans** (`concept/concept.contracts.test.tsx`, `concept/reading.contracts.test.tsx`; never edit them): each document chip stays a link whose accessible name still matches its document title (`getByRole("link", { name: /Review/ })`), and the number of links in the key equals `key_chips.length`, so "+n more" is not a link.
- **Fixed:** the summary status (glyph and word, DESIGN.md "Status") and its action ("Rebuild", or "Rebuild all" while approval is pending) sit on one line, on one baseline. The approval notice leaves the key. A disabled reason shows inline after the button in `.plan-fine`, not stacked.
- Moving the notice out of the key: `KeyRowExtras` either renders nothing or is deleted with its mount. `concept-route.test.tsx` ("PageHeaderExtras, KeyChipExtras and KeyRowExtras mounted", and its `vi.mock("./key-row-extras")`) and `concept-route-headers.test.tsx`'s `vi.mock("./key-row-extras")` follow the choice. `summaries/rebuild-question.test.tsx` tests `PendingApprovalNotice` ("Rebuild all", "above the budget and waiting for approval"): keep those names and texts, or update that file.
- **`DisabledReason` is shared by about 40 files** (chat, feature, home, new, offers, run, editor, help). Do not change its default layout. Add an opt-in prop, `reasonPlacement?: "below" | "inline"` (default `"below"`), and pass `"inline"` only from `RebuildButton`. `disabled-reason.test.tsx` covers both placements. DESIGN.md's Buttons "Blocked" sentence ("the reason beneath in `.plan-fine`") gains one clause naming the inline placement in the document header.
- `PlanRunActions` and `BuiltMarker` stay in the header extras.

## DESIGN.md

Add the two exceptions from `../common.md` ("Deliberate exceptions") to the "Surfaces and borders" section, one bullet each, in current-state wording. That section says a border is not allowed around a button row, a list, a status, a heading or a notice; the #2 bullet names "an open block in the reading column" explicitly so the two rules do not read as a contradiction.

## Tests

- Keep every accessible name that tests and help scenes query, or update those tests in the same change. Help scenes carry selector contracts (`conventions/plan-web.md`, "UI Repair Test Rules").
- Update the tests that pin old cap values, header structure and key layout: `rg -n '0.78em|1.3em|run-edge|concept-key|Open all|HEADING_BUTTON' web/src/plan --glob '*.test.*'`.
- Do not weaken unrelated assertions.

## Files you own

- Concept: `web/src/plan/concept/concept-runs.css`, `fold-handle.tsx`, `fold-caps.test.tsx`, `fold-handle.test.tsx`, `doc-tokens.test.ts`, `page-heading.tsx`, `page-heading.test.tsx`, `page-heading-actions.test.tsx`, `concept-route-headers.test.tsx`, `concept-route.test.tsx`, `open-all.ts`, `open-all.test.tsx`, `document-key.tsx`, `document-key.test.tsx`, `key-row-extras.tsx`, `concept-page.css`, `page-header-extras.tsx`, `page-header-extras.test.tsx`.
- UI: `web/src/plan/ui/DESIGN.md`, `ui/doc-heading.tsx`, `ui/doc-heading.css`, `ui/doc-heading.test.tsx`, `ui/doc-heading-parts.test.tsx`, `ui/design.css`, `ui/disabled-reason.tsx`, `ui/disabled-reason.test.tsx`.
- Files: `web/src/plan/files/heading-parts.tsx`, `files/doc-heading-view.tsx`, `files/doc-heading-view.test.tsx`, `files/doc-heading-title.test.tsx`, `files/doc-heading-view-parts.test.tsx`, `files/doc-heading-kit.tsx`, `files/plan/plan-actions-extras.test.tsx`, `files/plan/plan-view.test.tsx`, `files/plan/stages-panel.tsx`.
- Summaries: `web/src/plan/summaries/approval-notice.tsx`, `summaries/rebuild-button.tsx`, `summaries/rebuild-button.test.tsx`, `summaries/rebuild-question.test.tsx`.
- Help: `web/src/plan/help/**`.

If `reading-column-runs.test.tsx` pins a value you change, report the line: O2 owns that file. A test outside these files that your change breaks (a `DisabledReason` or doc-heading consumer in `src/plan/chat`, `feature`, `home`, `new`, `offers` or `review`) is reported to the main agent with its line, never left failing: the stage gate runs the whole vitest suite.

## Contracts (frozen before you start; make them pass)

`web/src/plan/concept/reading-fixes.contracts.test.tsx`: "Open all becomes Fold all and folds the page" and "an open block draws a 2px edge in its document hue".

## Proof

Run once at the end: `node_modules/.bin/vitest run src/plan` from `web/`. You cannot build or take screenshots: the main agent builds, shoots, and sends a refinement round with the shots if anything misses. In your report, list each design decision you made beyond the starting values and why.
