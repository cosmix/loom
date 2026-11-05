# W1: one fragment per shared passage, and the two-level plan page (R8, web)

Tier: sonnet (`loom-software-engineer`). Read `common.md`, then `doc/loom/knowledge/architecture/planner-web-shell.md` section "Reading column" and `doc/loom/knowledge/conventions/plan-web.md` section "Concept Mode Test Rules". Read each file below before editing it; line numbers are anchors, the symbols are the truth.

## What the reader gets

1. **Fragments.** Within one paragraph, consecutive sentences that would bring in the same content when opened form one **fragment**: one button, one hover wash over all of its text, one chevron after its last mark, one `aria-expanded`. Opening it shows the union of what its sentences open to, each item once. The same rule applies to the sentences shown inside an opened run.
2. **Two-level plan page.** A plan page whose summary has two levels (S1 builds them on the server) shows its opening sentences; opening one shows the detail sentences it cites, in place, as an overview's run shows member sentences; opening a detail sentence shows its source passages with the chip link to the source.

## The wire (S1 writes the server side)

- `ConceptPage` gains an optional `detail` array of `Sentence`, present only on a two-level plan page. In `web/src/plan/api/schema-concept.ts` add `detail: z.array(sentenceSchema).optional()` to `conceptPageSchema` after `blocks`. Optional, so no fixture and no `ConceptPage` literal changes.
- An opening sentence cites detail sentences as `{ kind: "sentence", summary: "plans/<slug>", id }`, the page's own summary id, and its marks carry the plan's own `DocLink` (`doc.id` `plans/<slug>`, `doc.page` the page key). Detail sentences are ordinary source-citing sentences.

## Decisions (settled)

- **Grouping is computed on the client**, from the sentences in display order. The server cannot know adjacency inside an opened run (a run shows cited sentences in citation order, across summaries), so a server-side group id would be wrong there. There is no `group` field on the wire.
- **Open keys.** `openKeys(sentence)`: when the sentence has any sentence citation, one key `s|<summary>#<id>` per sentence citation; otherwise one key `p|<path>#<anchor>` per anchor of `passageAnchors(sentence)` (`column-context.ts` ~72). A sentence with no keys stands alone and never joins.
- **Merge rule.** Walk the sentences in order; a sentence joins the current fragment when its keys meet the union of the keys the fragment has gathered so far; otherwise it starts a new one. So A{a}, B{a,b}, C{b} is one fragment; A{a}, B{b}, C{a} is three.
- **A fragment is a merged sentence.** `mergeFragments` returns, for a fragment of two or more, a new `Sentence` object: `id` the first member's id (the lead), `text` the member texts joined by one space, `marks` the union by `doc.id`, `citations` the union (by `summary#id` or `path#anchor`), `rests_on` the union by `anchorKey`, all in first-seen order, `plans` the lead's. A module-level `WeakMap<Sentence, Sentence[]>` records its members. Because the merged sentence is a plain `Sentence`, the existing column machinery (open runs, reading order, carry, Open all, keys) handles a fragment as one sentence with no further change wherever it receives the merged page.
- **Union order** is first-citation order across the members (members are in reading order and cite in source order, so this is source order in practice). Do not fetch documents to sort.
- **Where the merged view applies:** the reading column and everything that reasons about its open runs. Raw sentences stay everywhere a sentence is resolved by id for chat (selection, `a` attach, the selection resolver), so an attachment always names one real sentence.

## Build

### New `web/src/plan/concept/fragments.ts`

