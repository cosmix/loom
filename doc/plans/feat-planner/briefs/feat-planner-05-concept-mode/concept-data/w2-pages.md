# concept-data W2: page bodies and load_page

Working dir `loom/`. You own `src/plan_server/concept/{body.rs, page.rs, tests.rs}`. W1's
`wire/concept.rs`, `key.rs`, `links.rs`, `state.rs` exist when you start; read them, do not edit
them. The frozen contracts are `tests/plan_concept_contracts.rs` and
`tests/plan_concept_contracts/**`: never edit them; make them pass.

## page.rs

The public surface from the stage description: `Scope`, `PageKey` with `parse`, `PageContext`,
`load_page(project, eval, key, ctx) -> Option<ConceptPage>`.

1. `Overview(scope)`: key chips (W1), blocks from the overview summary (`product` or
   `engineering` in `eval.summaries`) through `body::blocks`, `unlisted` from
   `store::scope(project).outside` filtered to the scope (sections for product, plans for
   engineering), each a `DocLink` to its summary page; `empty` when the overview cannot render
   (below). Title "Product overview" / "Engineering overview".
2. `Summary { scope, doc }`: resolve `doc` to the summary id (`product/<stem>` for a product
   section file stem, `plans/<slug>` for a plan); `None` when the project has no such document.
   Title from `ProjectDoc.title`, `path` the source path (repo-relative), `file_hash` the summary
file's `normalized_hash` from the `SummaryEval`'s file (also set on overviews with a file), colour from `key::colour_of` over the
   scope's chips, `state` from the `SummaryEval` plus `state::out_of_date_reason`, for plans
   `plan_state` (written, running, done, retired), `last_pressure` (latest
   `pressure::records` date) and `implements` (W1's `implements_view`); for sections
   `implemented_by` (W1).
3. Empty states, product: no vision and no section files gives `no_vision` with offer
   `start-vision` (plan 1's `Offer::StartVision.id()`); sections but no vision gives
   `sections_no_vision` listing section summaries in file-name order; documents but every
   summary `Missing` gives `never_built` with `count`; the overview summary `Missing` while some
   document summary exists gives `building` (the key still lists finished document summaries).
   Engineering mirrors it with `no_metaplan` (offer `start-metaplan`) and `plans_no_metaplan`.
   `building` and `estimate` stay `None` (set them in one place, a `fn progress(ctx: &PageContext)`
helper returning `(None, None)`, which plan 10 fills from its `PageContext` field). An overview with a valid file always renders that
   file, whatever its state: the store replaces files atomically, so a page never mixes two
   builds.

## body.rs

`pub(crate) fn blocks(file: &SummaryFile, eval: &Evaluation, chips: &[KeyChip], level: Level,
ctx: &PageContext) -> Vec<Block>`. Headings become `Heading`. Each paragraph's sentences
become `Sentence` with:

- `citations`: in link order; `LinkTarget::Sentence { summary, id }` gives
  `Citation::Sentence`; `LinkTarget::Source { path, anchor }` gives `Citation::Source`;
  `Unresolved` links are dropped (plan 4's validation reports them).
- `rests_on`: level 1, the sentence's own source anchors; level 0, the ordered union of the cited
  level-1 sentences' source anchors (looked up in `eval.summaries[summary].file`) and direct
  `Source` citations, deduplicated, first occurrence wins. A cited sentence that no longer exists
  contributes nothing.
- `marks`: level 1, one mark for the summary's own document; level 0, one per distinct cited
  document in citation order (a cited summary sentence marks that summary's source document; a
  direct vision or metaplan citation marks that document with colour 0). Each mark carries
  `out_of_date` from that document summary's state (`None` for colour 0).
- `plans`: on the first sentence of each paragraph of a product section's summary only, W1's
  `implemented_by(number)` (the glyph sits after the first sentence's marks); empty otherwise.

## tests.rs

Unit tests beside the contracts for cases the contracts do not pin: an overview citing a
sentence that no longer exists (citation kept, its mark dashed, nothing added to `rests_on`),
the `building` empty state, a plan page with two pressure records (latest date wins), a
`sections_no_vision` list in file-name order. Proof: `cargo test --lib plan_server::concept::`
and `cargo test --test plan_concept_contracts`.

## Required test cases (coverage audit; each a named test in `tests.rs`)

"two metaplans give chips in metaplan then entry order"; "latest pressure date wins"; "summary
page colour equals its overview chip colour"; "sentence without links has no marks";
"implemented_by on a section page"; "plans on the first sentence of each paragraph only";
"sections_no_vision list"; "no_metaplan"; "plans_no_metaplan"; "never_built count";
"building state"; "a sealed overview renders whatever its state; missing gives building";
"missing cited sentence keeps the citation and dashes its mark".
