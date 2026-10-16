# Source Graph Review Backlog

> Unimplemented reviewer suggestions

Reviewer suggestions from the source-graph plan that no stage implemented. Each is a small local change or a
test to add; none blocks the contract. Larger limits are in [Known Gaps](source-graph-known-gaps.md).
Anchors are `path:line` at merge time; locate by symbol (`loom map --find-all`).

## Resolver and Path Conventions

- `resolve/paths/rust.rs` `package_named` reads any one-segment external import from `tests/`, `benches/` or
  `examples/` as `crate::<seg>`, so every such `use` gets an `Import` edge to `lib.rs` and the rule 4/6 external
  refusal never fires there; a test should pin the trade-off. `rules.rs::placed()` rejects a package-name first
  segment, so `mycrate::Widget::new()` without a `use` in those directories is a gap; consider accepting what
  `package_named` would.
- `paths/rust.rs` `unanchored` should read a non-anchored path against the citing file's own crate root only, so
  `use log::info;` in one workspace crate cannot match `crates/a/src/log.rs`.
- `paths/python.rs`: a one-segment spec is suffix-matched anywhere (record the trade-off in a test); `from . import
  helper` resolves only to `__init__.py`, so a namespace package refuses instead of falling through to name matching
  (pin the intended behaviour). `paths/go.rs`: a slashed stdlib path can suffix-match a local directory.
- `bindings.rs`: the bare-member fallback in `through` also applies to a named import whose item is a value (a TS
  `export const Logger = {...}` can bind a same-named top-level function); it is needed for Rust `use a::b;`, so add
  a test and consider limiting it. `prototyped()` lifts the system-include refusal, so a single `static` namesake in
  another file binds `UniqueName` although a `static` is not linkable across files (the confidence is low; the bind
  is wrong).
- `rules.rs`: narrow scope-matched ids to the files the dropped prefix names so `crate::a::Widget::new` binds
  the matching file's `Widget::new` instead of listing every `Widget::new`. A comment there names `RESOLVER_VERSION`; keep it in
  step with `view/identity.rs`.
- `receivers.rs`: `traits_used` is not gated to PHP, so any dialect that later emits `References` edges from type
  nodes would make `self.m()` bind to an unrelated type's member at `Receiver` confidence; Rust and C++ rule 3 match
  any same-named type in the crate or family (prefer ids whose full scope equals the caller's).
- `record.rs`: `name_key` splits file names on `.`, so every file node shares `name:{family}:rs` (`:h`, `:py`), and any
  change to a file of that family reopens every edge that read another file's entry (re-export hops, prototype
  checks); `family_of` duplicates `paths/mod.rs`, and `PACKAGE_FAMILIES` duplicates the go/java/csharp/php arm of
  `PathIndex::package_files` (let it return an `Option`).
- `resolve.rs`: `resolve_edges` with an empty edge set still builds both indexes over the whole graph (return early so
  a no-op relink is free); `retarget` is a one-line wrapper over `SourceEdge::bind`; `impact()` and
  `EdgeProvenance::rank` are used only by tests (remove, or say why they stay).
- `source_graph/edge.rs`: `bound()` debug-asserts against `Structural` and `Syntax` only; add `Compiler` so no call can
  mint a 1.0 edge that is not containment.
- Resolver tests to add: a multi-match relative ES import (`an_import_resolves_only_when_exactly_one_file_matches`
  checks only the unique and no-match cases), `ns:` keys on a C# or PHP call and the rule 2 and 3 key sets, the
  re-export hop limit (a cycle terminates unbound), `prototyped()` being C-family only, the `item_module` fallback
  not binding when the parent module lacks the item, a C# partial class or forward-declared C++ class staying
  ambiguous, and that `src/g.go#function:helper` exists in `names_never_bind_across_families` (else the test passes
  vacuously).

## Extraction and Identity

- `extract/treesitter/ids.rs::disambiguate` is O(k²) per duplicate group (two filter scans per member); a running
  ordinal in a `HashMap` makes it O(k). A 512 KiB file with ~25k identical declarations is ~1e9 string compares.
- `collect.rs`: receiver text is the raw expression and part of the dedupe key, so chains such as
  `expect(a).toBe(1)` store O(n²) text and stop equal callees merging sites; keep only identifier or path receivers
  or cap the length.
- `binding.rs`: `receiver_type` walks through a nested JS/TS `function`
  declaration to the class, so `this.x()` inside a non-arrow nested function (which rebinds `this`) binds `Receiver`
  to `Class::x`; stop the walk at a nested `function`.
