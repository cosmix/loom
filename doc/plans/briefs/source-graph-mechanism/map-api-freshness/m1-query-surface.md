# M1: direct neighbours with call sites, and candidate-aware impact

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 1, alongside M2 and M3. M4 wires your API into the CLI in wave 2.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 3.1,
  3.2, 3.3 and 10 (the `--callers`/`--callees`/`--references`/`--impact` rows).

## Goal

`direct_callers` and `direct_callees` return direct `Calls` edges only, with their
call sites and written symbol. They also include candidate edges, flagged. A new
`direct_references` does the same for `References`. `impact_with` follows candidate
sets at `AMBIGUOUS_CANDIDATE_CONFIDENCE` and filters by provenance class.

## Files you own (write)

- `loom/src/context/resolve/neighbors.rs`, `resolve/impact.rs`
- `loom/src/context/resolve/tests_neighbors.rs`, `tests_impact.rs`
- new `loom/src/context/resolve/tests_candidates.rs`, declared from `resolve.rs`. That
  declaration line and the `pub use` line for `direct_references` are your only edits
  to `resolve.rs`.
- `skills/loom-plan-writer/references/v2-contracts.md`: the `reachable` paragraph only
  (step 5 names the sentence to replace)

Read-only: `resolve/rules.rs` and the other resolver files; `verify/goal_backward/reachable.rs`
and `verify/impact_tests.rs` (callers of `impact_with`; confirm they compile);
`loom/tests/map_api_contracts.rs` (frozen).

## API (exact; M4 codes against it)

```rust
pub struct Neighbor {
    pub id: String, pub kind: SourceNodeKind, pub path: String,
    pub edge_kind: SourceEdgeKind, pub confidence: f32, pub provenance: EdgeProvenance,
    pub line_start: Option<usize>,          // the neighbour's declaration line
    pub symbol: String,                     // spelling written at the site
    pub sites: Vec<Span>,                   // call/reference sites, in the caller's file
    pub site_path: String,                  // path of the file holding the sites (the caller's file)
    pub candidate_of: Option<usize>,        // Some(n) when this row comes from an n-member candidate set
}
pub fn direct_callers(graph: &ResolvedGraph, node_id: &str, limit: usize) -> (Vec<Neighbor>, usize);
pub fn direct_callees(graph: &ResolvedGraph, node_id: &str, limit: usize) -> (Vec<Neighbor>, usize);
pub fn direct_references(graph: &ResolvedGraph, node_id: &str, limit: usize) -> (Vec<Neighbor>, usize);
pub struct ImpactOptions { /* existing fields (max_depth, kinds, limit, path_prefix, min_confidence) + */
                           pub provenances: Vec<EdgeProvenance>, /* empty = all */ pub follow_candidates: bool /* default true */ }
pub struct ImpactHit { /* existing fields + */ pub via_candidates: bool }
pub struct ImpactResult { /* existing fields + */ pub filtered_out: usize } // the count `path_prefix` removed
```

Keep the current return shapes, meaning the suppressed-count tuple if it exists today;
read `neighbors.rs` first and extend it, never duplicate it.

`ImpactOptions` keeps `limit` and `path_prefix` with today's post-traversal semantics
(`impact.rs`, after `walk.finish()`). `filtered_out` is the number of hits `path_prefix`
removed; M4 renders it as design 10's `filters.path.filtered_out`.

## Steps

1. **`neighbors.rs`.**
   - `is_neighbor_kind` splits:
     - callers and callees use `Calls` only;
     - references use `References` only.
   - Callers of X are every edge with `to == X`, plus every edge whose `candidates`
     contain X (`candidate_of = Some(len)`, confidence `AMBIGUOUS_CANDIDATE_CONFIDENCE`,
     provenance `Syntax`).
   - Callees of X are the edges from X, plus each candidate of an edge from X.
   - Rows carry the edge's `symbol`, its `sites`, and the path of the edge's
     `FileEntry`.
   - Top-level calls (`from` is a file node) stay excluded from callers, as today;
     document it.
   - Sort by confidence descending, then id, then the first site's line.
2. **`impact.rs`.**
   - `reverse_adjacency` adds candidate edges when `follow_candidates`.
   - `edge_is_allowed` also filters by `provenances` when non-empty.
   - The trust of a step through a candidate is `AMBIGUOUS_CANDIDATE_CONFIDENCE`, and
     the hit records `via_candidates = true` when its path used one.
   - `ImpactOptions::default()` keeps `max_depth: 3`, the four semantic kinds,
     `limit: 0`, `path_prefix: None`, `min_confidence: 0.0`, `provenances: vec![]` and
     `follow_candidates: true`.
   - `path_prefix` filtering stays after `walk.finish()`; count the hits it removes
     into `ImpactResult.filtered_out`.
   - The `impact()` wrapper (all six kinds) is unchanged apart from following
     candidates.
3. **Callers of `impact_with`.** Confirm `verify/goal_backward/reachable.rs` and
   `verify/impact_tests.rs` still compile. Graph-contract W1 already rewrote every full
   `ImpactOptions` literal (in `map/views/json.rs`, `map/views/mod.rs` and
   `verify/goal_backward/reachable.rs`) to `..Default::default()`, so your new fields
   compile. Those files are read-only for you. If `cargo build --all-targets` still
   names an `ImpactOptions` literal, report it to the main agent, who briefs the
   fix; do not leave the lib broken for M2 and M3.
4. **Tests.**
   - Callers include a candidate row with `candidate_of == Some(2)`.
   - Callers carry two sites for a repeated call.
   - `References` edges appear only in `direct_references`.
   - Impact reaches a node through a candidate at trust 0.2 with `via_candidates`.
   - `provenances: vec![Import]` excludes a `UniqueName` edge.
   - `follow_candidates: false` excludes the candidate path.
   - `path_prefix` removes hits after traversal and reports the count in
     `filtered_out`.
   - Update the existing tests in `tests_neighbors.rs`/`tests_impact.rs` only where
     callers semantics changed (References and Implements are no longer callers), and
     list each change in your report.
5. **Doc.** In `skills/loom-plan-writer/references/v2-contracts.md`, replace the
   sentence beginning "The walk follows resolved edges only" (in full: "The walk
   follows resolved edges only, so a path through dispatch the extractor cannot
   resolve reads as unreachable") with the current behaviour. Locate it with
   `rg -n -F 'The walk follows resolved edges only'`. The stage's before and after
   checks pin it: the phrase must be gone afterwards. The new text says the walk
   follows bound edges and ambiguous-candidate edges; a candidate step trusts 0.2; set
   `min_confidence` above 0.2 to require bound edges throughout; a path through
   dispatch the extractor cannot bind or list as a candidate reads as unreachable. The
   new text must contain the phrase "above 0.2" verbatim: integration-verify checks it.

## Traps

- Edge sites belong to the file of the edge's `FileEntry` (design 3.2): take
  `site_path` from the map key, never from the neighbour node.
- Keep `resolve/impact.rs` and `neighbors.rs` under 400 lines each, and functions
  under 50.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::resolve:: 2>&1 | tail -15`

## Report

Report the files changed, the final signatures, and any `ImpactOptions` literal sites
outside your files.
