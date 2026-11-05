# W2: a passage shown twice is a reference the second time; section 05 (R8, web, wave 2)

Tier: sonnet (`loom-software-engineer`). You start after W1: the reading column now merges adjacent sentences that share a passage into one fragment (`web/src/plan/concept/fragments.ts`), so a passage repeats only across separate fragments. Read `common.md`, then `doc/loom/knowledge/conventions/plan-web.md` section "Concept Mode Test Rules", then `fragments.ts`, `reading-order.ts`, `reading-column.tsx`, `passage.tsx` and `run.tsx` `PassageRun`.

## Behaviour

When two separate fragments open in one view would both show a source passage (same `path#anchor`), the first one in reading order shows it as today and every later one shows a one-line reference instead of the body:

- the same `.concept-passage` element (same `data-focus`, `data-passage`, `leading` and `trailing` caps), its chip link (`file › heading`, unchanged), then a `<button type="button">` whose text is exactly `Same passage as above`, inside a `<p className="concept-muted">`; no Markdown body;
- clicking the button moves the roving focus to the first showing (its focus key), which scrolls it into view as the roving focus does for any target.

The same passage inside one fragment never repeats (W1's union), so this rule acts only across fragments.

## Build

- New `web/src/plan/concept/same-passage.ts` (no JSX):
  - `export function firstPassageKeys(order: ColumnTarget[]): Map<string, string>`: for each passage target in order, its `id` (the `path#anchor` key) mapped to the `targetKey` of its first occurrence.
  - `export type SamePassage = { first: Map<string, string>; goTo: (key: string) => void }`, `export const SamePassageContext = createContext<SamePassage | null>(null)`.
  - `export function useSamePassage(anchor: string, ownKey: string): (() => void) | null`: null when there is no provider, the anchor is unknown, or `ownKey` is the first occurrence; else a callback calling `goTo(first)`.
- `reading-column.tsx`: wrap the column's children in `<SamePassageContext.Provider value={...}>` with `first: firstPassageKeys(model.order)` (memoised on `model.order`) and `goTo: model.roving.move`.
- `passage.tsx` `PassageBlock` (~73): `const goFirst = useSamePassage(key, passageKey(parent, sentenceId, key))`; when `goFirst` is set, render the reference line above in place of `PassageBody`/missing text. Keep every attribute and the caps.
- Copy goes through plain text (no server text in the button). No new CSS: `.concept-muted` and the chip already exist (do not edit `concept-runs.css` or `fold-handle.tsx`; `concept.css` is V1's, who runs beside you).

## Tests

- `same-passage.test.ts`: `firstPassageKeys` keeps the first of two passage targets with one id and ignores sentence targets.
- A column test (an existing concept test file under 350 lines, or a new `same-passage-column.test.tsx`): a page with A{stages}, B{intent}, C{stages}; open A and C; the stages body renders once, one button named "Same passage as above" exists, and clicking it focuses A's passage element (assert inside `waitFor`).

## Product spec (`doc/product/feat-planner/section_05.md`)

Rewrite to the new truth (no history):

- "Levels" (~11-31): the table's level 1 row keeps vision, section and metaplan summaries; state that a plan summary has two levels of its own (an opening level of about one sentence per topic, and a detail level of short paragraphs), so a plan page reads opening sentence, then detail sentences, then passages, and an overview's engineering sentences cite a plan's detail sentences. Replace "A reader starting from a document summary page (below) has two levels" accordingly.
- "Document summary pages" body paragraph (~87-88): a plan page shows its opening level; each opening sentence opens to its detail sentences in place.
- "What is clickable" (~113-119): add **A fragment.** Consecutive sentences of a paragraph or an opened run that open to the same content (a shared source passage, or the same cited sentences) form one fragment: one hover wash over all of its text, one chevron after its last mark, one button; opening it shows what its sentences open to, each passage once.
- "Opening" (~135-152): add that a passage already shown by an earlier fragment in the same view appears as one line, its chip and "Same passage as above", which moves focus to the first showing.

## Contract (written before you start, by the contract session)

`web/src/plan/concept/levels.contracts.test.tsx` "a passage shown twice in one view shows a reference the second time" is yours. Do not edit the file.

## Files

`web/src/plan/concept/same-passage.ts`, `same-passage.test.ts`, `passage.tsx`, `reading-column.tsx` (the provider only), the column test you add or extend, `doc/product/feat-planner/section_05.md`.

## Run once at the end

`cd web && node_modules/.bin/vitest run src/plan/concept`

## Report

Files changed; the run's result; every pinned test you changed and why; anything unsettled.