- `treesitter/mod.rs`: the `import_bindings` doc must say `path` is the `import_spec` result (Ruby `./x`, PHP full
  group path) and needs rewrapping; `dialect_by_id(node_language.as_str())` is looked up in both `self_receivers()`
  and `binding_rules()`, and each extractor repeats its dialect literal in `ExtractorIdentity` and `dialect_by_id`
  (pinned only by a test). The query cache key is `(NodeLanguage string, query text)` without the grammar, sound while
  every dialect has unique query text; `Query::new` runs under the global mutex (about 12 × 39 ms once per process);
  no test asserts that two `run_query` calls share one `Arc<Query>` or that a bad query is not cached.
- `php.rs`: `imports::clauses(statement)` is re-parsed in `import_spec` and `import_bindings` for every clause
  (quadratic for large `use A\{...}` groups). `Capabilities.references` stays false for php, c and cpp although they
  emit `References` edges, and Go reports `receivers: false` though its query captures `@call.receiver`; the flag is
  never read but misdescribes the extractor.
- `c.rs`: the shared prototype pattern may match a C++ local direct-init (`Foo w(x);`) if the grammar parses it as a
  function declarator; add a C++ extractor test. `source_graph/imports.rs::last_segment` splits on `.` (Go
  `gopkg.in/yaml.v3` has local name `v3`).
- `javascript.rs` imports the `jsx_patterns!` macro from `tsx.rs`, and `tsx.rs` duplicates `kind_for_capture`,
  `import_bindings` and `capabilities` from `typescript.rs` (`ecmascript_import_bindings` is a one-line forwarder);
  move the JSX patterns to a neutral module and share a `concat!` fragment so the JS and TSX queries cannot drift.
  Confirm JSX member names (`<ui.Icon />`) are `member_expression` in the pinned grammar, and that the query text
  inside `typescript_query!` is byte-identical to the former const (the TS digest must not move).
- Doc or test drift: `typescript.rs` says `.tsx` falls through to the lexical node (it is a named Gap when no extractor
  is registered); `extract/tests.rs` has two tests asserting the same wave-B Java gap; the oversized-file test
  asserts neither `parser_version` nor currency across `ensure_snapshot` and `build_worktree_graph`; C# file-scoped
  and PHP statement-form namespaces give the same class a different id than block form (tell the binding stage).

## Resolved View, Store and Cache

- `view/store.rs`: `cache_view` keys an overlay view whose layer has an empty generation at the base view path with an
  identity equal to the base identity, so a later `view(rev, None)` in the same process could take the overlay graph as
  the base view, and keys by the caller's overlay when the overlay layer vanished (the entry is then never found). On
  an overlay build, `self.view(&revision, None)` takes the base view out of `view_cache` (`take_cached_view`), so a
  later `view(HEAD, None)` parses the base file again; when the base view was not persisted, `build_view` removes the
  view `materialize_view` cached a moment earlier. A successful `persist_view` does not drop a `view_fallback` entry at
  the same path (layers do via `write_or_fall_back`), so an old-generation overlay entry makes `load_view` return
  `None` without reading the current file. `newest_older_base_view` fully parses each older view (up to
  `keep_base_graphs` files of about 100 MB) before the `relinkable_from` filter, although after a `RESOLVER_VERSION`
  or extractor bump none can seed a relink. `has_current_view` and `view()` each load the overlay layer (harmless race).
- `graph_store/prune.rs`: `remove_views` deletes files only, so `view_cache` and `view_fallback` keep the view of a
  replaced base in a process on a read-only cache; `view_dir()` uses `base_dir().with_file_name("view")` where
  `self.graph_root.join(VIEW_RELATIVE_DIR)` is reachable. A denied `publish_base` skips `prune_after_publish` and
  `remove_views` (no current caller order reaches it). A successful `replace_base` does not clear a `memory_fallback`
  entry left by an earlier denied write, so a stale in-memory layer would shadow disk if write permission returned
  mid-process. `graph_store` imports `view::ResolvedView` while `view/store.rs` implements `GraphStore` methods, a
  mutual dependency that design section 12 prescribes (`fall_back_view_to_memory` copies the body of
  `fall_back_to_memory`; a generic helper over the target map would keep the `is_write_denied` policy in one place).
