# concept-web W3: pages, header, key, empty states, routes

Working dir `web/`. You own `src/plan/concept/{concept-route.tsx, concept-route.test.tsx,
page-header.tsx, page-header-extras.tsx, document-key.tsx, key-chip-extras.tsx, key-row-extras.tsx,
empty-states.tsx, empty-offer.tsx, chain.test.tsx}` and the concept lines in
`src/plan/router.tsx`, `src/plan/tabs.ts` and `src/plan/help-keys.ts` (plan 2's files; add entries
only). W1's
modules and W2's `ReadingColumn` exist; import them. The frozen contracts are
`src/plan/concept/concept.contracts.test.tsx`.

## Routes and tabs (`src/plan/router.tsx`, `src/plan/tabs.ts`)

Add `product`, `engineering`, `product/:doc`, `engineering/:slug` to plan 2's `routes` array
(children of the planning shell), each rendering `<ConceptRoute />` (the stage's wiring check
greps `ConceptRoute` in `router.tsx`). Add `{ id: "product", label: "Product", path: "/product" }`
and `{ id: "engineering", label: "Engineering", path: "/engineering" }` to `PLAN_TABS` before
plan 3's Files tab. Crossing tabs keeps each tab's open runs (view state is per page key), and
the back button works through the router.

## concept-route.tsx

Calls W1's `useConceptSubscription()`, reads the page key from the URL, loads it through W1's
page-state, and renders: a `Skeleton`
while loading, the empty state when `page.empty` is set, else the page header (summary pages) or
the column label (`eyebrow`, "Product overview"), the document key, `ReadingColumn`, and
`unlisted` below the overview ("Unlisted plans" / "Unlisted sections", each linking to its
summary page). Global keys `g p` and `g e` (two-key sequence within one second, ignored while
typing) navigate to the overviews. `a`: pass `ReadingColumn` an `onAttach` that writes the focused sentence's or passage's
`AttachmentDraft` to plan 3's `addAttachmentAtom` only while `canAuthorAtom` is true (otherwise
pass nothing). Selecting text in the column shows plan 3's floating `<AddToChat draft rect onAdded />` fed by
`useSelectionAttachment(columnRef, sel => attachmentFromSelection(sel, page))` (W1), which
returns `{ draft, rect, clear }`; plan 9 adds "Send to
author" for read-only browsers.

## page-header.tsx, page-header-extras.tsx

