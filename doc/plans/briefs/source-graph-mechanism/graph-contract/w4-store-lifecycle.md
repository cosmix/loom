# W4: schema checks, corrupt layers, and worktree staleness

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside W2 and W5. W1 added `GraphLayer.schema_version`,
  `GRAPH_SCHEMA_VERSION` and the `layer_types.rs` split.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 2.

## Goal

A layer built before this plan, or one that fails to parse, is never served and never
wedges the cache. It is rebuilt. Worktree graphs stop trusting base entries whose
extractor identity is stale.

## Files you own (write)

- `loom/src/context/graph_store/mod.rs` (only `read_layer`, `load_base`,
  `load_newest_base` and their callers there), `graph_store/prune.rs`,
  `graph_store/fallback.rs`
- a new `loom/src/context/graph_store/tests_corrupt.rs`, declared from
  `graph_store/mod.rs`
- `loom/src/context/refresh/snapshot.rs` (`layer_is_current`, `ensure_base`,
  `overlay_is_current`)
- `loom/src/context/refresh/source_graph.rs` (`resolve_scope_layers`)
- `loom/src/context/refresh/source_graph/layer.rs` (`build_layer` previous-layer
  selection)
- a new `loom/src/context/refresh/tests_schema.rs`, declared from
  `loom/src/context/refresh.rs`. That declaration line is yours; no other edit to
  `refresh.rs`.
- `loom/src/context/worktree_graph.rs`, `worktree_graph_tests.rs`
- `loom/src/commands/run/tests/preflight.rs`: the `GraphLayer` literal in
  `test_preflight_silent_when_base_exists` (step 7)

Read-only: `loom/src/context/source_graph/**`, `loom/src/context/extract/**`, and
`loom/tests/graph_contract_contracts.rs` (frozen).

## Steps

1. **`read_layer`** (`graph_store/mod.rs`; W1 moves it, so locate it with
   `loom map --find-all read_layer`, never by line).
   - A `serde_json` parse failure returns `Ok(None)` after
     `tracing::warn!(path = %path.display(), error = %e, "discarding unparseable source graph layer")`.
   - A read error other than `NotFound` stays an `Err`.
   - Check every caller for a behaviour change:
     - `load_base`, `load_overlay`, `load_newest_base`;
     - `read_layer_or_memory` in `fallback.rs`.
2. **`layer_is_current`** (`refresh/snapshot.rs`): return false unless
   `layer.schema_version == GRAPH_SCHEMA_VERSION`, then keep the parser-version walk.
   - `ensure_base` already deletes and rebuilds a non-current base.
     Confirm it also replaces a base whose file was unparseable: `load_base` now
     returns `None` for it, and the rebuild must overwrite the file.
     `publish_base` refuses to overwrite an existing revision today (knowledge:
     "`GraphStore::publish_base` never overwrites a revision already published").
     Make the unparseable or stale-schema path remove the file before publishing, the
     same way `ensure_base` removes a non-current base.
3. **`overlay_is_current`** also requires the current schema.
4. **`build_layer` / `resolve_scope_layers`.** A previous layer or base with a stale
   `schema_version` is not a reuse source: treat it as absent, so every file is
   re-extracted. `load_newest_base` skips unparseable and stale-schema bases, never
   returning them.
5. **`build_worktree_graph`** (`worktree_graph.rs`). A base with a
   stale schema counts as missing. For every base entry kept, compare the first
   node's `parser_version` against `extractor_for(path)`'s current identity (the
   lexical version when there is no extractor), and re-extract the file from the
   worktree bytes on mismatch. This is today's gap: it trusted stale entries.
6. **Tests.**
   - `refresh/tests_schema.rs`, mirroring the temp-repo setup in
     `refresh/tests_snapshot.rs` (read how `map_answers_in_a_checkout_that_never_ran_init`
     builds its repo):
     1. a base written with `schema_version: 0` is rebuilt, never `Reused`;
     2. garbage bytes at the base path give `Rebuilt`, and the file parses afterwards;
     3. an overlay with a stale schema is rebuilt.
   - `graph_store/tests_corrupt.rs`:
     1. `load_newest_base` skips a corrupt newest base and returns the older valid
        one;
     2. `read_layer` on garbage returns `Ok(None)`.
   - `worktree_graph_tests.rs`: a base entry with a different `parser_version` is
     re-extracted.
7. **`commands/run/tests/preflight.rs`.**
   `test_preflight_silent_when_base_exists` publishes
   `GraphLayer { revision, ..Default::default() }`, whose `schema_version` is 0 and is
   never current after this stage. Set `schema_version: GRAPH_SCHEMA_VERSION` in that
   literal. This is not an assertion line, so it raises no test-integrity event.

## Traps

- `graph_store/mod.rs` must stay at or under 400 lines. W1 split it; re-count before
  and after your edits.
- A memory-fallback layer (`fallback.rs`) is never parsed from disk. Leave its
  semantics alone.
- Never delete a base layer for a revision other than the one being rebuilt.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::refresh:: 2>&1 | tail -15`

## Report

Report the files changed, and every caller of `read_layer` whose behaviour changed.
