# V1: resolved view, indexes, and incremental relink

- **Agent type / tier:** `loom-senior-software-engineer` (opus)
- **Wave:** 1, alone. V2 persists the view and wires its consumers in wave 2.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 7.0
  and 12 in full, and section 3 for the edge fields that canonical equality covers.

## Goal

`context::view` builds a `ResolvedView` (identity, resolved graph, stats, dependency
index; no persisted name or adjacency index, design 12) cold, or incrementally from a previous view. The
incremental relink is proven equal to the cold build, byte for byte in canonical JSON,
for every scripted change.

## Files you own (write)

- new `loom/src/context/view/mod.rs`, `identity.rs`, `build.rs`, `incremental.rs`,
  `deps.rs`
- new tests `loom/src/context/view/tests.rs` and `view/tests_equivalence.rs`
- `loom/src/context/mod.rs`: the `pub mod view;` line only
- `loom/src/context/graph_store/layer_types.rs`: the `ResolvedGraph` derive line only
  (add `Serialize, Deserialize`; `FileEntry` already has serde)

You own `view/mod.rs`. After you finish, V2 adds only its `mod store;` and
`#[cfg(test)] mod tests_store;` lines there.

Read-only:

- `loom/src/context/resolve*` (the recording API from design 7.0),
  the rest of `loom/src/context/graph_store/**`, `loom/src/context/source_graph/**`,
  `loom/src/context/extract/**`, and `loom/src/context/store.rs` (for `canonical_json`);
- `loom/tests/resolved_view_contracts.rs` (frozen). Read it first: it pins
  `build_cold`, `relink`, `canonical_bytes`, `ViewIdentity { .. }` field names, and one
  equivalence case (a namesake added in a new file makes a `UniqueName` binding in an
  unchanged file ambiguous).

## API (exact, design 12)

`RESOLVER_VERSION`, `ViewIdentity` (with `ViewIdentity::current`),
`DependencyIndex`, `ResolvedView`, `build_cold`, `relink` and `canonical_bytes`, all
public under `loom::context::view`. `GraphStore::view` and `view_path` are V2's.

- `ResolvedGraph` derives `Serialize` and `Deserialize`. `ResolutionStats` and
  `EdgeRef` carry serde from stage `binding-resolver`.
- `ResolvedView` gains `#[serde(skip)] pub origin: ViewOrigin`, with
  `enum ViewOrigin { Materialized, Built }`. `canonical_bytes` never sees it; it
  carries `loom map`'s `"view": "materialized" | "built"`.

## Steps

1. **`identity.rs`.**
   - `ViewIdentity` derives `Serialize`, `Deserialize`, `Clone`, `PartialEq`, `Eq` and
     `Debug`.
   - `current()` computes `extractor_digest` from `extract::registry()` as design 12
     says (`"{dialect}={parser_version}"` lines, sorted, `sha256`).
   - Add `pub fn digest12(&self) -> String`, the first 12 hex digits of `sha256` over
     the canonical JSON of the identity. V2 uses it in file names.
2. **No `index.rs`.** The view persists no name or adjacency index (design 12).
3. **`build.rs`: `build_cold`.** Clone the graph, run `resolve_graph_recording`, invert
   the keys into `DependencyIndex` (key to sorted `EdgeRef`s), and compute stats with
   `resolution_stats`. `build_cold` and `relink` each increment a `#[cfg(test)]`
   thread-local resolution-run counter, read by
   `#[cfg(test)] pub(crate) fn resolution_runs() -> usize`.
