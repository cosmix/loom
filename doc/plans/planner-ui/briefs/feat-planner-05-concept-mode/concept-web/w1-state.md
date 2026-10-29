# concept-web W1: data, view state, open-state rule, reading order, selection

Lane note: codex units own `api/schema-concept.ts` (C1), `concept/types.ts` (C7),
`reading-order.ts` (C2), `view-state.ts` (C3), `passages.ts` (C4), `selection.ts` (C5) and
`doc-tokens.test.ts` (C6), each with its test; the sections below on those files are the
reference their briefs build on. C1 and C7 are in the tree when you start; C2 to C5 run beside
you.

Working dir `web/`. You own `src/plan/concept/{page-state.ts, page-state.test.ts,
open-state.ts, open-state.test.ts}`; `open-state.ts` re-exports `OpenRun` and `FoldNote` from
codex unit C7's `types.ts`. W2 and W3 start from your types; report the exported names
when `bun run typecheck` passes on your files. The frozen contracts are
`src/plan/concept/concept.contracts.test.tsx`: never edit it.

## api/schema-concept.ts

Zod schemas mirroring `loom/src/plan_server/wire/concept.rs` field for field (read it; the
fixtures `src/plan/api/fixtures/concept-{overview,summary}.json` must parse:
`schema-concept.test.ts` parses both, and checks the enum strings concept-data's Rust test pins).
Export the inferred types (`ConceptPage`, `Sentence`, `Citation`, `KeyChip`, `Mark`,
`SourceAnchor`, `OutOfDate`, `ConceptChanged`, ...). Follow plan 2's `schema.ts` style.

## page-state.ts

Jotai: an atom family by page key holding `{ page: ConceptPage | null; previous: ConceptPage |
null; error: string | null }`, loaded with plan 2's `apiGet("/api/concept/" + key, schema)`.
`subscribeConcept(store)` uses plan 2's `subscribe("concept_changed", handler)`: for each
`out_of_date` summary id, mark the loaded pages' marks and chips for that document dashed at once
(reason `source_changed`, until the refetch says otherwise); for each listed page that is loaded,
refetch and store `previous = page, page = next`; for each `sources` path, refetch that path's
document in the passage cache. Page keys: `product`, `engineering`, `product/<stem>`,
`engineering/<slug>`.
Also export `useConceptSubscription()`, a hook that calls `subscribeConcept` on mount and
disposes it on unmount; W3 calls it in `ConceptRoute`, so no file of plan 2's entry changes.
Because the subscription lives only while a concept page is mounted, every page load from the
cache also refetches (render the cached page at once, then replace it through the normal
replacement path so `carryOpenState` runs), and plan 2's `onResync` (on the `planSocketAtom` handle) refetches every loaded
page and passage.

## passages.ts

A cache of plan 3's `DocView` per document path, loaded with `apiGet("/api/doc?path=" +
encodeURIComponent(docRel(<repo-relative path from the wire>)), <plan 3's DocView schema>)`.
`passageOf(doc: DocView, anchor: string): { title; anchor; startLine; endLine; markdown; hash }
| null`: the section whose `anchor` matches (the preamble `""` runs from line 1 to the first
section's start); `markdown` is those lines of `doc.text`; `hash` is `doc.hash` plus the anchor
(a changed file re-renders and flashes its open passages). Tests cut a nested section, the
preamble, and a missing anchor (null).

## view-state.ts

`openRunsAtom(pageKey)`: `OpenRun[]` (type from the stage description), mirrored to
`sessionStorage` key `loom-plan:concept:<pageKey>` as JSON. Every storage read and write sits in
`try/catch`; a failure keeps the in-memory value. Reads validate with zod and drop bad data.
`notesAtom(pageKey)`: `FoldNote[]`, memory only. A note is removed when the reader opens or folds
anything in its paragraph (`clearNotesIn(paragraphOf)`).

## open-state.ts

`carryOpenState(prev, next, open)` per section 05 "What stays open":

- A level-2 (passage) entry stays while some sentence of `next` still cites that source anchor
  and the anchor exists (an anchor missing from every `rests_on` of `next` counts as deleted);
  else it is dropped and a note "Section *<title>* was removed from `<file>`" is added, where
  title comes from the anchor's last known passage title (pass it in `OpenRun.restsOn[0]`'s
  label or keep a title map; settle on adding `title?: string` to `SourceAnchor` in the TS type
  only).
- A level-0 or level-1 entry: among `next`'s sentences at the same level (inside the same
  parent run for level 1), the one whose `rests_on` shares the most anchors with the old
  entry's `restsOn`; ties go to the first in reading order; zero overlap means none. The entry
  moves to that sentence's id with its new `restsOn`. With none, drop it and every entry whose
  parent chain includes it, and add "This summary was rewritten and no longer draws on
  *<title>*" for the first anchor.
