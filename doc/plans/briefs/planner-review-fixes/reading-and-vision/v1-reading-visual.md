# V1 — sources key, the Summary/Source switch, the source view in the feature, the vision page (issues 3, 4, 6)

Tier: opus, effort medium (`loom-senior-software-engineer`). First invoke `frontend-design:frontend-design` with the Skill tool. Then read `common.md` (Design rules, Visual work), `web/src/plan/ui/DESIGN.md`, and this brief.

## Sources key (issue 4, D5)

`web/src/plan/concept/document-key.tsx` (`ChipLink`, `ChipMark`, `shortLabel`, `DocumentKey`) and `key-chip-extras.tsx`.

- Today a chip draws a kind glyph on a 16% wash, a mono number (`§05`, the server's `chip_label`), and the title only in an sr-only span plus a tooltip. A separate `●` dot (`KeyChipExtras`) means "changed since you looked".
- Fixed targets:
  - Each chip shows its document's title visibly, beside the glyph and number, truncated with an ellipsis at a fixed chip width by CSS only (`text-overflow: ellipsis`): the DOM holds the full `doc.title`, which the stage contract `key-chip-shows-its-title` reads. At 390px the key wraps to at most two lines before "… n more".
  - "Changed" is a word in `--type-fine`, not a dot. Keep its accessible name "changed since you looked".
  - Frozen contracts hold:
    - every chip is a link whose accessible name contains the title;
    - four chips, then a "… n more" button (`concept/reading.contracts.test.tsx` expects `/^…\s?8 more$/`);
    - `data-dashed` on an out-of-date chip;
    - `getAllByRole("link")` over the key equals the chip count, so the "changed" word sits outside the link, as the dot does today.
  - The key carries a one-line lede in `--type-fine`, for example "Sources: the 12 sections this overview summarizes".

## Summary / Source switch and the source view (issue 6, D4)

R3 adds the route `/f/<f>/source/*` and points the links at it. You own the look.

- `web/src/plan/ui/doc-heading.tsx` `ViewSwitch` (125-155) and `doc-heading.css`: a clear two-state segmented control labelled Summary and Source, at one height with the heading's other actions.
  - The current state is `aria-current` and is visible in both themes. It stays a pair of links named exactly "Summary" and "Source" (journeys J2 and J3 click them by role link).
  - It reads as the obvious way back. A reader on a source page never hunts for it.
- `web/src/plan/feature/spine.tsx` (and its CSS): on a source route the document's row is current and shows "Source" as a quiet sub-line under it, so the sidebar keeps the connection the operator lost. R3's `spine-model.ts` marks the row current; your `spine.tsx` reads `useLocation()` itself to decide whether to draw the sub-line (a pathname containing `/source/`).
- The source page inside the feature uses `SourcePage`, with the column width and heading inset of the concept pages (`--doc-heading-inset`). Check it at 1440px and 390px.

## Vision page (issue 3)

`/f/<f>/vision` is now a summary page (R1, R3), with at most 8 sentences in at most 3 paragraphs. Make it read as a short pitch:

- the reading face at its DESIGN.md size;
- the sentences set as paragraphs, not a list;
- the page heading and Source switch above;
- a visible "Read the full vision" link to the source route at the end.

No new tokens.

## Files

Yours are listed in the plan YAML worker table. You never edit R3's routing and link files: `router.tsx`, `landing.ts`, `page-key.ts`, `passage.tsx`, `page-heading.tsx`, `document-page.tsx`, `source-href.ts`, `spine-model.ts`. You never edit R1's or R2's Rust.

Add each deliberate exception you make to DESIGN.md's Surfaces section.

## Run once at the end

`node_modules/.bin/vitest run src/plan/concept src/plan/ui src/plan/feature src/plan/help` from `web/`.