- `view/identity.rs` and `map/views/snapshot.rs::extractor_versions` both walk `registry()`; extract one helper. The
  view's `ResolvedView` derives `PartialEq` including `origin`, so a materialized and a built view with identical
  content compare unequal (compare via `canonical_bytes`). `SnapshotOutcome.persisted`'s doc names only the layer
  fallback though it is now false for a refused `state.json` write and a view fallback.
- `refresh/snapshot.rs`: `let _ = super::mark_semantic_stale(..)` (two sites) swallows the failure; log it with
  `tracing::warn!`. The warm path still parses the full base layer in `ensure_base` only for `layer_is_current`, then
  parses the view: it meets "view parsed at most once" but not a one-file warm path. `worktree_graph.rs`: a changed
  worktree file over `MAX_EXTRACTED_FILE_BYTES` fails `read_bounded` and becomes an unreadable entry with no file node,
  while the refresh path records `Oversized` with a file node.
- `map/views/snapshot.rs`: `built_at` is the layer file mtime and so null for a layer held only in memory (the stage
  sandbox); a `GraphStore::resolved_with_layers` would remove the ~12-line overlay merge duplicated there.
  `commands/knowledge/eval.rs::ensure_local_snapshot` uses a private `GraphStore` whose in-memory layers are dropped
  before `retrieve_for_stage` reads disk under a read-only cache. `timings.resolve` reads about 0 on a cold run
  because a view built inside `ensure_snapshot` counts under `snapshot` (documented in `timings.rs`).
- Tests: `view/tests_equivalence.rs` seeds only Rust, TypeScript and Python direct imports; add re-export chains, C
  prototypes and headers, Java and C# globs and namespace changes, and unreadable-to-readable transitions. The
  memory-fallback tests (`graph_store/fallback_tests.rs`, `view/tests_store.rs`) skip when 0o555 is not enforced
  (root, some sandboxes) and pass without asserting; a fallback seam that does not depend on file modes would fix it.
  `refresh/tests_source_graph/mod.rs` duplicates `git_ok`, `init_repo` and the stores from `tests_snapshot.rs`, and
  `layer_reuse.rs::a_base_holding_a_gap_dialect_file_is_reused` returns early when wave B is compiled, so it needs a
  non-vacuous variant.

## Lease, Census, Window and Hardening

- `reconcile_graph/cancel.rs`: a sub-millisecond PID-reuse window remains between the argv and start-time check and
  `kill` (`pidfd_open` and `pidfd_send_signal` close it on Linux; an attacker-writable lease controls the epoch, so argv
  is the only gate); `ps` is spawned through `.output()` with no deadline (use `run_bounded_output`); `--cancel` rewrites
  the lease to `pid 0` with a fresh epoch even when the recorded holder is dead, where design says no live holder is a
  no-op.
- `reconcile_graph/lock.rs::read_lock` uses `fs::read_to_string`: it follows symlinks, is unbounded and blocks on a FIFO
  in place of `reconcile.lock`, stalling the prompt hook's claim path; writer capability on `<main>/.loom/cache` is not
  established, so this is a hardening item.
- `census/classify.rs`: `size_of` and `has_generated_marker` refuse a symlink only at the last component (a directory
  swapped for a symlink is followed; `open_safely` and `read_bounded` would confine it); `git check-attr` is spawned
  with no deadline (a hung git on an unusual `--root` hangs `--census`); `--census` with only `--root <other>` still
  builds a snapshot for the current project; the current project's coverage comes from the resolved graph even when it
  is stale or never built and the report does not say so; `extractor_status` allocates a `String` per eligible file;
  `report.rs::inline_safe` truncates root and subproject paths at 200 chars, so two long roots can print identically.
- `window.rs`: a window on an `Oversized` file reads the whole file (`read_bounded(.., usize::MAX)`) to hash it; cap it.
  `commands/map.rs:300` and `window.rs:45-47` echo the id and path raw on stderr while the success path uses `safe()`
  (not attacker-reachable). `map/views/filters.rs`: `--path .` normalises to `.` and matches nothing while `--path ./`
  matches everything; treat a bare `.` as no filter.
- Terminal safety: `render.rs::inline_code` maps only `is_control`, so U+2028/U+2029 and bidi controls survive in the
  `Literal text` line (no injection, but it drifts from the single-sanitizer rule in `untrusted.rs`); `terminal_safe`
  keeps Unicode `Cf` bidi controls (U+202A-202E, U+2066-2069), which allows Trojan-Source reordering in source windows;
  `knowledge/context.rs` prints knowledge excerpts raw through `render_excerpt_block` (older than this plan): sanitize
  inside the block renderer.
