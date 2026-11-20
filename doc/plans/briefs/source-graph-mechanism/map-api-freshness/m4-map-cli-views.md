# M4: the `loom map` command surface

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, after M1 (neighbours/impact), M2 (snapshot states) and M3
  (census/window) report.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 10 in
  full, and section 8 (the footer fields).

## Goal

`loom map` implements design 10 exactly. It needs:

- honest help text;
- direct-call views with call sites;
- `--references`, `--window`, `--census` and `--timings`;
- a `--impact` default of semantic edges only;
- the `--evidence` and `--lang` filters;
- exact-id matching and labelled substring fallback in every view;
- reported filter stages and suppressed counts;
- JSON with `schema` and `snapshot` identity;
- stale serving.

## Files you own (write)

- `loom/src/commands/map.rs`, `loom/src/commands/tests_map.rs`
- `loom/src/map/views/**`. Split `views/mod.rs` (389 lines) before adding code: one
  module per view (`outline.rs`, `find_all.rs`, `impact.rs`, `neighbors.rs`,
  `window.rs`, `census.rs`, `matching.rs`, `filters.rs`), keeping `json.rs` or
  splitting it the same way. `views/tests.rs` and the `json.rs` key-set tests are
  yours.
- new `loom/tests/map_cli.rs` (binary-level integration tests beyond the contracts)
- the one `MapArgs` literal in `loom/src/context/refresh/tests_snapshot.rs`, after M2
  has finished with that file (step 1)

Read-only: M1–M3's modules, `loom-hooks/_read_discipline.sh` (the outline row
contract), and `loom/tests/map_api_contracts.rs` (frozen; read it first).

## Steps

1. **`MapArgs`.**
   - New flags: `--references`, `--window`, `--window-lines` (default 60),
     `--census`, `--root` (repeatable, census only), `--evidence` (comma list parsed
     with `EdgeProvenance` names), `--lang`, `--timings`.
   - The `--kinds` default becomes `calls,references,implements,extends`.
   - Help strings: `--callers`/`--callees` read "Print direct callers (one hop over
     call edges) with their call sites" and the callee form.
   - `require_view` counts the new views; update its error text and the test that
     pins it (`map_without_a_view_flag_names_all_available_views` in `tests_map.rs`).
   - `MapArgs` has no `Default`, and `refresh/tests_snapshot.rs` builds one by full
     literal. Replace that literal with
     `MapArgs::try_parse_from(["map", "--outline", "src.rs", "--json"])`, so later flags
     (yours, and stage `edge-quality-eval`'s `--eval-edges`) never break it.
2. **Matching** (`views/matching.rs`).
   - An argument containing `#` is an exact node id or file id. Otherwise try an exact
     name, then the substring fallback.
   - Return `(nodes, MatchMode::{Id, Exact, Substring})`.
   - Every view uses it; `resolve_starts` must stop discarding the label.
3. **Views.**
   - Callers, callees and references use M1's API. Each row reads
     `caller path:L<site line> → target path:L<decl line>  symbol=<s>  <provenance> <confidence>  sites=<n>`,
     plus `candidate (<n> total)` when `candidate_of` is set.
   - Impact passes `--evidence` as `ImpactOptions.provenances`, and marks
     `via_candidates` hits. It renders M1's `ImpactResult.filtered_out` as
     `filters.path.filtered_out` (`ImpactOptions` keeps `limit` and `path_prefix` with
     post-traversal semantics).
   - Find-all honours `--limit`, `--path` and `--lang` while scanning; remove
     `FIND_ALL_CAP`.
   - Path filtering is component-aware.
   - Every view reports `suppressed_starts`, `suppressed` and `filters` (design 10).
4. **Snapshot.**
   - `load_graph` keeps `LocalCurrent`. It prints `snapshot.describe()` on stderr
     unless reused, as today.
   - Keep the literal call `resolve_graph(&mut graph)` in `load_graph`. Stage
     `resolved-view` replaces it, and its `before_stage` check looks for that text.
   - When `snapshot.state()` is `NeverBuilt`, it prints
     `source graph never built: <reason>` and emits empty views.
   - A stale serve uses `snapshot.revision`, which M2 points at the served base.
   - The JSON `snapshot` object follows design 10: the `extractors` map comes from
     `registry()` as `dialect.id` to `cache_identity().to_parser_version()`, and
     `schema_version` from `GRAPH_SCHEMA_VERSION`. The generation and `built_at` come
     from the loaded layers: read them through `GraphStore::load_base` and
     `load_overlay` in the same call, never by parsing twice.
5. **Timings.** Wrap snapshot, load, resolve, query and render in `Instant`s.
   `peak_rss_kb` comes from `libc::getrusage(RUSAGE_SELF)` `ru_maxrss` (KiB on Linux,
   bytes on macOS: normalize). `libc` is already a dependency; confirm in
   `Cargo.toml`, and never add one.
   - The frozen contract `map_timings_json_reports_phases_and_peak_rss` runs
     `loom map --find-all target --json --timings`. The JSON `timings` object must
     hold numeric `snapshot`, `load`, `resolve`, `query`, `render`, `total` and
     `peak_rss_kb` (an empty or partial object fails), and stderr must contain the word
     `total`. Stage `resolved-view` adds a `view` phase and keeps these seven.
6. **Census and window.**
   - `--census` calls `census::run` with the current root and graph plus every
     `--root`.
   - `--window` calls `window::read_window`. The exit-code mapping lives in
     `commands/map.rs`:
     - on `UnknownId` it prints `unknown id: <id>` on stderr and exits 2;
     - on `ChangedSinceSnapshot` it exits 3.
     - It never prints bytes on either error.
   - The JSON neighbour-row keys are exactly those in design 10.
   - `footer_json` renders `ResolutionStats.by_provenance` beside `retargeted`,
     `ambiguous` and `unresolved`, so the field has a reader. Update the
     `map/views/json.rs` key-set tests to include it.
7. **Tests.**
   - Unit tests in `views/tests.rs` for:
     - matching modes;
     - filter reporting;
     - the row formats;
     - outline row compatibility: the regex `^\s+L[0-9]+-L[0-9]+\s` must still match
       every outline row.
   - Binary tests in `loom/tests/map_cli.rs` (temp git repo). Spawn the binary ONLY
     through `helpers::loom_cmd()`, declared as
     `#[path = "integration/helpers.rs"] #[allow(dead_code)] mod helpers;` exactly as
     `loom/tests/token_optimization_contracts.rs` does.
     `tests/integration/binary_spawn_guard.rs` fails any other file that writes
     `Command::new(env!("CARGO_BIN_EXE_loom"))`, and an unscrubbed spawn inherits the
     caller's `LOOM_*` identity and real `HOME`:
     - `--window` of a node prints exactly its lines;
     - `--timings --json` has a `timings` object holding the seven numeric phases
       above, and stderr contains `total`;
     - `--references` on a fixture with no references prints its empty-state line
       and exits 0;
     - `--evidence import` filters impact;
     - `--lang` filters find-all.

## Traps

- `loom-hooks/_read_discipline.sh` parses outline rows. The outline format must not
  change.
- The JSON key-set tests pin keys. Update them to the new sets; never delete them.
- `commands/map.rs` must stay under 400 lines. Put view orchestration in
  `map/views/`, not in the command.
- `loom map` runs in any checkout, whether or not `loom init` has run: keep that
  (`tests_snapshot.rs::map_answers_in_a_checkout_that_never_ran_init`).

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib map:: 2>&1 | tail -15`

## Report

Report the files changed, the final `--help` text for the changed flags, and one
sample JSON payload.
