# D2: explained, token-capped expansion, and weak-coverage caveats

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside D1 and D3. D0 added `RankedCandidate.via`,
  `ContextItem.explanation` and `ContextItem.caveat`.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 13 (the
  Explained neighbours, Token-capped expansion and Caveats bullets).

## Goal

Every graph neighbour carries the edge that introduced it. Expansion stops at
`MAX_EXPANDED_TOKENS` of rendered neighbour text. A source item from a weakly covered
file, or from a stale, never-built or unavailable snapshot, is ranked lower and says
why.

## Files you own (write)

- `loom/src/context/rank_source/expand.rs`
- `loom/src/context/pack/source_item.rs` (D0 created it): set `explanation` and
  `caveat` on source items
- `loom/src/context/pack.rs`: the caveat score factor, applied before selection. It
  lives in `pack.rs` only, where the pack's freshness is known
- tests `loom/src/context/tests/rank_source_explain.rs` and `pack_caveats.rs`. D0
  created and declared both; add to them, and never edit `tests/mod.rs`.

`expand.rs`, `pack.rs` and `pack/source_item.rs` are yours alone in wave 2.

Read-only: `rank_source.rs`/`routing.rs`/`intent.rs` (D1; you never edit
`rank_source.rs`), `rank.rs` and `rank/candidate.rs` (D0), the render and brief files
(D3), and `loom/tests/retrieval_delivery_contracts.rs` (frozen; it pins `via` on a
`GraphNeighbor` whose seed was an exact symbol).

## Steps

1. **`expand.rs`.**
   - `neighbours_for_seed` returns `(id, confidence, &SourceEdge, EdgeDirection)`
     instead of `(id, confidence)`.
   - `append_neighbours` sets `via` from that edge: the seed id, the edge kind, the
     direction relative to the neighbour, the provenance, and the first site's
     `line_start`.
   - D0 already added `pub const MAX_EXPANDED_TOKENS: usize = 300;` to `expand.rs` and
     `pub use expand::MAX_EXPANDED_TOKENS;` to `rank_source.rs`. Use the constant; do
     not add it again and do not edit `rank_source.rs`.
   - Expansion keeps a running total of each admitted neighbour's estimated rendered
     tokens (`estimate_tokens` over the rendered entry plus explanation text; use the
     same function `render.rs` uses, `rendered_item_tokens`, or its underlying
     estimator) and stops before exceeding the cap.
   - The count caps stay.
2. **Explanation text** (`pack/source_item.rs`). From `via`, render:
   - `calls \`seed\`` for an outgoing `Calls` edge, and `called by \`seed\` at path:L<n>`
     for an incoming one;
   - `references \`seed\`` and `referenced by \`seed\` at path:L<n>` likewise.
3. **Caveats.**
   - In `pack.rs` (never in `rank_source.rs`), a source node whose file coverage is
     not `Full` gets `caveat = Some("partial coverage")` and its score multiplied by
     0.6.
   - In pack assembly, when the pack's `semantic_freshness.state()` is not `Current`,
     every source item gets `caveat = Some(format!("snapshot {}", state.as_str()))`
     (combined with an existing caveat as `"partial coverage; snapshot stale"`), and
     its score is multiplied by 0.6 before selection.
4. **Tests.**
   - `tests/rank_source_explain.rs`:
     - `via` for callers and callees;
     - a neighbour's explanation names the seed and the site line;
     - expansion stops at the token cap on a fixture of 5 seeds with 4 callers each
       and long names;
     - the existing expansion tests pass.
   - `tests/pack_caveats.rs`:
     - a Partial-coverage node carries the caveat and ranks below an equal Full node;
     - a stale pack carries `snapshot stale` on every source item;
     - a Current pack carries none. `Freshness::default()` reads as `NeverBuilt`
       (empty revision), so a healthy fixture sets `revision: "test-rev".into()` (D0
       did this in the existing fixtures).

## Traps

- `MIN_NEIGHBOR_EDGE_CONFIDENCE` (0.5) stays. Candidate edges (0.2) are never
  followed.
- Today `withhold_partial_coverage` drops rungs for non-`Full` files. Keep it: the
  caveat adds explanation, and the 0.6 factor applies on top.
- Every file under 400 lines, and every function under 50.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::tests:: 2>&1 | tail -15`

## Report

Report the files changed, the caveat factor's call site in `pack.rs`, and the
explanation strings.
