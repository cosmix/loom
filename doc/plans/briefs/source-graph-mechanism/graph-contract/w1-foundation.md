# W1: evidence types, dialect table, and crate-wide migration

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 1, alone. W2, W4 and W5 start after you report; W3 starts after W2.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 2,
  3, 4 (the `SourceNode.symbol_key` field only), 5.1, 5.2 and 5.3 in full before
  anything else.

## Goal

Introduce the new types and signatures exactly as the design specifies. Then migrate
every consumer mechanically so that `cargo build --all-targets` compiles and every
existing test passes under the new vocabulary, with no behaviour change beyond the
renames listed here. The semantic changes (receiver binding, candidate sets, id
disambiguation, sites, corrupt-layer handling, coverage dimensions) belong to W2–W5.
Where this brief says "placeholder behaviour", keep today's behaviour under the new
names.

## Files you own (write)

These are the owned files:

- `loom/src/context/source_graph/mod.rs`, `edge.rs`, `node.rs`, `tests.rs`, and the
  new `imports.rs`
- `loom/src/context/extract/mod.rs`, `tests.rs`, `lexical.rs`, and the new `dialect.rs`
- `loom/src/context/extract/rust.rs`, `typescript.rs`, `python.rs`, `go.rs`: the trait
  impl blocks and test tables only; W3 rewrites the queries later
- `loom/src/context/extract/rust/tests.rs`
- `loom/src/context/extract/treesitter/mod.rs`, `build.rs`, `collect.rs`: mechanical
  renames only
- `loom/src/context/graph_store/mod.rs`: split it (see step 6)
- `loom/src/context/graph_store/fallback_tests.rs`, `tests.rs`
- `loom/src/context/refresh/source_graph/layer.rs`, `loom/src/context/refresh/snapshot.rs`,
  `loom/src/context/refresh/tests_source_graph/mod.rs`, `layer_reuse.rs` (it holds
  `.supports(` calls), `loom/src/context/refresh.rs` (the `BoxedExtractor`
  re-export), `loom/src/context/refresh/source_graph.rs` (`schema_version` stamping
  only)
- `loom/src/context/schema.rs`: the re-export line only. It is at 394 lines, so add
  at most 2.
- `loom/src/context/worktree_graph.rs`, `worktree_graph_tests.rs`
- `loom/src/verify/goal_backward/reachable.rs`, `definition_sites.rs`, `reachable_tests.rs`
- `loom/src/context/resolve.rs`, `resolve/fixtures.rs`, `resolve/tests_resolve.rs`,
  `tests_qualified.rs`, `tests_impact.rs`, `tests_neighbors.rs`, `resolve/neighbors.rs`,
  `resolve/impact.rs`
- `loom/src/context/coverage.rs` (renames only)
- `loom/src/map/views/mod.rs`, `json.rs`, `neighbors.rs`, `tests.rs` (renames, plus the
  `..Default::default()` literal rewrites in step 8)
- `loom/src/context/tests/rank_source_expand.rs`, `rank_source_expand_fusion.rs`,
  `source_fixtures.rs`, `retrieve_source.rs`, `pack_required.rs`
- `loom/src/commands/knowledge/bootstrap/tests_clusters.rs`
- `loom/src/commands/hook/tests_user_prompt_e2e.rs`, `commands/hook/worker_brief/test_support.rs`
- `loom/src/commands/knowledge/tests_context.rs`
- `loom/src/orchestrator/signals/tests_brief_e2e.rs`
- `loom/src/plan/schema/tests/v2_lint_tests.rs`
- `loom/src/verify/impact_tests/libtest.rs`
- `loom/tests/integration/source_graph_fixtures.rs`

These are every file that names a changed type today. Re-run the enumeration commands
in step 9 before you finish; any new hit is yours too.

Read-only: `loom/tests/graph_contract_contracts.rs` (frozen contracts; never edit it).

## Steps

