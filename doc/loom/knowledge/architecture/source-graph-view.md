# Source Graph View

> Resolved view, relink

`context/view/` (`mod.rs`, `identity.rs`, `build.rs`, `incremental.rs`, `deps.rs`, `store.rs`) holds a
persisted, versioned resolved graph and the incremental relink that keeps it cheap. Nothing outside
`context/view` calls `resolve_graph`: the readers (`commands/map.rs`,
`commands/knowledge/bootstrap/graph.rs`, `context/worktree_graph.rs`, retrieval) ask
`GraphStore::view(revision, overlay)`, which loads, relinks or builds as needed. Resolution rules are
in [Source Graph Resolution](source-graph-resolution.md).

## Identity and Shape

`ResolvedView { identity, graph: ResolvedGraph, stats: ResolutionStats, deps: DependencyIndex,
origin }`. `ViewIdentity { schema_version, base_revision, overlay_generation, extractor_digest,
resolver_version }`; `ViewIdentity::current` fills the current schema, `RESOLVER_VERSION`
(`identity.rs`, currently 3) and the digest. `extractor_digest` is a `sha256` over the sorted
`{dialect}={parser_version}` lines of the registered extractors **plus a `lexical=<LEXICAL_PARSER_VERSION>`
line**: gap and unknown-type files are stamped with the lexical version, and relink copies unchanged
entries on an equal digest, so a lexical bump must change identity or relink would differ from a cold
build. `ViewOrigin` (`Materialized` | `Built`, default `Materialized`, no serde) says whether the view
was read from disk; `loom map` prints it as `"view"`. The view persists no name or adjacency index:
queries scan the parsed graph in one linear pass, and a persisted index would grow the JSON every warm
query parses. The storage-engine choice waits on `--timings` data
([Known Gaps](../concerns/source-graph-known-gaps.md)).

## Relink Equals Cold Build

`relink(previous, next, identity)` returns `build_cold(next, identity)` unless `previous.identity` has the
same `schema_version`, `extractor_digest` and `resolver_version` (`relinkable_from`). Otherwise it starts
from `previous` for files whose `content_hash` is unchanged and from `next` for changed, added and removed
files, takes `base_revision` and `overlaid` from `next`, then unbinds and re-resolves:

- every edge originating in a changed or added file;
- every edge whose consulted keys intersect `touched_keys` of the old and new entries of every changed,
  added or removed file;
- every edge bound to, or listing as a candidate, a node in a changed or removed file.

An edge extraction already decided (a bind, or a same-file candidate set) is never unbound: relink tells
it from a resolver-made edge by reading `next`'s entry of the unchanged file at the same edge index and
reopens only edges extraction left as a `Syntax` gap with no candidates. After removing the `EdgeRef`s of
re-resolved edges it drops every dependency key whose list is empty; a cold build never has an empty key.
Stats always come from `resolution_stats`, never from summing partial runs.

**`canonical_bytes(relink(..)) == canonical_bytes(build_cold(..))` for every change** is the invariant. A
seeded property test (`view/tests_equivalence.rs`) and a contract test pin it; the property covers only Rust,
TypeScript and Python direct imports (see [Review Backlog](../concerns/source-graph-review-backlog.md)).
Copying an old entry is sound only because the identity gate proves the same extractor and resolver produced it.

## Persistence and Currency

- A base view is `<cache>/graph/view/<revision>-<identity digest12>.json`; a stage or local overlay view is
  `<work>/context/<plan>/<stage>/view.json`. Writes use `locked_write`; a write-denied cache falls back to
  memory (`view_fallback`) exactly as layers do.
- A view whose identity differs from the request, or that fails to parse, is rebuilt and never served. A
  view is persisted, and used as a relink seed, only when every entry of its graph passes
  `entries_are_current` (`refresh/source_graph/layer.rs`): a view over a stale-served older-extractor base
  would otherwise carry a current identity and relink would copy its retired entries. No view is persisted
  for a revision with no base layer (`resolved()` leaves `base_revision` empty for a missing or unparseable
  base); `publish_base` removes `graph/view/<revision>-*.json`.
- An overlay view is current only when `view.json` is not older than the overlay layer file
  (`GraphStore::layer_modified`), and `ensure_snapshot` discards the overlay view whenever it wrote a layer,
  which also defeats a coarse-clock same-tick rewrite. Materializing a view deletes sibling views of the
  revision whose identity digest differs.
- `ensure_snapshot` materializes the base view after publishing or reusing a layer (only when the view is
  missing, checked with `is_file`, never parsed) and the overlay view when it wrote or reused an overlay. A
  base view relinks from the newest older base view, an overlay view from its base view. One `loom map`
  process parses a view at most once through the `GraphStore` in-process view cache.
- Pruning removes a base's views with the base and enforces `RetrievalConfig.graph_cache_budget_bytes`
  (default 2 GiB) over `graph/base` plus `graph/view`, oldest unprotected revision first, per main cache.
  `loom clean` (`commands/clean/base_graphs.rs`) removes views with their base. The worktree graph stays
  read-only: `load_view` never writes and `worktree_graph` relinks in memory.
- `SnapshotIdentity.built_at` is the layer file's mtime (zero cost; reading the layer would re-parse the
  ~100 MB base), so it is null for a layer that lives only in memory. `--timings` splits `view` into load or
  resolve by `ViewOrigin`; a view built inside `ensure_snapshot` is handed over as `Built`, so its resolve time
  is counted under `snapshot`.