Title (`--font-display`), path (monospace), colour chip, "Up to date" or the out-of-date reason
in words, an "Open in file mode" link (`/files/<path>`). Plans: state (written, running, done,
retired), last pressure date when present, and for running, done and retired plans the locked
notice of section 06 ("This plan is running. `loom run` works from the committed copy." / "This
plan is done. Record changes in a new plan."). Plans: "Implements:" with links to
`/product/section_<nn>`, a broken one shown as written without a link, or "Implements: not
stated". Sections: "Implemented by:" with each plan and its state. Last child:
`<PageHeaderExtras page={page} />` from `page-header-extras.tsx`, which returns `null` (plan 9
fills it; keep its props exactly `{ page: ConceptPage }`).

## document-key.tsx, key-chip-extras.tsx

A `role="list"` labelled "Document key", one link chip per `key_chips` entry in order: colour
square, number or slug (monospace), short title; dashed chip (`data-dashed="true"`) when out of
date, with the reason in its tooltip; plan chips' tooltip lists "Implements:"; "no plan" in muted
text when set. Each chip links to its summary page. A "Fold all" button beside the key folds
every run on the page (clears W1's open runs for the page key). After the key row render
`<KeyRowExtras page={page} />` from `key-row-extras.tsx`, which returns `null` (plan 10 adds the
above-budget notice there; keep the props). Each chip ends with
`<KeyChipExtras chip={chip} />` returning `null` (plan 9 fills it).

## empty-states.tsx, empty-offer.tsx

One `EmptyState` (`web/src/aurora-ui/feedback/EmptyState.tsx`) per `EmptyState.kind` with section
05's texts: "No product vision yet", sections listed in file-name order, "No metaplan yet",
plans listed with "start a metaplan", "Summaries not built" with the count and the documents
linked to file mode, `BusyRoundel` with "Building <done> of <total>" when `building` is set
(always null in this plan; render "Building summaries" without numbers). Offers render through
`<EmptyOffer offer={id} />`, which shows the offer's label as a disabled button with the tooltip
"Available once chat is built" (plan 9 replaces its body; keep the props). Read-only browsers
see the text without the button.

## Help keys (`src/plan/help-keys.ts`)

Append a "Concept mode" group to `PLAN_HELP_KEYS` with section 05's key table rows (`↓ / j`, `↑ / k`, `Enter / →`,
`Escape / ←`, `f`, `d`, `a`, `g p`, `g e`), set with the existing `Kbd` component through plan
2's help rendering.

## chain.test.tsx (the metaplan's expansion-chain test)

Render the planning routes at `/product` with fetch answering from the concept fixtures: open
the first overview sentence, open its first level-1 sentence, assert the passage renders through
plan 3's renderer, click the passage header and assert the router location is
`/files/<path>` with the anchor. `concept-route.test.tsx`: tab switching keeps open runs, `g p`
/ `g e`, unlisted list, locked notice on a done plan, `PageHeaderExtras` and `KeyChipExtras`
mounted (spy through `vi.mock`).

## Required test cases (coverage audit; each a named test in `concept-route.test.tsx`)

"engineering tab renders and keeps runs"; "unlisted list links to summary pages"; "chip shows
colour, label and title"; "chip click navigates to summary page"; "header shows title, path,
colour, state and Open in file mode"; "plan header shows state and pressure date"; "locked
notice on a done plan"; "locked notice on a running plan"; "Fold all folds every run"; "a adds an
attachment while canAuthor"; "a does nothing while another browser holds authoring"; "g p and
g e"; "PLAN_HELP_KEYS holds the Concept mode group with nine rows"; "opening a run changes no URL
and sends no request"; "plan chip tooltip lists Implements"; "Implemented by with states"; "no
plan in muted text"; "tab switching keeps open runs"; "back returns to previous page"; "sections
without vision list"; "No metaplan yet"; "plans without metaplan list and offer"; "never built
state"; "building state with key chips"; "read-only browser sees no offer button";
"conceptPageLabel names Product overview and section_04 summary"; "PageHeaderExtras,
KeyChipExtras and KeyRowExtras mounted".

Also export `conceptPageLabel(pathname: string): string | null` from `concept-route.tsx`
("Product overview", "Engineering overview", "section_04 summary", "<slug> summary") so plan 2's
presence list can name the page a browser reads; if plan 2's presence code takes a label
function, register it there; if not, record a loom memory note for plan 2's owner. The column
label uses the `eyebrow` utility in `concept-route.tsx` (a wiring check greps `eyebrow`).

## Fix-round additions (pressure test px-browser-19, -20)

- `empty-offer.tsx` renders plan 2's `<DisabledReason reason="Available once chat is built">{<offer
  label>}</DisabledReason>` (focusable, reason as accessible description), not a disabled button
  with a tooltip; viewers see the label as text.
- `ConceptRoute` renders through plan 2's `<LoadState>`: Skeleton while loading; a 404 shows
  `EmptyState` "No summary for <doc>" with an "Open in file mode" link when the document exists,
  else "No such document" with links to both overviews (`<LoadState state="not_found" ... />`);
  any other failure is `<LoadState state="error" message action={{ label: "Retry", onClick: refetch }} />`, Retry refetching.
- `/files/` links are built with plan 3's `docRel` from the wire's repo-relative paths.
- Tests: "offer reason is the accessible description" (`toHaveAccessibleDescription`); "unknown
  summary shows not-found with file mode link"; "fetch failure shows the error with Retry and
  Retry refetches".