4. **`incremental.rs`: `relink`.**
   - Identity gate first: relink only when `previous.identity` has the same
     `schema_version`, `extractor_digest` and `resolver_version` as `identity`.
     Otherwise return `build_cold(next, identity)`.
   - Diff `previous.graph.files` against `next.files` by `content_hash`: changed,
     added, removed.
   - Assemble the working graph:
     - take unchanged files' entries from `previous`, which carry resolved edges;
     - take changed and added entries from `next`, which carry extraction-time edges;
     - drop removed files;
     - take the header (`base_revision`, `overlaid`) from `next`, never from
       `previous`.
   - Compute the touched keys: `touched_keys(old entry, removed)` and
     `touched_keys(new entry, added)` for every changed, added or removed file.
   - Select edges to re-resolve (design 12):
     - every edge in changed and added files;
     - every edge in unchanged files whose recorded keys (from
       `previous.deps`) intersect the touched keys;
     - every edge in unchanged files bound to, or listing as a candidate, a node in a
       changed or removed file.
   - Extraction-time candidate sets (an unresolved `Syntax` edge with non-empty
     `candidates` at extraction, binding-resolver R2's rule) are never unbound or
     re-resolved: exclude them from selection.
   - Call `unbind()` on each selected edge in an unchanged file, then run
     `resolve_edges` on the selected set.
   - Rebuild the dependency index: keep the previous keys of unselected edges,
     re-indexed if file ordinals moved (they are keyed by path, so they did not), and
     use the new keys of re-resolved edges. After removing the `EdgeRef`s of changed,
     removed and re-resolved edges, drop every key whose list is empty: a cold build
     never has an empty key.
   - Recompute stats with `resolution_stats`.
5. **`canonical_bytes`.** `canonical_json` of the whole view. Every map is a
   `BTreeMap`, and every list is sorted or in a defined order, so the cold and
   incremental builds serialize identically when equal.
6. **Tests.**
   - `view/tests.rs`: index lookups, candidate incoming lists, and an identity digest
     that is stable and changes with `RESOLVER_VERSION`.
   - `view/tests_equivalence.rs`: build graphs by extracting in-memory files through
     `extract_file(&registry(), ...)` plus `FileEntry::from_extraction` (the pattern in
     the frozen contract file). Script at least these changes, each asserting
     `canonical_bytes(relink(cold(G0), G1)) == canonical_bytes(cold(G1))`:
     1. add a namesake definition in a new file;
     2. delete a file holding a bound target;
     3. rename a file (remove plus add);
     4. change an import path so a binding moves;
     5. add a glob import that refuses a previous `UniqueName`;
     6. edit a file without changing its definitions;
     7. add a same-family definition that turns a candidate set into a unique bind.

     Also run a seeded pseudo-random edit sequence over a 12-file Rust/TypeScript/Python
     mix: 50 steps of add, remove, rename or edit, with a fixed seed and no new
     dependency (a small LCG). After each step, compare relink-from-previous with the
     cold build.

     Acceptance runs these four tests with `--exact`, so give them these names in
     `tests_equivalence.rs`:
     - `seeded_edit_sequence_relink_equals_cold`: the 50-step seeded sequence;
     - `same_file_ambiguity_survives_relink`: a same-file ambiguous call plus 9
       namesakes across the graph; edit one namesake file, and the extraction-time
       candidate set stays;
     - `namespace_change_reselects_import`: hand-built C# entries in which a
       changed (not added) file starts declaring namespace `A.B`; the `ns:` key
       reselects the import;
     - `relink_takes_header_from_next`: a non-empty `overlaid` and a different
       `base_revision` in `next` appear in the relinked graph.

     Add a test that a `previous` view with a different `resolver_version` (or
     `extractor_digest` or `schema_version`) makes `relink` equal `build_cold`.

## Traps

- `relink` must never rely on edge index stability in a changed file; those entries
  are fresh. Indices in unchanged files are stable because their entries are copied.
- A bound edge in an unchanged file whose target file was removed must be unbound
  even when no recorded key matched. The third selection rule covers it; test it.
- Performance is secondary to equality. Never skip re-resolution to save time.
- Every file under 400 lines and every function under 50.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::view:: 2>&1 | tail -15`

## Report

Report the files changed, the final API, and the selection rules as implemented (so V2
can document them).