1. **Evidence types** (`source_graph/edge.rs`, `mod.rs`).
   - Replace `EdgeProvenance` with the seven classes in design 3.1, including
     `as_str`, `ceiling()` and `rank()`.
   - Add the confidence constants to `mod.rs` with docstrings, and delete
     `MAX_INFERRED_CONFIDENCE` and `MAX_RESOLVED_INFERRED_CONFIDENCE`. Name
     `AMBIGUOUS_CANDIDATE_CONFIDENCE` (0.2) and `MAX_CANDIDATES` (8) there as well.
   - Add one `fn syntax_confidence(kind: SourceEdgeKind) -> f32` (0.3 for `Calls`, 0.5
     otherwise) in `mod.rs`. Both `SourceEdge::syntax` callers and
     `SourceEdge::unbind` take their confidence from it.
   - Add the fields `sites`, `candidates` and `receiver` to `SourceEdge` with the
     serde attributes in design 3.2.
   - Replace the constructors `parser`, `inferred`, `unresolved` and `resolve_to` with
     `structural`, `syntax`, `bound`, `bind` and `unbind`, plus the builders
     `with_receiver` and `with_candidates` and the helpers `site_count()` and
     `pub fn site_id(path, span)`.
   - `bind` enforces the design's rules. Add unit tests in `source_graph/tests.rs`
     covering:
     - `bind` refuses a `Structural` edge;
     - `bind` refuses an already-bound edge;
     - `bind` refuses a `LocalName` provenance;
     - `bind` sets confidence to the provenance ceiling and clears `candidates`;
     - `unbind` restores `syntax_confidence(kind)`, `0.3` for `Calls`.
2. **Import bindings.** Create `source_graph/imports.rs` with `ImportBinding` and
   `local_name()` (design 3.4), re-exported from `source_graph/mod.rs` and
   `context/schema.rs` (extend its existing `pub use` list of source-graph types).
   - `local_name()` returns `alias`, else `name`, else the last path segment. It returns
     `None` for a glob and for `alias == Some("")`.
   - Harnesses (W3 and the language-packs workers) set `alias` explicitly whenever the
     bound local name is not the last path segment: Python `import a.b` gives alias
     `Some("a.b")`, so the receiver text `a.b` in `a.b.f()` matches it (resolver
     rule 2). A side-effect import (TS `import "x"`, a statement-level `require("x")`,
     Go `import _ "p"`) gives alias `Some("")`. Go `import . "p"` is a glob.
   - Add a unit test in `source_graph/tests.rs` for each `local_name()` case: alias,
     name, last segment, glob, and `Some("")`.
3. **Nodes** (`source_graph/node.rs`).
   - Add `symbol_key` with serde attributes (design 4). It is always empty in this
     wave; W2 fills it.
   - Extend `NodeLanguage` with the eight variants and the explicit
     `#[serde(rename = ...)]` per variant (design 5.1).
   - Delete `impl From<DetectedLanguage> for NodeLanguage` and the
     `use crate::language::DetectedLanguage` line.
   - Update the `FileCoverage::Full` docstring per design 6.3.
4. **Dialect table** (`extract/dialect.rs`, new).
   - Add `GrammarPack`, `DialectSpec`, `DIALECTS` (all 12 rows exactly as in design
     5.1), `dialect_for_path` and `dialect_by_id`.
   - `DialectSpec` has two columns beyond design 5.1's first six fields, both used at
     extraction and resolution:
     - `self_receivers: &'static [&'static str]`: rust `["self", "Self"]`; typescript,
       tsx, javascript, java, csharp, cpp `["this"]`; python `["self", "cls"]`; ruby
       `["self"]`; php `["$this", "self", "static"]`; go and c `[]`;
     - `bare_calls_reach_members: bool`: true for java, csharp, cpp and ruby, false
       otherwise.

     W2's `QueryHarness::self_receivers()` defaults to the dialect's list.
   - `GrammarPack::compiled()` in this stage is `cfg!(feature = "source-graph")` for
     `Core` and `false` for `WaveB` and `WaveC`. Stage `language-packs` adds those
     features and flips them.
   - Unit tests in `extract/tests.rs`:
     - every extension appears once;
     - every id equals its `language.as_str()`;
     - `dialect_for_path("a/B.H")` returns `cpp` (the lookup lowercases the
       extension);
     - an unknown extension returns `None`.