- `git/runner.rs`: `core.fsmonitor=false` on runner status calls in the user's own repo drops the index fsmonitor
  extension whenever git rewrites the index, so fsmonitor users pay a full rescan; consider `--no-optional-locks`.

## Retrieval Routing and Rendering

- `rank_source/intent.rs`: `PRONOUNS` holds no articles, so `classify("who calls the parser")` returns
  `Relationship { Callers, "the" }` and a node named `a` would be seeded by `what uses a cache` (add article
  negatives to `rank_source_intent.rs`); `captured_name` examines only the first regex match, so a pronoun first match
  hides a later real symbol; the `NON_SYMBOL_WORDS` doc example (`who calls the parser`) reads as if it routes to
  `parser` when `classify` returns `General`. `classify` runs on the multi-line composite text of stage and worker
  briefs, where a quoted phrase halves source lexical scores and adds an `rg` line.
- `rank_source/routing.rs`: `who uses X` and `usages of X` map to `References` edges only, which for a function returns
  little because uses are recorded as `Calls` (consider `Calls` plus `References`); expansion runs again from the routed
  seed in every direction, so `who calls target` also returns target's callees although the module doc promises the asked
  direction only; `relation_edges` calls `direct_callers`, `direct_callees`, `direct_references` or `impact_with` once per
  seed (up to 5), each building a full node map or adjacency (acceptable for a CLI). `admit_neighbour`,
  `source_candidate` and the `RelationEdge` to `NeighborVia` conversion repeat the construction in `expand.rs` (share
  one helper); `routing/relation.rs::admit_relationship` does not skip `File` neighbours as `expand.rs` does, and the
  `File` half of `is_expandable` there is unreachable because `inputs.nodes` already excludes `File` nodes.
- `rank_source/expand.rs`: `neighbour_tokens` prices the raw explanation while `render_source_entry` charges the
  `inline_safe` text plus `;`, so the budget undercounts slightly; routed neighbours are not counted in
  `Expansion.expanded`, so the count cap can be exceeded up to 24 combined (`MAX_EXPANDED_TOKENS` still bounds the
  total).
- Rendering: `inline_safe` also rewrites the backticks `neighbor_explanation` writes, so a brief shows ˋseedˋ where the
  design shows backticks (not a safety defect, pinned by tests); `retrieve/windows.rs` discards `read_window`'s
  `truncated` flag, so a window cut at 12 lines looks complete (add a trailing marker).
  `commands/hook/user_prompt_compose.rs` sits at 396 of 400 lines, so the next edit must split it.
- Tests and cases: `loom/eval/retrieval-cases.yaml` `relationship-callers-of-reconcile-source-graph` expects only the
  seed, which ordinary candidacy admits from its snake_case spelling, so it passes with routing disabled (put a real
  caller id in `expect`; the symbol-question cases have the same property); the bystander in
  `retrieval_delivery_contracts.rs::relationship_query_seeds_direct_callers` has no edges, so the absence assertion
  catches only a router that admits every node, and direction cannot be asserted at rank level because `expand.rs`
  expands both directions.

## Map, Evaluator and Fixtures

- `commands/map.rs`: an unreadable or invalid `--thresholds` file propagates as `Err` and exits 1, indistinguishable
  from a failing threshold (give it a distinct exit code). `commands/tests_map.rs` covers neither `--eval-edges` as a
  view flag, `--thresholds` requiring `--eval-edges`, nor the `--census` conflict.
- `context/eval_edges/`: `build.rs` repeats `excluded(&stored)` that `walk_into` already applies; `metrics.rs`
  `SEMANTIC_KINDS` copies the edge-kind list and its `kinds` and `follow_candidates` overrides restate
  `ImpactOptions::default()`; `map/views/eval_edges.rs` computes `failing` twice; `syntax_error_files` entries are only
  counted, never checked to be syntax errors (assert non-`Full` coverage); reference-label paths are not checked, so an
  `external` label with a mistyped path always passes; floors are uniform at the smallest corpus's counts, so a larger
  corpus (rust: 18 targets, 14 externals) can be cut to 6/3 by deleting failing labels (pin per-dialect counts);
  `tests_unknown_ids.rs` should filter shipped corpora by `spec.pack.compiled()` as `edge_quality_contracts.rs` does,
  since under `--no-default-features --features source-graph` the wave-B and wave-C corpora extract only file nodes.