```ts
export function openKeys(sentence: Sentence): string[];
export function mergeFragments(sentences: Sentence[]): Sentence[];
/** The sentences a merged sentence stands for, in order; `[sentence]` for one that stands alone. */
export function fragmentMembers(sentence: Sentence): Sentence[];
/** `page` with each paragraph's sentences merged; memoised per page object (WeakMap), `detail` untouched. */
export function columnPage(page: ConceptPage): ConceptPage;
export type RunEntry = { citation: SentenceCitation; sentence: Sentence | undefined };
/** What an opened `citing` sentence shows, in citation order: each sentence citation resolved through
 * `lookup`, consecutive resolved entries merged by the merge rule (a merged entry keeps its lead's
 * citation); an unresolved entry stands alone. */
export function runSentences(citing: Sentence, lookup: CitedLookup): RunEntry[];
/** Moved here from column-context.ts and made member-aware: the open run (level < 2) at the site of
 * any member of `site.sentence`, at `site.parent`. */
export function runAt(open: OpenRun[], site: SentenceSite): OpenRun | undefined;
```

`fragments.ts` imports `passageAnchors`, `sentenceCitations` and the `SentenceSite` type from `column-context.ts`; delete `runAt` from `column-context.ts` and update its three callers (`run.tsx`, `open-all.ts`, `column-actions.ts`), so no import cycle forms. Tests in `fragments.test.ts` (a `.ts` file: no JSX): the two merge examples above, a sentence-citing pair sharing a cited sentence, a no-citation sentence between two sharing ones (three fragments), unions and lead id, `fragmentMembers` of a single sentence, `columnPage` returning the same object for the same page, `runSentences` merging adjacent resolved entries and leaving an unresolved one alone.

### Two-level lookups

- `open-state.ts`: `export function citableSentences(page: ConceptPage): Sentence[]` = `pageSentences(page)` then `page.detail ?? []`. `pageSentences` stays the page's own paragraphs.
- `cited-pages.ts` `useCitedPages` (~45): `citedKeys` (~29) leaves out the column's own `pageKey`; `citedPage(summary)` returns `page` when `summary === summaryIdOfKey(pageKey)`; `lookup` searches `citableSentences`.
- `page-state.ts`: `citedLookup` (~78) answers the page's own summary id (`summaryIdOfKey(page.key)`) from `citableSentences(page)` and other summaries from `citableSentences` of the loaded page; `restoreStoredRuns` (~92) carries against `columnPage(page)`; the out-of-date dashing (~180-192) also maps `page.detail` marks.
- `concept-route.tsx` `sentenceResolver` (~164): `citableSentences` instead of `pageSentences`.

### The column on the merged view

- `reading-column.tsx`: `ReadingColumn` computes `const view = columnPage(page)` and uses `view` for the model, the context and `<Blocks>`. `useCarryOnReplace` (~78) calls `carryOpenState(columnPage(previous), columnPage(page), ...)`. `columnOrder` gets `cited.lookup` (below).
- `reading-order.ts`: `readingOrder(page, open, passages, lookup)` and `columnOrder(...)` take `lookup`. A sentence's open test uses `runAt`; an open sentence-citing sentence lists `runSentences(sentence, lookup)`: a resolved entry adds a level-1 target with the entry sentence's id and `runOf` the citing id, then its passages when `runAt` finds its run (site parent: the citing id); an unresolved entry adds the citation id as today.
- `column-model.ts`: `passagesRecord` (~21) and `findSite` (~153) go through `runSentences` for cited sentences, so a merged nested sentence's passages are its union and its site is the merged sentence. `runFor` is unchanged (it reads the merged sentence's union `rests_on`).
- `open-all.ts`: `closedSites` and `hasOpenable` read `columnPage(page)`.
- `column-keys.ts`: `isOpenAt` and `runAround` use `findSite` plus `runAt` instead of matching the target id alone; `attach` (~112) calls `onAttach` once per member of `fragmentMembers(site.sentence)`, each with that member's id, text and `rests_on`.

### Rendering

