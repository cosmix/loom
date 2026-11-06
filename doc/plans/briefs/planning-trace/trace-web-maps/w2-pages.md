# W2: the Coverage and Findings pages (trace-web-maps, wave 1)

Read `../common.md` "The browser" in full. You build behaviour and plain markup; V2 sets the
look after you.

## Files you own

The `web/src/plan/trace/` page, model, keys and action files in the stage table, plus
`web/src/plan/router.tsx`, `feature/spine-model.ts`, `spine-model.test.ts`,
`feature/built-page.tsx` and `feature/page-offer.ts`.

## Tasks

1. **Routes.** `coverage` and `findings` children of `/f/:feature` in `router.tsx`, before the
   `*` entry, rendering `<CoveragePage />` and `<FindingsPage />`.
2. **Coverage model** (`coverage-model.ts`, pure): columns built from the trace (vision rows from
   section documents' stubs, headings grouped by section, metaplan entries, plans with their
   stages from `covered_by`, findings), each card `{ key, column, title, state, lineage: key[] }`;
   `lineage(key)` returns the cards related upstream and downstream; the filters All, Uncovered,
   Changed since settled, Open findings.
3. **Keys** (`coverage-keys.ts`): roving focus inside the page; `j`/`k` within a column, `l` to
   the first downstream card of the focused card's lineage, `h` to the first upstream one,
   `Enter` opens the node (summary page for a document, source passage link for a heading, the
   Findings page anchor for a finding). Inactive while typing or with a dialog open, as
   `useEscapeKeys` decides.
4. **Coverage page.** Five columns, cards as buttons with an accessible description for gaps
   ("Not covered") and for changed settle states, the selected card's lineage marked with
   `data-lineage`, and a focus panel (`aside` named after the node) listing Why, How and Findings.
5. **Findings model and page.** Group by `source` run (the review stem, newest plan state first),
   then findings without a run source under "Reviews". Each card: id, title, About chips (linked
   to the node), status, text, all as text. Actions (`findings-actions.ts`): Plan this and Fold into
   entry call `runOffer` with the feature's `plan_follow_ups` offer (from the checklist; disabled
   with a reason when the feature has none) and a seed; Fold into entry first picks an entry
   whose plan is unwritten. Dismiss opens a reason field, confirm disabled while it is empty,
   then POST `/api/findings/status` with the ledger hash (the trace's ledger document hash, or
   fetch the file's hash as file mode does). The page header owns the `Review the feature`
   secondary offer: update `page-offer.ts` so no other screen draws it.
   Seed text: "Plan finding <id>: <title>\nAbout: <tokens>\n\n<text>" and "Fold finding <id> into
   metaplan entry <entry title> (metaplan#<anchor>): <title>\nAbout: <tokens>".
6. **Spine and Built.** A `Coverage` child under the Product spec group and a `Findings` child
   under the Run group whose name carries the open count ("Findings, 3 open"), both from the
   trace; the Built page links to the Findings page.

## Tests

Model and keys unit tests, a pages test for each page's empty state (no trace, no ledger), the
offer owner rule (`page-offer` tests), and spine model tests in new files if
`spine-model.test.ts` would pass 400 lines. The six contracts in `trace/maps.contracts.test.tsx`
are frozen.

## Done

The stage's web acceptance commands pass; every changed file under 400 lines.