- Fixtures: only the Rust corpus (`Vec::new` vs `Widget::new`) and the TSX corpus (`render` vs `Panel::render`) label an
  external call whose bare name also exists in the corpus; the Rust corpus has no `pub use` and the TypeScript corpus no
  overload signatures although design 14.1 lists re-exports and overloads; the Ruby `D-import-spec` labels pass only if a
  wrong bind fails, so a resolver leaving both helper calls unbound stays green under `target_recall 0.5` (pin them with a
  dedicated assertion); add a method on a non-struct Go type and a Rust `impl From<X> for String` to the corpora; the
  TSX `panel.header()` label needs a header comment saying why it is ambiguous; the Rust label suffix `@16bb7dca` cannot
  be checked by reading the code, and the evaluator does not verify that a labelled target id exists.
- `tests/map_api_contracts.rs`: `window_rejects_ids_outside_the_graph` tries only `../outside.txt` (add an absolute
  path such as `/etc/hostname@0-5` and an untracked file inside the root); `map_timings_json_reports_phases_and_peak_rss`
  accepts any number, so assert `peak_rss_kb > 0` on Linux.

## Smaller Items

- `orchestrator/signals/format/brief.rs::freshness_word` is a one-line wrapper over `state().as_str()`; inline it.
  `retrieve/graph.rs` keeps a doc comment describing the rebuild trigger as "stale OR degraded", which `wants_rebuild` no
  longer matches for never-built and unavailable.
- `tests/worker_evidence/support.rs` holds a third copy of `retry_past_etxtbsy` (also in the native wrapper tests and
  `commands/self_update/tests.rs`); a shared `tests/integration/helpers.rs` would serve all three.
- `loom/maintainability-baseline.txt` has one entry out of alphabetical order (cosmetic).

- Proposal (a recurrence of [Stage Wiring Patterns Were Not Quoted in the Brief](../mistakes/source-graph-delivery.md)): a
  `loom plan verify` check that warns when a worker brief names a file carrying a stage wiring pattern but does not quote
  the pattern.
- `commands/stage/complete.rs` around line 311 carries a doc comment saying `--no-verify` marks the stage
  `CompletedWithFailures`; the code path at lines 694-742 calls `try_complete`. Correct the comment.

- Proposal (a recurrence of [A Ticket Whose Line Never Reached the Hook Was Leaked Forever](../mistakes/memory-relay-drain-gap.md)):
  a shell loop of 45 `loom memory resolve` calls with stdout sent to `/dev/null` hit the 32-ticket cap after 32, because
  the relay hook reads each `LOOM_RELAY_V1` line from the Bash tool output. Either let `loom memory resolve` take several
  ids in one call, or make the command refuse to start when the session already holds more than a handful of unconsumed
  tickets; until then run at most about ten memory writes per Bash call with stdout unfiltered.

- `extract/treesitter/binding.rs::is_member` counts a qualified `Function` (a Go method with its receiver type as
  `@definition.qualifier`) as a member, so any future dialect that qualifies a free function (namespace-qualified) with
  `bare_calls_reach_members = false` would lose bare-call binding to it.
- `rank_source/routing.rs::admit_neighbour` recomputes `expand.rs`'s private `max_neighbor_score`
  (`BOOST_EXACT_SYMBOL * config.test_path_factor`); expose it `pub(super)` and call it from routing.
- Flaky tests seen once each, unidentified or unfixed: `worker_evidence::transcript_growth_invalidates_stop` (an
  `ExecutableFileBusy` write-then-exec race; write to a temp name, close, rename, or retry on ETXTBSY),
  `commands::repair::tests_local_keys::repair_strips_exactly_the_loom_written_keys_and_keeps_the_rest` (byte-identical
  left and right at `:89`), and one `cargo test --all-targets` failure in `--lib` whose test name the check output did not
  print. Capture the check output to a file so the failing test name survives.
- The `loom plan verify` base-tree check does not warn on a criterion such as `rg -q -F -- '--field acceptance|wiring'
  README.md` that already passes at HEAD: a pattern starting with `--` after `-F --` escapes the passes-at-HEAD warning, so
  run every `rg` criterion at HEAD by hand.
- Chaining several `loom memory` commands with `&&` in one Bash call is refused by the relay hook (it relays only a call
  that runs one loom command); the tickets were recovered on the next single call, which duplicated one entry. A `while`
  loop over a single command form works.
