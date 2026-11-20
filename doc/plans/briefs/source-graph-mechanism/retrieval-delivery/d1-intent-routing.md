# D1: query intent, relationship routing, symbol questions, literal hints

- **Agent type / tier:** `loom-senior-software-engineer` (opus)
- **Wave:** 2, alongside D2 and D3. D0 added the types.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 13 (the
  Intent and Routing bullets) in full.

## Goal

`classify` implements the four intents exactly. `rank_source` routes on them:

- a symbol question admits an exact one-word node at low confidence;
- a relationship seeds the named node and adds its direct neighbours with `via` set;
- a literal demotes source lexical scores and asks the packer to attach a text-search
  hint.

Every other query keeps today's ranking, bit for bit.

## Files you own (write)

- new `loom/src/context/rank_source/intent.rs`: D0 does not create it, and does not
  declare `pub mod intent;`. You create it with `QueryIntent`, `RelationDirection` and
  `classify`, and declare it
- `loom/src/context/rank_source.rs`: `pub mod intent; mod routing;`, the routing call
  in `rank_source_channel_cached`, and the text-search step. It is at 377 lines, so put
  routing in a new `rank_source/routing.rs`. D2 never edits this file.
- `loom/src/context/rank_source/candidacy.rs`
- tests `loom/src/context/tests/rank_source_intent.rs` and `rank_source_routing.rs`.
  D0 created and declared both with one test each; add to them, and never edit
  `tests/mod.rs`.
- the request-level hint: `loom/src/context/retrieve/request.rs`, the step in
  `build_pack_request` that sets `PackRequest.text_search` for a literal intent. This
  file is yours alone in wave 2

Read-only: `rank.rs` and the `schema` types (D0), `expand.rs` (D2), render and brief
(D3), and `loom/tests/retrieval_delivery_contracts.rs` (frozen; it pins `classify`'s
`Literal` text and the symbol-question admit).

## Steps

1. **`classify`.** Implement design 13's rules with `regex` (already a dependency).
   - Compile the patterns once in `std::sync::OnceLock`.
   - Evaluation order: `Literal`, `Relationship`, `SymbolQuestion`, `General`.
   - Backticked names are unwrapped, and the symbol keeps `::` or `.` qualification as
     written.
   - Test (`tests/rank_source_intent.rs`):
     - at least 12 positive and 12 negative phrasings. Negatives include "what does
       the plan say", a lowercase word that is not a node, prose with `do` in the
       middle, and a quoted single word (`"x"`, too short). Write the negative
       phrasings as one test named `negative_phrasings_admit_nothing` (12 phrasings);
       acceptance runs it by name;
     - each relationship direction;
     - the escape of a `'` in the `rg` command.
2. **Routing** (`rank_source/routing.rs`).
   - `SymbolQuestion { symbol }`:
     - find nodes whose `node_names` equal `symbol`, or whose qualified spelling ends
       with it when it is qualified;
     - admit each as a candidate with reasons `[SymbolQuestion]`,
       `matched_term_count: 1` and `confidence_ceiling: Some(Low)`;
     - the score equals the lexical score if one exists, otherwise
       `BOOST_EXACT_SYMBOL * 0.25`;
     - keep test-path demotion.
   - `Relationship { direction, symbol }`: exact-name seeds as `ExactSymbol`, then
     their direct neighbours in that direction. Use the M1 query surface from stage
     `map-api-freshness` (`direct_callers`/`direct_callees`/`direct_references`, or
     `impact_with` depth 1 for `Impact`) at confidence at least
     `MIN_NEIGHBOR_EDGE_CONFIDENCE`. Each becomes `GraphNeighbor` with `via` set from
     the edge (seed, kind, direction, provenance, first site line).
   - `Literal`: multiply every source candidate's lexical contribution by 0.5, and
     leave rungs untouched.
   - `General`: no change. Existing tests in `tests/rank_source_candidacy.rs` must
     pass unchanged, including
     `a_one_word_name_is_not_a_candidate_however_often_the_prompt_says_it`.
3. **Hint.**
   - In `intent.rs`, implement `impl TextSearchHint { pub fn for_pattern(pattern: &str) -> Self }`
     exactly as design 13 specifies. `TextSearchHint` is D0's struct, and an inherent
     impl may live in this module.
   - In `retrieve/request.rs::build_pack_request`, when `classify(&query.text)` is
     `Literal { text }`, set
     `PackRequest.text_search = Some(TextSearchHint::for_pattern(&text))`. Nothing sets
     the hint on the pack directly: D0's `pack()` copies `PackRequest.text_search` into
     `ContextPack.text_search`.
4. **Tests** (`tests/rank_source_routing.rs`) cover:
   - a symbol question admits `tokenize` with `[SymbolQuestion]` at `Low`;
   - "what does the parser do" with no node named `parser` admits nothing;
   - a relationship query returns callers with `via` naming the seed and a site line
     (test `relationship_query_seeds_direct_neighbours`; acceptance runs it by name);
   - a literal query halves lexical scores and leaves an exact-path rung intact;
   - a general query's candidates equal today's for a fixed fixture (snapshot the
     ranked ids before your change, from the existing test fixtures).

## Traps

- The existing candidacy rule protects against prose false positives. `SymbolQuestion`
  admits only when the **whole query** matches the pattern, never on a substring.
- Relationship neighbours must not duplicate candidates already present. Merge
  reasons instead.
- Every file under 400 lines, and every function under 50.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::tests::rank_source 2>&1 | tail -15`

## Report

Report the files changed, the patterns as implemented, and any phrasing that surprised
you.
