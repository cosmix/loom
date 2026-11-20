# V2: persist the view, materialize it per snapshot, and switch readers

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, after V1.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 12 (the
  persistence and lifecycle bullets) and section 10 (the `snapshot` JSON keys
  `resolver_version` and `view`).

## Goal

Views are written once per snapshot, loaded by identity, rebuilt when corrupt or
mismatched, and pruned with their bases under a byte budget. `loom map`, the knowledge
bootstrap and worktree graphs (`reachable`, impact-selected tests) read views instead
of resolving per process. Cold builds report bytes read.

## Files you own (write)

- new `loom/src/context/view/store.rs`: `GraphStore::view` and `GraphStore::view_path`,
  in an `impl GraphStore` block in this file, so `graph_store/mod.rs` stays under
  400 lines. V1 owns `view/mod.rs`; you add only its `mod store;` and
  `#[cfg(test)] mod tests_store;` lines there, after V1 has finished
- `loom/src/context/graph_store/mod.rs`: only the `GraphStore` fields the view needs
  (`view_fallback` and the in-process view cache, step 1) and their lines in
  `GraphStore::new`. Use the existing `pub` `base_dir()` and `overlay_dir(plan, stage)`
  for paths; add no `graph_root()` or `overlay_root()` accessor
- `loom/src/context/graph_store/prune.rs` (views pruned with their base; byte budget)
- `loom/src/context/graph_store/fallback.rs`: extend the memory fallback to views
- `loom/src/context/config.rs`: the `RetrievalConfig.graph_cache_budget_bytes` field
  (step 3)
- `loom/src/context/refresh.rs`: the `tests_view_lifecycle` declaration only
- `loom/src/context/refresh/snapshot.rs` and `refresh/snapshot/describe.rs`:
  materialize after publish or reuse; `describe` prints `bytes_read`
- `loom/src/context/refresh/source_graph.rs` and `refresh/source_graph/layer.rs`: the
  `bytes_read` counter
- `loom/src/commands/map.rs`: `load_graph` reads the view; add the snapshot JSON keys
  and a `view` timing phase
- `loom/src/commands/knowledge/bootstrap/graph.rs`
- `loom/src/commands/clean/base_graphs.rs`: `loom clean` removes and counts views
- `loom/src/context/worktree_graph.rs`
- tests: new `loom/src/context/view/tests_store.rs`,
  `loom/src/context/refresh/tests_view_lifecycle.rs`, and updates to
  `worktree_graph_tests.rs`

Read-only: `context/view/{mod,build,incremental,index,identity,deps}.rs` (V1, apart
from the two lines above) and `loom/tests/resolved_view_contracts.rs` (frozen). The
frozen file pins that a garbage view file is rebuilt, not served, through
`GraphStore::view`.

## Steps

1. **`view/store.rs`.**
   - `view_path(identity, overlay)` returns, built from the existing `pub` `base_dir()`
     and `overlay_dir(plan, stage)`:
     - base: `<cache>/graph/view/<base_revision>-<identity.digest12()>.json`;
     - overlay: `overlay_dir(plan, stage)` joined with `view.json`.
   - The memory fallback gains a `view_fallback: RefCell<HashMap<PathBuf, ResolvedView>>`
     field (plus its line in `GraphStore::new`). `fall_back_to_memory`,
     `read_layer_or_memory` and `is_write_denied` widen to `pub(crate)` so
     `view/store.rs` can use them.
   - `view(revision, overlay)`:
     1. compute `ViewIdentity::current(revision, overlay_generation)`, where the
        generation comes from the overlay layer (empty for a base);
     2. return the in-process cached view when its identity matches (see the parse
        budget below); else try the persisted file. Parse failure or identity
        mismatch counts as a miss, after a `tracing::warn!`;
     3. on a miss for a base, relink from the newest older base's view that loads, or
        `build_cold` from `resolved(revision, None)`;
     4. on a miss for an overlay, relink from the base view with the overlay applied
        (`GraphStore::resolved(revision, overlay)` gives the `next` graph);
     5. persist with `locked_write` (`canonical_bytes`), falling back to memory on a
        permission-denied or read-only write, exactly like layers (`fallback.rs`).
   - Never persist a view when `load_base(revision)` is `None` (the empty graph
     `resolved()` returns for a missing base): build in memory and return it. Test
     `missing_base_view_is_never_persisted` (in `tests_store.rs`): `view` on a missing
     base, then `publish_base`, then `view` has nodes > 0. `publish_base` removes
     `graph/view/<revision>-*.json` for the revision it publishes.
   - Parse budget: one `loom map` process deserializes the view at most once.
     `ensure_snapshot` stores the view it materialized in the `GraphStore`'s
     in-process view cache, and `GraphStore::view` returns that entry when the identity
     matches.
   - `pub(crate) fn load_view(&self, identity, overlay)` loads a persisted view and
     never writes. `worktree_graph.rs` uses it (step 5).