5. **Trait and lookup** (`extract/mod.rs`).
   - Change `SourceGraphExtractor` per design 5.3: add `dialect()` and
     `capabilities()`, and remove `language()` and `supports()`.
   - `extract/mod.rs` declares `pub mod dialect;`, so
     `loom::context::extract::dialect::{DIALECTS, GrammarPack, dialect_for_path}` is
     public (the frozen contract imports it).
   - Add `Capabilities` (derives `Debug, Clone, Copy, Default, PartialEq, Eq,
     Serialize`), `pub type BoxedExtractor` (move the alias from `context/refresh.rs`
     here and re-export it there), `Lookup` and `extractor_for`.
   - `extract_file` uses `extractor_for`. A `Gap` becomes `FileExtraction::file_level`
     with `LexicalOnly` and the exact detail strings in design 5.3. Its file node
     carries `parser_version = LEXICAL_PARSER_VERSION` and
     `language = dialect.language`.
   - `layer_is_current`, `parser_version_matches` and the `worktree_graph` currency
     check treat `Gap` exactly like `Unknown`.
   - Add `ExtractorIdentity.dialect` and the new `to_parser_version()` format.
   - `FileExtraction` gains `imports: Vec<ImportBinding>` (empty for every
     file-level extraction).
   - `lexical::language_for_path` derives its tag from `dialect_for_path`, falling
     back to `Other(ext)` (and `Other("unknown")` for no extension). `LEXICAL_PARSER_VERSION`
     is unchanged.
   - Each of `rust.rs`, `typescript.rs`, `python.rs` and `go.rs`:
     - implements `dialect()` via `dialect_by_id("<id>").expect(...)`, or a static
       reference into `DIALECTS`;
     - implements `capabilities()` as
       `{declarations: true, imports: true, import_bindings: false, calls: true, receivers: false, references: false}`;
     - sets `ExtractorIdentity.dialect`.
   - Registry test in `extract/tests.rs`:
     - registered dialect ids are distinct;
     - each registered dialect is in `DIALECTS`;
     - a `.java` path gives a `Gap` whose detail is exactly
       `grammar pack source-graph-wave-b not compiled in (java)`;
     - a `.tsx` path gives `no extractor registered for dialect tsx`.
     - Guard the last two assertions with runtime conditions, never `#[cfg]`
       attributes. The `source-graph-wave-b` feature does not exist until stage
       `language-packs`, and a `cfg` naming an undeclared feature is an
       `unexpected_cfgs` warning, which clippy `-D warnings` rejects. Assert the
       `.java` gap only when `!GrammarPack::WaveB.compiled()`, and the `.tsx` gap
       only when `registry()` has no extractor whose `dialect().id == "tsx"`. Stage
       `language-packs` deletes the `.tsx` assertion when it registers TSX.
   - Snapshot test in `refresh/tests_source_graph/`: a base holding a gap-dialect file
     (a `.java` file, guarded at runtime on `!GrammarPack::WaveB.compiled()`) is
     `Reused` on the second `ensure_snapshot`.
6. **Store types.**
   - `graph_store/mod.rs` is at the 400-line cap. Move `FileEntry`, `GraphLayer` and
     `ResolvedGraph` (with their impls) into a new `graph_store/layer_types.rs` and
     re-export them from `mod.rs`. `pub use` keeps every import path working.
   - Add `GraphLayer.schema_version` (design 2).
   - Add `FileEntry.imports` with serde attributes.
   - Add `pub fn FileEntry::from_extraction(bytes: &[u8], extraction: FileExtraction) -> FileEntry`
     (`content_hash = body_hash(bytes)`), and use it in `refresh/source_graph/layer.rs`
     and `worktree_graph.rs` wherever an entry is built from an extraction.
   - Every layer the builder persists sets `schema_version: GRAPH_SCHEMA_VERSION`.
     Find the constructions in `layer.rs::assemble_layer`, `persist_layer` and
     `refresh/source_graph.rs`. W4 adds the checks that read it.
7. **Replace every `supports()`/`language()` call** with `extractor_for`:
   - `refresh/source_graph/layer.rs::parser_version_matches`;
   - `refresh/snapshot.rs::layer_is_current`;
   - the extraction scan filter in `worktree_graph.rs` (the `supports` call);
   - `verify/goal_backward/reachable.rs` (the `supports` call in its extractor check).
     Its check becomes "any node whose path `extractor_for` maps to
     `Lookup::Extractor`";
   - `refresh/tests_source_graph/layer_reuse.rs` (its `.supports(` calls);
   - `definition_sites.rs`: it has no `supports()` call, so only the registry change
     applies. Build the registry once, in a `OnceLock`.

   Locate each call with `rg -n '\.supports\(' loom/src` or `loom map --outline`, never
   by line number.
