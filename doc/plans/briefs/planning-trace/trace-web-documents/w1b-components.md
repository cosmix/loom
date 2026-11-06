# W1B: settle control, stub panel, chips and impact rows (trace-web-documents, wave 2)

Read `../common.md` "Settled marks", "Routes" and "The browser" in full, then W1's report (the
exported `trace/state.ts` and `trace/model.ts` signatures). Use semantic markup and the accessible
names the contracts read (`Mark settled`, `Changes since settled`); V1 sets the look after you.

## Files you own

`web/src/plan/trace/settle-control.tsx`, `stub-panel.tsx`, `edge-chips.tsx`, `impact-rows.tsx`,
`components.test.tsx`, `web/src/plan/concept/page-heading.tsx`, `concept/sentence.tsx`, the
feature source page (`web/src/plan/feature/feature-source-page.tsx` or the merged file holding
`FeatureSourcePage`), `web/src/plan/chat/turn-footer.tsx` (or the merged file holding the turn's
changed-file list).

## Tasks

1. **SettleControl.** Shows the state; `Mark settled` and `Unsettle` post `/api/settle` with the
   path relative to `doc/` and the opened document's hash through `apiPost`
   (`web/src/plan/api/client.ts`). A 409 `unresolved` answer opens a confirmation built on
   `web/src/components/ui/dialog.tsx` listing the My call and open-question counts; confirming
   resends with `confirm=1`. A 409 conflict refreshes the document as file mode's save does.
   For `revised`, `Changes since settled` asks `/api/settled-version` and opens the review
   module's commit comparison (`web/src/plan/review/changed-since.tsx` and its api) at that
   commit; with `commit: null` it says "The settled version is not committed". Locked documents
   show the state and no buttons.
2. **StubPanel.** For sections and plans: the stub text (the vision row, or the metaplan entry's
   heading and first paragraph) linked to its place, the "stub changed since settled" note, and
   for plans the brief list from `bundleLinks`.
3. **EdgeChips** from `chipsFor`; **ImpactRows** from `impactFor`.
4. **Wiring.** `SettleControl` and `StubPanel` in `page-heading.tsx` (summary pages) and the
   feature source page; `EdgeChips` in `sentence.tsx` after the sentence text (keep `PlanGlyph`
   and `plansOf` exactly as they are: a frozen contract pins them); `ImpactRows` under each changed
   document in the turn footer.

## Tests

`components.test.tsx`: each settle state's controls, the unresolved dialog and its resend, the
conflict refresh, the not-committed message, the locked case, chips on a product and a plan page,
impact rows, the brief list. The frozen contracts in `trace/trace.contracts.test.tsx` must all
pass now. Existing concept and chat tests keep passing; never edit a `*.contracts.test.tsx`.

## Done

tsc, oxlint, oxfmt and `vitest run src/plan` pass; every changed file stays under 400 lines.