- Children are re-parented to their parent's new id. Output order follows `next`'s reading
  order.

Unit tests in `open-state.test.ts` for: kept unchanged; moved to best overlap; tie by reading
order; nested run dropped with its parent; passage deletion note; renamed heading (old anchor
gone, new present) treated as deletion.

## reading-order.ts

`readingOrder(page: ConceptPage, open: OpenRun[], passages): FocusTarget[]`, the flat list the
roving focus walks: sentences in document order, and inside an opened sentence its run's
sentences (or its passages, as one target each) in citation order, recursively. A folded run's
sentences are never in the list. `FocusTarget = { kind: "sentence" | "passage"; id: string;
level: 0 | 1 | 2; runOf: string | null }`. Tests: closed page, one open level-0 run, nested
level-1 run with two passages.

## selection.ts

`attachmentFromSelection(sel: Selection, page: ConceptPage): AttachmentDraft | null` producing
plan 3's `AttachmentDraft`, filling exactly these fields: summary arm `{ kind: "summary",
summary: <summary id, e.g. product/section_05>, sentence: <sentence id>, text: <selected text>,
restsOn: <the sentence's rests_on, repo-relative>, hash: page.file_hash }`; source arm (passage
text) `{ kind: "source", path: <passage path, repo-relative doc/...>, anchor, text, lines,
stageId: null, hash: <the passage document's DocView.hash> }`: for summary
text, `kind: "summary"`, the summary file path and `#<sentence id>` anchor, `summary_sentence`,
`rests_on` from the sentence; for passage text, `kind: "source"`, path, heading anchor and line
range (`start_line`..`end_line` of the passage, narrowed when the selection lies inside one
rendered block that carries `data-line`); a selection spanning levels gives the draft of the part where the
selection starts, and `attachmentsFromSelection` (plural, same arguments) gives one draft per part
for W3 to add in order. Elements carry `data-sentence-id`, `data-summary`, `data-passage="path#anchor"`
attributes (W2 sets them). Tests build DOM fragments in jsdom.

## Required test cases (coverage audit; each a named test)

- `page-state.test.ts` (yours; add it): "concept_changed out_of_date dashes marks before the
  refetch"; "listed loaded page refetches and keeps previous"; "sources refetch open passages";
  "unlisted page is not fetched".
- `open-state.test.ts`: "kept unchanged"; "moved to best overlap"; "tie by reading order";
  "nested run dropped with its parent"; "passage deletion note"; "rewritten summary note text";
  "renamed heading".
- `view-state.test.ts`: "storage failure keeps memory"; "bad stored data dropped";
  "clearNotesIn removes the paragraph's notes".
- `reading-order.test.ts`: "closed page"; "one open level-0 run"; "nested level-1 run with two
  passages"; "folded run never listed".
- `selection.test.ts`: "summary provenance"; "passage provenance with lines"; "spanning levels
  gives one draft per part".
- `passages.test.ts`: "nested section"; "preamble"; "missing anchor gives null".

## Fix-round additions (pressure test px-browser-06, -07, px-seams-04)

- Stored open runs whose sentence ids are absent from the loaded page (a reload after a rebuild):
  run `carryOpenState(syntheticPrevious(stored), page, stored)`, where the synthetic previous
  page holds one sentence per stored run with its stored `restsOn`, so the run moves by overlap or
  folds with its note.
- Tests: "returning to the tab after a concept_changed shows the new page" (unmount, deliver
  `concept_changed`, remount, assert the refetch and the new text); "reload after a rebuild moves
  the stored run by overlap and writes the fold note"; "resync refetches every loaded page";
  "concept_changed without sources refetches the named pages"; "summary draft fields"
  (summary id, sentence id, repo-relative restsOn, page file hash); "passage draft carries the
  DocView hash".