8. **Mechanical provenance migration**, with placeholder behaviour and today's semantics
   under new names:
   - `build.rs`:
     - `Contains` uses `SourceEdge::structural`;
     - a same-file spelling hit uses `SourceEdge::bound(..., Calls, ..., site, LocalName)`,
       with `site` = `Span::default()` for now (W2 fills real spans);
     - an unresolved call uses
       `SourceEdge::syntax(..., Calls, ..., Span::default(), syntax_confidence(Calls))`;
     - an import uses
       `SourceEdge::syntax(..., Imports, ..., Span::default(), syntax_confidence(Imports))`.
   - Keep `dedupe`'s key.
   - `treesitter/mod.rs` constants move to `source_graph` names; keep
     `IMPORT_CONFIDENCE`/`UNRESOLVED_CALL_CONFIDENCE` as private aliases or inline the
     values.
   - `resolve.rs`:
     - `retarget` becomes `edge.bind(target, provenance)`;
     - a hit found through the qualified-spelling loop or the qualifier module path
       binds with `Import`;
     - a bare-name hit (`by_name` on an unqualified symbol) binds with `UniqueName`;
     - the loop's guard becomes "skip unless `provenance == Syntax && is_unresolved()`";
     - delete `UNIQUE_MATCH_CONFIDENCE` and update its docstring references.
   - `resolve/impact.rs` and `neighbors.rs`: replace any `EdgeProvenance::Parser` or
     `Inferred` reference, and use `rank()` wherever "weakest provenance" compares
     provenances.
   - Map views, `coverage.rs` and every test: replace `"parser"` with `"structural"`
     for `contains` edges and `"local-name"` for same-file `calls`, and
     `"inferred"` with `"syntax"` for unresolved edges, `"unique-name"` for bare
     resolutions or `"import"` for qualified resolutions. The confidence `0.75`
     becomes `0.6` (unique-name) or `0.85` (import), and `1.0` on a same-file call
     becomes `0.8`.
   - Struct literals later stages extend: rewrite, to `..Default::default()`, the full
     `ImpactOptions` literals in `map/views/json.rs`, `map/views/mod.rs` and
     `verify/goal_backward/reachable.rs`, and the two full `ResolutionStats` literals in
     `map/views/tests.rs`. Stage 3 (R2) and stage 4 (M1) add fields to both types in
     files they do not own, so a full literal left here would break their build.
   - Update each test's expected table to the value the new code produces. Never
     delete a test or loosen an assertion to make it pass. If a test pinned a behaviour
     this plan removes, say so in your report and leave it failing.
9. **Enumerate and finish.** Run each command and resolve every hit:
   - `rg -n 'EdgeProvenance::(Parser|Lsp|Inferred)|MAX_INFERRED_CONFIDENCE|MAX_RESOLVED_INFERRED_CONFIDENCE|UNIQUE_MATCH_CONFIDENCE|resolve_to\(|SourceEdge::(parser|inferred|unresolved)\(' loom/src loom/tests`
     must print nothing;
   - `rg -n '\.supports\(|fn language\(&self\) -> DetectedLanguage' loom/src/context loom/src/verify`
     must print nothing;
   - `rg -n 'DetectedLanguage' loom/src/context` must print nothing.

## Traps

- `NodeLanguage` serde:
  - Put the explicit `#[serde(rename = "typescript")]` on `TypeScript`, so that
    serialized layers write `typescript`, not `type-script`.
  - `Other(String)` stays externally tagged.
- `SourceEdge` has no `Eq`, so keep the derive set.
- `sites`/`candidates` use `skip_serializing_if`, which keeps small layers readable.
- Do not add sites or change `dedupe` semantics. That is W2's work, and W2 starts after
  you.
- `extractor_for` precedence is the `DIALECTS` row, never registration order.
- Keep `graph_store/mod.rs` and every file you touch under 400 lines. Put new tests in
  new `tests_*.rs` files when a test file is over 350 lines.
- `loom/maintainability-baseline.txt` is a ratchet file: never edit it.

## Your one check (run once)

`cargo build --manifest-path loom/Cargo.toml --all-targets 2>&1 | tail -5`. Then
`cargo test --manifest-path loom/Cargo.toml --lib context:: 2>&1 | tail -15`. The
main agent owns the full gate.

## Report

Report:

- the files changed;
- any test you could not migrate, and why;
- every consumer you found outside the owned list.
