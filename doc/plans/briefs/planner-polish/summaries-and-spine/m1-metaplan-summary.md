# M1: a document summary for each metaplan (issue 7, D2)

Tier: sonnet (`loom-software-engineer`). Read `common.md` (D2 above all), `doc/loom/knowledge/architecture/summary-store.md` and `doc/loom/knowledge/architecture/planning-document-model.md` before code. The vision summary (`<feature>/product/vision`) is your precedent: a document summary that belongs to no overview. Follow how it is threaded through each module below, and do not copy its 8-sentence limit.

## Build

1. **Store** (`loom/src/planning/summary/store.rs`):
   - `validate_id` accepts `<feature>/metaplan/<stem>`: `is_feature_name(feature)`, and a stem matching `[a-z0-9][a-z0-9-]*`. It still rejects `..`, empty stems and any other shape.
   - Add `pub fn is_metaplan(id: &str) -> bool`, next to `is_vision`.
   - `summary_id_for` maps `DocKind::Metaplan { feature: Some(f) }` to `<f>/metaplan/<file stem>`. A stem the grammar rejects gets `None`.
   - `expected_ids` includes every such id (through `document_ids`). `source_of` finds it.
   - `Scope` lists never include it.
   - Update the module doc comment (lines 1-28) to the new grammar.
   - The file is at 330 lines. If the change would pass 400, move the id grammar into `store/ids.rs`.
2. **Format** (`format.rs`): `sentence_limit` returns `MAX_DOCUMENT_SENTENCES` for a metaplan id (the existing fall-through; add a unit test that pins it). `classify_href` must resolve a link into `doc/loom/summaries/<f>/metaplan/<stem>.md#s-…` as a `Sentence`. Check it, and test it.
3. **Evaluate, brief, engine, seal, write, claims, status, cost:** wherever the code matches on the vision id or on a document kind, a metaplan summary takes the document path:
   - its sources are its own metaplan file;
   - its sentences cite only source anchors of that file;
   - the rebuild brief names it as a document;
   - its staleness follows its source hash.
   `rg -n 'is_vision|product/vision|DocKind::Vision' loom/src/planning/summary loom/src/plan_server` lists the threading points. Handle each one, or say in your report why the vision-specific branch does not apply.
4. **Prompts:** `plan-plugin/agents/summary.md` and `loom/src/planning/guidance/text/summary_codex.md` name a metaplan as a summarizable document: summarize its plan map and its sequencing decisions, cite its own passages.
5. **Server** (`loom/src/plan_server/`):
   - `concept/page_key.rs`: `Scope::Metaplan`. `PageKey::parse` accepts `<feature>/metaplan/<stem>`, and the key formats back to the same string.
   - `concept/page.rs` `page_key_of` maps the summary id to that key.
   - `concept/key.rs` and `concept/body.rs`: a metaplan summary page's key chips and links behave like a plan summary page's.
     - Do NOT change `doc_link`'s metaplan stand-in for overview-level citations (`body.rs` around 216-217 and 256-262). The engineering overview still cites raw metaplan passages, and that mapping still applies there.
   - `summaries/spawner.rs` `summary_feature` and `summaries/routes.rs` (`validate_id`, `expected_ids`) accept the new id.
   - `wire/concept.rs`: the page's scope serializes as `"metaplan"`.
6. **Tests:** a unit test per module you change, next to its existing tests. Name the ones that fail on the wrong behaviour: an id the grammar must reject, and the summary appearing in a `Scope` list.

## Pinned tests you will change (planned, common.md table)

`expected_ids` grows by one id per feature metaplan. The store fixture in `store/tests.rs` has `doc/plans/alpha/index.md`, `alpha/metaplan-02.md` and `beta/index.md`, and the top-level `doc/plans/index.md`, which gets no id.
- In lib tests (`store/tests.rs`, `status/tests.rs`, `engine/tests.rs`, `evaluate/tests.rs` and others), update every id list and count, and keep every other assertion.
- In the frozen Rust contract targets named in common.md's table, change ONLY those id lists and counts. List every one by file and test name in your report; the main agent files the integrity dispute.

## Contract (written before you start, by the contract session)

`loom/src/planning/summary/store/metaplan_contract_tests.rs`, `a_feature_metaplan_has_a_document_summary`. Make it pass. Do not edit it, and do not touch its `mod` line in `store.rs`.

## Files

Your rows in the plan's worker table. Never edit W1's or N1's files.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::summary:: plan_server::concept:: plan_server::summaries::`
