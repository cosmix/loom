# W5: coverage by dialect, bytes and gaps

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside W2 and W4. W1 has finished: `DIALECTS`, `extractor_for`,
  `Capabilities` and the new provenance names exist.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 5.1,
  5.3 and 8.

## Goal

`CoverageReport` separates the questions a single status used to compress:

- which files were enumerated, by files and bytes;
- which parsed (status);
- which dialect and extractor capabilities applied;
- how edges resolved (by provenance, unresolved, ambiguous).

It also reports gap dialects.

## Files you own (write)

- `loom/src/context/coverage.rs`, which becomes a directory module:
  `context/coverage/mod.rs` and `context/coverage/dialects.rs`, with tests in
  `context/coverage/tests.rs` (the existing tests move there unchanged except for the
  new-field assertions)
- `loom/src/map/views/json.rs`: `footer_json` and its key-set test only
- `loom/src/map/views/mod.rs`: `render_footer` only

Delete `loom/src/context/coverage.rs` when you create `coverage/mod.rs`, so the file
and the directory module never coexist.

Read-only: everything else, including `loom/tests/graph_contract_contracts.rs`.

## Steps

1. Convert `coverage.rs` into `coverage/mod.rs`. `pub use` keeps
   `crate::context::coverage::CoverageReport` and `crate::context::CoverageReport`
   working.
2. Add the fields in design 8 to `CoverageReport`, plus
   `pub struct DialectCoverage { ... }` in `dialects.rs`, built from the graph:
   - A file's dialect is `dialect_for_path(path)`. `None` counts as unsupported.
   - A file's bytes are its file node's `span.end_byte`, or `Oversized.bytes`.
   - The extractor status comes from `extractor_for(&registry(), path)`:
     - `Lookup::Extractor` gives `extractor: "registered"` plus `capabilities()`;
     - `Lookup::Gap` gives `extractor` in design 8's wording (`"pack {feature} not
       compiled"`, or `"no extractor"` when the pack is compiled but nothing is
       registered), while the `gaps[].detail` entry carries design 5.3's detail text
       from the `Gap`;
     - `Lookup::Unknown` counts as unsupported.

   Build the registry once per `CoverageReport::of` call.
3. Edge counts per dialect come from the edges of each file's `FileEntry`:
   - `edges_by_provenance` keys on `as_str()`;
   - `unresolved_edges` counts `to == UNRESOLVED_TARGET`;
   - `ambiguous_edges` counts a non-empty `candidates`.
4. `Display` keeps its current one-line shape and adds bytes to the symbol-level share
   (`52% of files, 61% of bytes symbol-level`). A zero byte total renders the
   files-only share, with no division. When gaps exist, append
   `; gaps: java 3 files (pack source-graph-wave-b not compiled)`, collapsing to
   `; gaps: 4 dialects (n files)` when more than 2.
5. `footer_json` carries every new field. `by_dialect` is a JSON object keyed by
   dialect id.
6. Tests in `coverage/tests.rs`:
   - the existing tests pass unchanged except where the footer string gains the bytes
     clause;
   - add: a mixed graph with a `.rs` file (registered), a `.java` file and a `.md` file
     (unsupported) reports per-dialect files and bytes, the gap detail, and unsupported
     counts. Guard the `.java` assertion at runtime, never with `#[cfg]`: when
     `!GrammarPack::WaveB.compiled()` it is a gap with design 5.3's detail text, and
     otherwise `.java` reports `"registered"` (stage `language-packs` compiles wave B
     by default);
   - add: `ambiguous_edges` counts an edge with candidates;
   - update the JSON key-set test in `map/views/json.rs` to the new footer keys.

## Traps

- `coverage.rs` is at 394 lines. Split it first, then add code.
- `Display` flows into the bootstrap prompt (`commands/knowledge/bootstrap/mod.rs`).
  Keep it one line.
- `map/views/mod.rs` is at 389 lines. Touch only `render_footer`, and keep the file at
  or under 400.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::coverage:: 2>&1 | tail -15`

## Report

Report the files changed and the new footer line for a sample graph.