- `paragraph.tsx`: unchanged in shape; it now receives merged sentences.
- `sentence.tsx` `ClosedSentence` (~47): when `fragmentMembers(sentence)` has one member, render exactly as today (frozen tests pin this markup). For two or more: the same `.concept-sentence-wrap` and `role="button"` span with every existing attribute (`data-focus`, `data-sentence-id` and `aria-controls` from the lead, `data-summary`, `aria-expanded`), holding one inner `<span data-sentence-id={member.id} data-summary={summary}>` per member with its `SentenceText`, separated by a space (the inner ids keep selection provenance per sentence); then one `SentenceTail` with the merged marks; flashing when any member flashes. The wrap of a fragment of two or more sentences carries `data-fragment={members.length}`; a single sentence's wrap carries no `data-fragment` (user decision: a fragment is marked as one unit at rest). V1 (fable, wave 2) styles that attribute in `concept.css`; write no CSS yourself.
- `run.tsx` `InlineRun` (~70): render `runSentences(sentence, column.lookup)`; `CitedSentence` (~149) takes the entry and uses `entry.sentence` when present, else its existing loading and missing notices. Pass `glyph={false}` when the cited summary is the page's own (`summaryIdOfKey(column.pageKey)`): the plan page's header already names what the plan implements. `PassageRun` needs no change (it reads `passageAnchors` of the merged sentence).
- Do not edit `concept.css` (V1's), `fold-handle.tsx`, `concept-runs.css` or `ui/DESIGN.md`. planner-visual has merged before this stage; build on its versions.

## Tests

- New: `fragments.test.ts` (above).
- Update `reading-order.test.ts` and `column-model.test.ts` for the new `lookup` parameter; add one case each for a merged fragment.
- Add to an existing column test file under 350 lines, or a new `fragments-column.test.tsx`: a summary page with sentences A{intent}, B{intent, stages}, C{stages}: one button whose `.concept-sentence-wrap` has `data-fragment="3"`, while a single-sentence wrap on the same page has no `data-fragment` attribute; clicking it opens one group whose passages are intent and stages once each; `a` on it attaches three summary drafts.
- Pinned test you will change: `fold-caps.test.tsx` "both caps of each opened run are named Fold and tabbable in reading order" clicks the button named exactly "Each stage names the files it may change."; in `concept-summary.json` that sentence and the next share `stages`, so the button's name now starts with it. Change that query to `new RegExp("^" + secondSentence)`, nothing else, and report it. Report every other non-contract test you change and why.
- The frozen contracts (`*.contracts.test.tsx`, `*contracts-kit*`) must pass unchanged. They were checked against this design: single-sentence fragments keep today's markup, and every frozen query on a merged button matches its name by a leading regex.

## Contract (written before you start, by the contract session)

`web/src/plan/concept/levels.contracts.test.tsx`: "a two level plan summary opens to its detail sentences and then to the source" and "adjacent sentences that share a passage open as one fragment" are yours. The third, "a passage shown twice in one view shows a reference the second time", is W2's (wave 2); it stays red after you. Do not edit the file.

## Files

`web/src/plan/api/schema-concept.ts`; in `web/src/plan/concept/`: `fragments.ts`, `fragments.test.ts`, `open-state.ts`, `cited-pages.ts`, `page-state.ts`, `concept-route.tsx`, `column-context.ts`, `column-model.ts`, `reading-order.ts`, `reading-column.tsx`, `paragraph.tsx`, `sentence.tsx`, `run.tsx`, `column-keys.ts`, `open-all.ts`, and the non-contract `*.test.ts(x)` files there that the change breaks (`fold-caps.test.tsx` included). Not `passage.tsx` or `same-passage.ts` (W2). Every file stays under 400 lines and every function under 50.

## Run once at the end

`cd web && node_modules/.bin/vitest run src/plan/concept src/plan/api src/plan/feature src/plan/review src/plan/files src/plan/chat/chips-marks.test.tsx src/plan/chat/viewed-source.test.tsx`

## Report

Files changed; the run's result (name the failing tests if any); every pinned test you changed and why; every importer outside your files that a renamed or moved symbol touched (`rg -n 'runAt|readingOrder|columnOrder' web/src`); anything unsettled.
