# R1 — the vision gets its own document summary (issue 3, server side)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first, D1 in particular.

## Goal

`doc/product/<f>/index.md` (the vision) becomes a summarizable document with id `<f>/product/vision`, summary path `doc/loom/summaries/<f>/product/vision.md`, `kind: document`, and at most 8 sentences. It is expected, evaluated, rebuilt, sealed and served exactly like a section summary. The product overview keeps citing vision sections directly: `seal/links.rs::allowed_sources` and `uncited` stay as they are.

## Sites to change (traced at `ed36c12d`; reopen each before editing)

- `loom/src/planning/summary/store.rs`:
  - `validate_id`: accept `<feature>/product/vision` beside `product/section_NN`.
  - `summary_id_for` (102-124): map the vision `DocKind` of a feature's `index.md` (find the variant in `planning/` with `loom map --find-all DocKind`) to `<f>/product/vision`.
  - `document_ids` (148), `expected_ids` (229) and `source_of` (243): the vision id is expected whenever `vision_path(feature)` exists.
  - `Scope`/`scope()`/`product_members`: the vision summary is NOT a product member, and `scope()` also leaves it out of `result.outside` (otherwise `plan_server/concept/page.rs` lists it among the overview's outside documents). The overview's members stay the sections; the existing `outside` assertions in `tests/planning_summary_contracts.rs` `summary_scope_table` stay as they are.
- `loom/src/planning/summary/format.rs`: the sentence cap. Add `MAX_VISION_SENTENCES = 8` and `pub fn sentence_limit(id: &str) -> usize` (overview ids give `MAX_OVERVIEW_SENTENCES`, a `<f>/product/vision` id gives 8, every other document id `MAX_DOCUMENT_SENTENCES`). Use it at both limit sites, `parse_summary` and `parse_draft`.
- `loom/src/planning/summary/brief.rs` `write_rules`: it prints `MAX_DOCUMENT_SENTENCES` (15) for every non-overview id today, so a vision rebuild would be told "at most 15" and refused at 9. Pass the id in and print `format::sentence_limit(id)`. Add the overview rule line for R2's seal check, worded exactly: "- An overview sentence that cites member sentences cites at least two of them and says what they add up to." Add a `brief/tests.rs` case: the brief for `demo/product/vision` contains "At most 8 sentences". (R2 does not edit `brief.rs`.)
- `loom/src/planning/summary/evaluate*.rs` and `status.rs`: they follow `expected_ids`. Confirm that a vision summary's staleness is judged from its own cited spans.
- `loom/src/plan_server/concept/`:
  - `page_key.rs::parse` and `PageKey`: the vision page key is `Summary { feature, scope: product, doc: "vision" }`.
  - `page.rs::page_key_of`.
  - `body.rs::doc_link` and `mark`: a vision passage on the vision summary page links to the vision summary page itself. On the product overview, the vision citation still opens straight to its passage and keeps colour 0.
  - `delta.rs::document_fields`: it splits on `/product/section_` and would misread the new id as a plan. Handle `/product/vision`.
  - `key.rs::chip_label`: the vision is not a key chip on the product overview.
  - `empty.rs` (its section empty-state branches): the empty state for a vision with no summary yet uses the same "not built yet" path as a section.
- `loom/src/plan_server/wire/concept.rs`: only if a type needs a new value. Then update the hand-edited `loom/src/plan_server/wire/fixtures/concept-*.json`, which have no writer test.
- `loom/src/plan_server/summaries/` (`spawner.rs`, `scheduler/session.rs`, `rebuilds/panel.rs`): the vision summary joins the document wave, before the overview wave, like a section. Confirm with a test; change only what is needed.
- Product spec:
  - `doc/product/feat-planner/section_05.md` "Levels" says the vision has no summary of its own. Rewrite it to the new truth: the vision has a document summary of at most 8 sentences, and a product overview sentence that draws on the vision still opens straight to its source passage.
  - `doc/product/feat-planner/index.md`: update the decision the section cites ("Summary levels") to match.
  - R2 owns `plan-plugin/agents/**`. Tell the main agent which prompt line the vision needs, and R2's brief already carries it.

## Tests

- The stage contract `vision-summary-is-expected` (frozen; do not edit) drives the store change.
- Add a sealing test: a vision draft of 9 sentences is refused, and one of 8 seals.
- Add a concept page test: the vision summary page serves under the vision page key, and its passages open `doc/product/<f>/index.md` sections.
- Expected breakage: every fixture built on `evaluate/fixtures.rs` `two_sections()` or `two_features()` has `doc/product/demo/index.md`, so the vision id joins its expected ids. Known sites: `planning/summary/status/tests.rs` (`assert_eq!(ids, [ID1, ID2, PRODUCT, "plans/gone"])` and the `rows[3]` index after it), `planning/summary/engine/tests.rs` (the exact `expected` list and `invalidated`), and likely `brief/tests.rs`, `write/tests.rs` and `cost/feature_tests.rs`; also `loom/tests/planning_summary_contracts.rs`, `planning_summary_cli_contracts.rs`, `summary_engine_contracts.rs`, `plan_concept_contracts.rs`, `planning/summary/store/tests.rs` and `plan_server/concept/*_tests.rs`. You own all of them. Update each id list and count to include the vision id, keep every other assertion, and list each file and test in your report for the integrity dispute.
- `loom/tests/plan_summary_rebuild_contracts.rs` builds `SummaryEval` and `Scope` literals: add no field to either struct.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::summary:: plan_server::concept::` (one command; filters after `--`). The integration targets your ids touch run in the main agent's stage gate, not here. If the verify guard blocks the run, report that to the main agent.
