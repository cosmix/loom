# D0: schema split and new retrieval fields

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 1, alone. D1–D3 fan out on your types in wave 2.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 13 in
  full.

## Goal

Make room in the files at the size cap, and add every new type and field the other
workers need, so that D1–D3 compile against a fixed surface. Behaviour stays unchanged
in this wave: new fields default to `None` or empty, and nothing sets them yet.

## Files you own (write)

- `loom/src/context/schema.rs` (394 lines): move `ContextItem`, `UnmetRequirement`,
  `ContextPack` and their impls into a new `loom/src/context/schema/pack_types.rs`,
  re-exported from `schema.rs` so every path still resolves
- `loom/src/context/pack.rs` (390 lines): move `build_source_item` and its helpers into
  a new `loom/src/context/pack/source_item.rs`
- `loom/src/context/retrieve.rs` (399 lines): move `build_pack_request` and
  `check_require_ids` into a new `loom/src/context/retrieve/request.rs`
- `loom/src/context/rank.rs` (398 lines, and it must end at or below 380): move
  `RankQuery` and `RankedCandidate` (struct and impl, with their strength helpers) into
  a new `loom/src/context/rank/candidate.rs`, add `NeighborVia` and `EdgeDirection`
  there, and `pub use` all four from `rank.rs`
- `loom/src/context/rank_source.rs`: only the `pub use expand::MAX_EXPANDED_TOKENS;`
  line. Do not declare `pub mod intent;` and do not create `intent.rs`: D1 creates
  `intent.rs` and declares it
- `loom/src/context/rank_source/expand.rs`: only
  `pub const MAX_EXPANDED_TOKENS: usize = 300;`

In wave 1 you also edit `pack.rs`, `pack/source_item.rs`, `retrieve.rs`,
`retrieve/request.rs`, `rank_source.rs` and `rank_source/expand.rs`, as listed. In wave
2, `rank_source.rs` and `retrieve/request.rs` are D1's alone, and `expand.rs`,
`pack.rs` and `pack/source_item.rs` are D2's alone.

- `loom/src/context/tests/mod.rs` gains six declarations, and you create the six
  files. The fixtures in `context/tests/source_fixtures.rs` are `pub(super)`, so wave-2
  tests must live in `context/tests/`, and three workers cannot share `mod.rs`. Each
  file starts with one real test of the unchanged behaviour, and the wave-2 owner adds
  to it:
  - `rank_source_intent.rs` (D1): `plain_prose_admits_no_symbol_question`, which ranks
    `read the remaining knowledge files` over a source fixture and asserts no candidate
    carries `SelectionReason::SymbolQuestion` (`classify` and `QueryIntent` do not
    exist until D1 creates `intent.rs`, so this test cannot name them);
  - `rank_source_routing.rs` (D1): `exact_symbol_candidates_carry_no_via`;
  - `rank_source_explain.rs` (D2): `seed_candidates_carry_no_via`, where a
    backticked exact symbol's own candidate has `via == None`;
  - `pack_caveats.rs` (D2): `items_built_today_carry_no_caveat`;
  - `retrieve_windows.rs` (D3): `stage_query_defaults_to_the_stage_brief_surface`;
  - `render_source_fields.rs` (D3):
    `an_entry_without_new_fields_renders_as_before`. It pins today's rendered source
    entry text for one fixture item, taken from `render_source_entry`'s current
    output.

Read-only: every other file except the literal and fixture sites in steps 3 and 4, and
`loom/tests/retrieval_delivery_contracts.rs` (frozen).

## Steps

1. **Split first** (the three moves above), with no logic change. Run
   `cargo test --manifest-path loom/Cargo.toml --lib context:: 2>&1 | tail -5` to
   confirm green before step 2.
2. **Add the types** exactly as design 13 names them:
   - `SelectionReason::SymbolQuestion`, whose `Display` is `"symbol-question"` and
     which `Confidence::from_reasons` maps to `Low` through its fallback;
   - `ContextItem.explanation: Option<String>`, `ContextItem.caveat: Option<String>` and
     `ContextItem.window: Option<String>`, all
     `#[serde(default, skip_serializing_if = "Option::is_none")]`;
   - `pub enum Surface { StageBrief, Cli, Hook }` in `retrieve.rs`, reached as
     `crate::context::retrieve::Surface`. Do not touch `context/mod.rs`: stage
     `edge-quality-eval` edits it in parallel. Add `StageQuery.surface: Surface`;
     `StageQuery::new` sets `StageBrief`; add
     `StageQuery::with_surface(self, Surface) -> Self`;
   - `ContextPack.text_search: Option<TextSearchHint>`, the same serde attributes;
   - `pub struct TextSearchHint { pub pattern: String, pub command: String }` in
     `schema/pack_types.rs`, re-exported from `schema.rs` so
     `loom::context::schema::TextSearchHint` resolves (D1 adds `for_pattern`);
   - `PackRequest.text_search: Option<TextSearchHint>`, and `pack()` copies it into
     `ContextPack.text_search` (D1 sets it in `build_pack_request`);
   - `RankedCandidate.via: Option<NeighborVia>` in `rank/candidate.rs`, with
     `pub struct NeighborVia` and `pub enum EdgeDirection { Incoming, Outgoing }` there.
     `RankedCandidate` derives `Debug, Clone, PartialEq`, so `NeighborVia` derives the
     same, and `EdgeDirection` derives at least those;
   - `pub const MAX_EXPANDED_TOKENS: usize = 300;` in `rank_source/expand.rs` and
     `pub use expand::MAX_EXPANDED_TOKENS;` in `rank_source.rs`.

   `QueryIntent`, `RelationDirection` and `classify` are D1's: `intent.rs` does not
   exist after your wave.
3. **Fix every struct literal** of `PackRequest`, `ContextItem`, `ContextPack`,
   `RankedCandidate` and `StageQuery` in the crate
   (`rg -n 'PackRequest \{|ContextItem \{|ContextPack \{|RankedCandidate \{|StageQuery \{' loom/src`)
   with the new fields set to `None` or `Surface::StageBrief`. The `PackRequest`
   literals include `retrieve.rs` and `context/tests/pack_fixtures.rs`, `pack_source.rs`
   and `pack_twins.rs`. These are mechanical edits in test files and builders, and every
   literal site is yours for this edit only.
4. **Healthy freshness fixtures.** `Freshness::default()` reads as `NeverBuilt`
   (empty revision), and caveats fire on any non-`Current` pack. Where a fixture builds
   semantic freshness with `Freshness::default()` and means a healthy pack, give it
   `revision: "test-rev".into()`. The files are `context/tests/pack_source.rs`,
   `pack_fixtures.rs`, `pack_twins.rs`, `delivery.rs`, `schema.rs`,
   `orchestrator/signals/format/brief_tests.rs` and
   `commands/hook/worker_brief/test_support.rs`; edit each only where it builds
   semantic freshness that way.

## Traps

- Keep every moved item's visibility and docs. `pub use` must re-export exactly the
  names that were public before.
- `Confidence::from_reasons` uses `matches!` with a `Low` fallback, so it needs no arm
  for `SymbolQuestion`. Only the `Display` match is exhaustive: add the arm there.
- Every file stays under 400 lines.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context:: 2>&1 | tail -10`

## Report

Report the files changed and the new line counts of `schema.rs`, `pack.rs`,
`retrieve.rs` and `rank.rs`.