2. **Materialize.** After `ensure_snapshot` publishes or reuses a layer, call
   `graph_store.view(...)` once, so later readers hit the persisted file. A failure
   there is advisory: add it to the outcome's reason. It never fails the snapshot.
   - Materialize the base view and, whenever `ensure_snapshot` wrote or reused an
     overlay layer, the overlay view too, so the prompt hook never builds and persists
     a view on its hot path.
   - On materialize, delete the sibling `graph/view/<revision>-*.json` files whose
     identity digest differs from the one just written (a `RESOLVER_VERSION` bump would
     otherwise leave orphans until the revision is pruned).
   - Concurrent builders of one view are allowed: the bytes are deterministic,
     `locked_write` takes a directory lock plus an atomic rename, and the last write
     wins.
   - `discard_overlay` removes the overlay's `view.json` together with its
     `graph.json`; the lifecycle test asserts it.
3. **Prune.**
   - `prune_base_graphs` also removes every `view/<revision>-*.json` of a pruned
     revision.
   - Add the budget prune: after the count prune, remove the oldest unprotected
     revisions (base plus views) until `graph/base` + `graph/view` fit the budget. The
     budget is `RetrievalConfig.graph_cache_budget_bytes` (`context/config.rs`, default
     2 GiB), read the way `prune.rs` reads `keep_base_graphs`. It applies per main
     cache (`<main>/.loom/cache/context-v1`), shared by every worktree.
   - Call it from `prune_after_publish`, where `publish_base` runs the count prune.
     Test `prune_enforces_byte_budget_oldest_first` (in `tests_store.rs`) drives
     `publish_base` with a 1 KiB budget and asserts the older base and its views are
     evicted, which also proves `prune_after_publish` calls the budget prune.
   - `loom clean` (`commands/clean/base_graphs.rs`) removes `graph/view` entries with
     their base and counts their bytes. It does not return early when `graph/base`
     is empty but `graph/view` is not.
4. **Counters.**
   - `SourceGraphCounters` gains `bytes_read`, summed in `layer.rs::read_bytes`, and
     `SnapshotOutcome::describe` prints it.
   - `loom knowledge sync --json` already serializes the counters. Confirm the new key
     appears there; no other edit.
5. **Readers.**
   - `commands/map.rs::load_graph`: `graph_store.view(&snapshot.revision, overlay)`
     replaces `resolved()` plus `resolve_graph`.
     - The JSON snapshot gains `resolver_version` and `view`: `"materialized"` when the
       file was loaded, `"built"` otherwise.
     - `--timings` gains `view` and keeps `snapshot`, `load`, `resolve`, `query`,
       `render`, `total` and `peak_rss_kb`, all numeric: the map-timings contract still
       runs in this stage's acceptance.
   - `commands/knowledge/bootstrap/graph.rs`: same swap.
   - `context/worktree_graph.rs::build_worktree_graph` stays read-only:
     - load the nearest base's view through `GraphStore::load_view`, which never
       writes;
     - apply the changed files as `next`;
     - `relink` in memory, never persisting.
     - Without a base view, fall back to `build_cold`.
   - Afterwards no file outside `context/view` names `resolve_graph`, even in a
     comment or a `use` line: the stage's after check is
     `rg -w resolve_graph loom/src/commands/map.rs loom/src/commands/knowledge/bootstrap/graph.rs loom/src/context/worktree_graph.rs`,
     which must print nothing. Other `resolve_graph` call sites are `context/view/**`,
     `context/resolve*`, and tests.
6. **Tests.**
   - `view/tests_store.rs`:
     - a persisted view round-trips;
     - an identity mismatch is rebuilt;
     - garbage bytes are rebuilt;
     - the memory fallback serves a view when writes are denied (reuse the fallback
       test seam in `graph_store/fallback_tests.rs`).
   - `refresh/tests_view_lifecycle.rs`:
     - `ensure_snapshot` writes a view file for the base;
     - a second `loom map`-equivalent load reads it (assert
       `view == "materialized"` through the API, not the CLI);
     - prune removes views with bases;
     - `discard_overlay` removes the overlay's `view.json`;
     - `counters.bytes_read > 0` after a cold build;
     - the byte budget evicts oldest first and never the protected revision (the
       exact-named test `prune_enforces_byte_budget_oldest_first` lives in
       `tests_store.rs`, as step 3 says).
   - `worktree_graph_tests.rs`: a worktree graph relinked from the base view equals a
     cold build of the same files.
   - Record `loom map --find-all run_views --timings` cold, then warm, in a test repo
     with `loom memory note`. A warm run in this repository takes about 1.66 s today;
     record a warm run slower than that as a concern memory. Inside a stage sandbox
     the shared cache is read-only, so an in-stage `loom map` reports
     `persisted: false` and `view: "built"`: take the warm-query numbers from a TempDir
     or host run only. Your report and post-merge operator check 2 are the sources.

## Traps

- `graph_store/mod.rs` is near 400 lines: put new code in `view/store.rs` and
  `prune.rs`, and touch `mod.rs` only for the fields and their `GraphStore::new` lines.
- The view file can be large. Write it only when missing or mismatched, never on every
  `loom map` call.
- Never serve a view whose identity differs from the requested one.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::view:: 2>&1 | tail -15`

## Report

Report the files changed, the measured `--timings` of `loom map --find-all` in a test
repo (cold, then warm), and the remaining `resolve_graph(` call sites.
