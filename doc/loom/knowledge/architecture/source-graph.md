---
sources:
- loom/src/context/refresh/source_graph.rs
- loom/src/context/graph_store/mod.rs
verified: 5546d3c47ddc1f8890b40157134f057393b8b90e
---
# Source Graph

> Source graph contract, limits

## What It Is, and What It Is Not

`loom/src/context/source_graph/` plus `context/extract/` hold a derived,
tree-sitter-backed graph of the repository's own source: file and symbol nodes
plus edges between them. It has **two** production consumers, and both are live:

| Consumer | Route in | What it reads |
| --- | --- | --- |
| `loom map` (`--outline`, `--find-all`, `--impact`, `--callers`, `--callees`, `--references`, `--window`, `--census`, `--eval-edges`) | `context::graph_store` and the resolved view (`context::view`) | the resolved graph, rendered as read-only views |
| the `Source` retrieval channel | `context::rank_source` → `fuse` → `pack` | symbol nodes, scored and fused with knowledge chunks into one `ContextPack` |

The ranking design behind the second consumer is in
`architecture/context-retrieval.md`. The failure class it closed, a store nothing
reads, is in `mistakes/store-without-consumer.md`.

Nobody builds this graph by hand. `loom init` and `loom run`
publish a base layer through `advisory_source_graph_preflight`, and every stage's
overlay is reconciled just before its signal is written — see *Lifecycle* below.

Types live in `context/source_graph/` (not `context/schema.rs`) because the graph
is a distinct domain from the knowledge corpus; `schema.rs` re-exports the public
names, so callers may reach them through either path
(`context/source_graph/mod.rs:8-12`). A plan that names `schema.rs` as the home
of `SourceNode`/`SourceEdge` is naming the re-export — run
`rg -n "pub struct <Type>" loom/src/` before opening the file a plan points at.

## The Honesty Contract

**This graph is never claimed to be exhaustive** (`source_graph/mod.rs` module doc).
Every `SourceEdge` carries an `EdgeProvenance` (seven evidence classes, strongest
first) and an explicit confidence. A call whose target cannot be resolved is a `Syntax`
edge to `UNRESOLVED_TARGET` (`"<unresolved>"`), never silently dropped and never given
an invented destination. Consumers that render or traverse the graph **must surface
provenance and confidence rather than flattening them away**.

| Class (`as_str`) | Meaning | Confidence constant |
| --- | --- | --- |
| `Structural` (`structural`) | containment: both endpoints are declarations the grammar placed in one file | `STRUCTURAL_CONFIDENCE = 1.0` |
| `Compiler` (`compiler`) | bound by a compiler or language server; reserved, **nothing emits it** | ceiling `1.0` |
| `Receiver` (`receiver`) | a `self`/`this`/`Self`/`$this`/`static` call bound to a member of the enclosing type | `RECEIVER_CONFIDENCE = 0.85` |
| `Import` (`import`) | bound through an import, alias, module-qualified path or package/namespace scope to exactly one definition | `IMPORT_CONFIDENCE = 0.85` |
| `LocalName` (`local-name`) | same-file spelling with exactly one eligible definition | `LOCAL_NAME_CONFIDENCE = 0.8` |
| `UniqueName` (`unique-name`) | the only same-family definition of the name, no stronger evidence, no refusal rule firing | `UNIQUE_NAME_CONFIDENCE = 0.6` |
| `Syntax` (`syntax`) | captured at a site; target unresolved or ambiguous | calls `0.3`, imports and references `0.5` (`syntax_confidence(kind)`); ceiling `MAX_SYNTAX_CONFIDENCE = 0.5` |

The constants live in `context/source_graph/mod.rs`, each with a docstring stating that
the number is an evidence ranking, not a calibrated probability. Only `Structural` (and
the reserved `Compiler`) may carry `1.0`; a contract test pins that, and
`SourceEdge::bound` debug-asserts against minting a `Structural` or `Syntax` edge.
`EdgeProvenance::ceiling()` returns the column above and `rank()` orders strength for
"weakest provenance" reporting.

**Edge shape** (`source_graph/edge.rs`). Beyond `from`/`to`/`kind`/`provenance`/`confidence`
an edge carries `symbol` (the spelling written at the site), `sites` (every reference
`Span`, sorted by `start_byte`, deduplicated), `candidates` and `receiver` (receiver text
of a member call). An edge lives in the `FileEntry` of the file it was extracted from,
so a site's path is that entry's key; `site_id(path, span)` is `"{path}@{start}-{end}"`,
stable within one snapshot and never persisted. Extraction-time deduplication
(`extract/treesitter/build.rs::dedupe`) groups on `(from, to, kind, provenance, symbol,
receiver)` and merges `sites`, so two calls to one callee are one edge with two sites.
`Contains` edges carry no sites; `Calls`, `Imports` and `References` carry at least one.
`SourceEdge::bind` moves ONLY an unresolved `Syntax` edge to `Receiver`, `Import` or
`UniqueName` at that class's ceiling and clears its candidates; `unbind` restores the
extraction-time state (relink uses it, see [Source Graph Resolved View](source-graph-view.md)).

**Candidate sets.** A `Syntax` edge whose target is ambiguous keeps `candidates`: sorted
node ids, at most `MAX_CANDIDATES = 8`; with more than 8 the list stays empty and the edge is
plain unresolved. Traversal (`impact_with`, which `impact`, `reachable` and impact-selected
tests share) treats each candidate as a reverse edge trusted at
`AMBIGUOUS_CANDIDATE_CONFIDENCE = 0.2`, so the hit's weakest provenance is `syntax` and it is
flagged `via_candidates`. Retrieval expansion never follows candidates: they sit below its 0.5
floor. A plan check that must not rest on candidates sets `min_confidence` above 0.2.

**Import bindings** (`source_graph/imports.rs`). `ImportBinding { path, name, alias,
exported_as, glob, site }` records what each import statement binds; `local_name()` is the
alias, else the name, else the path's last segment (`None` for a glob). A side-effect import
(TS `import "x"`, Go `import _ "p"`) and a re-export bind no local name (alias `Some("")`); a
re-export records the name it exports under in `exported_as` (`export { a as b } from "x"`
gives `b`). `path` is lossless: C/C++ `#include <x>` keeps its `<` and is external, Ruby
`require_relative 'x'` is `./x`. One `Imports` edge per statement still exists, now a
`Syntax` edge with a site.

The ceiling discipline is the reusable idea: when a component's view of the world is
structurally narrower than the claim it is asked to make, encode the gap as a numeric ceiling
in a named constant with the reasoning in its docstring, not as a comment at the call site.
A unique *name* match is still not a parse: two unrelated crates can define one name, and a
graph that omits a file omits its definitions too, so "the only match I can see" is not "the
only match". Resolution only ever raises confidence with evidence.

## The Extractor Trait

`context/extract/mod.rs` — bytes in, `FileExtraction` out. Each dialect has one extractor
over a pinned grammar and a tree-sitter query embedded in its module.

```rust
pub trait SourceGraphExtractor {
    fn dialect(&self) -> &'static DialectSpec;
    fn capabilities(&self) -> Capabilities;
    fn cache_identity(&self) -> ExtractorIdentity;
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction>;
}
```

**Dialect lookup, not `supports()`.** `context/extract/dialect.rs` (always compiled, no `cfg`)
holds `DIALECTS: &[DialectSpec]`, `dialect_for_path` (lowercase extension) and `dialect_by_id`.
A `DialectSpec` carries `id` (equals `NodeLanguage::as_str()`), `language`, `family`,
`extensions`, `grammar` (crate and version), `pack: GrammarPack`, `self_receivers` and
`bare_calls_reach_members`. Every extension appears in exactly one row, pinned by a unit test.
`extractor_for(extractors, path)` is the one path to an extractor and returns
`Lookup::{Extractor, Gap, Unknown}`; `extract_file`, `parser_version_matches`,
`layer_is_current`, the `worktree_graph` scan and `verify/goal_backward` all call it.

| id | family | extensions | grammar | pack |
| --- | --- | --- | --- | --- |
| `rust` | rust | `rs` | `tree-sitter-rust 0.24.2` | Core |
| `typescript` / `tsx` | ecmascript | `ts,mts,cts` / `tsx` | `tree-sitter-typescript 0.23.2` | Core |
| `javascript` | ecmascript | `js,mjs,cjs,jsx` | `tree-sitter-javascript 0.25.0` | Core |
| `python` | python | `py,pyi` | `tree-sitter-python 0.25.0` | Core |
| `go` | go | `go` | `tree-sitter-go 0.25.0` | Core |
| `java` | java | `java` | `tree-sitter-java 0.23.5` | WaveB |
| `csharp` | csharp | `cs` | `tree-sitter-c-sharp 0.23.5` | WaveB |
| `ruby` | ruby | `rb,rake,gemspec` | `tree-sitter-ruby 0.23.1` | WaveB |
| `php` | php | `php` | `tree-sitter-php 0.24.2` | WaveB |
| `c` | c | `c` | `tree-sitter-c 0.24.2` | WaveC |
| `cpp` | c | `cc,cpp,cxx,hh,hpp,hxx,h` | `tree-sitter-cpp 0.23.4` | WaveC |

Resolution never binds across **families**: `ecmascript` joins TypeScript, TSX and JavaScript,
`c` joins C and C++, every other dialect is its own family. `.h` belongs to `cpp` because the C++
grammar accepts nearly every C header while the C grammar rejects C++ headers outright; the
standard `#ifdef __cplusplus / extern "C" {` guard still makes a header a whole-file `ParseError`
([Source Graph Known Gaps](../concerns/source-graph-known-gaps.md)). `NodeLanguage` has a unit
variant per dialect with an explicit `#[serde(rename = "<id>")]`, plus `Other(String)`.
`crate::language::DetectedLanguage` (the stage and skill language, 11 variants) takes its extension mapping
from `dialect_for_path`: TSX folds into TypeScript and `Other(_)` maps to no language.

**Gap vs Unknown.** `Lookup::Gap` is a dialect loom knows whose extractor is absent: the file
still gets a file-level node with `FileCoverage::LexicalOnly` and a named detail
(`grammar pack {feature} not compiled ({dialect})` or `no extractor registered for dialect
{dialect}`). `Unknown` (no dialect for the extension) keeps the lexical fallback. Both stamp
`LEXICAL_PARSER_VERSION`, and every currency check treats them alike, so a base holding such a
file is `Reused` on the next `ensure_snapshot`. `Capabilities { declarations, imports,
import_bindings, calls, receivers, references }` feeds the coverage report; the flags are
informational and never gate behaviour.

What an extractor promises: every node it emits is a real declaration in the bytes it was
handed, and every edge carries honest provenance. It does **not** promise an exhaustive call
graph. Extraction is per-file, so a cross-file call is unresolved or ambiguous until
`context::resolve` ([Source Graph Resolution](source-graph-resolution.md)) binds it. A syntax
error is data (`FileCoverage::ParseError`), never an `Err`.

**One shared harness.** The tree-sitter walk lives in `context/extract/treesitter/` (`mod.rs`
`run_query`, `build.rs`, `collect.rs`, `binding.rs`, `ids.rs`), parameterized by the per-language
`QueryHarness` trait. A language module supplies a grammar, a query using the capture protocol
(`@definition.<kind>`, `@name`, `@import.path`, `@import.statement`, `@call.name`,
`@call.receiver`, `@reference.name`, `@definition.qualifier`) and `kind_for_capture`. Identity-by-
default hooks cover dialect quirks: `import_bindings`, `import_spec` (Ruby `require_relative` gives
`./x`), `definition_name` (PHP `A\B` gives `A.B`), `self_receivers` (defaults from the dialect row)
and `top_level_self` (Ruby only). A `@definition.*` match with no `@name` counts toward
`FileCoverage::Partial`. Centralizing makes the honesty constraint (provenance, the 0.5 ceiling)
structural instead of a rule twelve `extract()` implementations each have to remember. The compiled
`Query` is cached per `(NodeLanguage string, query text)` for the process.

## Node and Edge Identity

- File node id: the relative path, forward-slashed (`file_node_id`).
- Symbol node id: `<relative-path>#<kind>:<scope-joined-by-::>`, scope outermost-first, joined
  with `::` regardless of language so ids are comparable across extractors. Empty scope is invalid.
- **The kind is part of the id because scope alone is not unique.** Rust's `struct Widget` and
  `impl Widget` share a name, as do a TypeScript `interface Foo` and a `const Foo`. Keying on
  scope alone let an implementation node shadow the type it implements and made their `Contains`
  edges indistinguishable. The `node_id` docstring carries the reasoning.
- **Duplicate declarations** get a signature suffix. After a file's definitions are collected,
  `extract/treesitter/ids.rs::disambiguate` groups them by computed id; every member of a group of
  two or more becomes `{base}@{sig8}` (first 8 hex of `sha256` over the signature with whitespace
  runs collapsed), and members that still collide append `.{n}` (1-based, source order). A node
  that collides with nothing keeps its plain id. `SourceNode.symbol_key` holds the un-suffixed
  base id when a node was disambiguated and is empty otherwise. `@` is the delimiter because
  `render.rs::parse_source_identity` and `pack/twins.rs::tier1_twin` split ids on `#` and `:`;
  line numbers never enter an id. Duplicates share spellings, so a call to them is ambiguous.
- A nested definition's scope is its parent's full scope (including the parent's
  `@definition.qualifier`) plus its own qualifier and name, so Ruby `class A::B` with a method
  `m` gives `[A, B, m]` and members stay addressable under the qualified type.
- A Go method `func (w *Widget) run()` has scope `[Widget, run]` and id
  `<file>#function:Widget::run` with no parent node; the receiver type is captured as
  `@definition.qualifier`, as for C++ `void W::run()`. Every Go `type_spec` and `type_alias`
  declares a `Type` node (an interface literal declares an `Interface`).
- A namespace or package `Module` node has ONE scope segment holding the dotted name (Java
  `package a.b;` gives `["a.b"]`, C# and PHP likewise, C++ `namespace A::B` gives `["A::B"]`).
  C# file-scoped `namespace A.B;` and PHP statement-form `namespace A;` emit the `Module` node
  only: their types keep no namespace scope, so the same class is `type:A.B::W` in block form and
  `type:W` in file-scoped form.
- Methods need a body: Java, C# and PHP methods and C++ definitions without one are not
  definitions (abstract and interface methods are dropped). C/C++ function prototypes add no
  node; they are `References` edges (see [Source Graph Resolution](source-graph-resolution.md)).
- `SourceNodeKind`: `File`, `Function`, `Type`, `Interface`, `Module`, `Constant`,
  `Implementation`. `SourceEdgeKind`: `Contains`, `Imports`, `Calls`, `References`, `Implements`,
  `Extends`. Both have `as_str()` giving the stable lowercase name used in ids, CLI output and
  fixture JSON, so renaming a variant breaks golden fixtures and node ids at once.

## Cache Identity

Three layers of identity stop a cached extraction from an older build being reused.

**Graph schema.** `GRAPH_SCHEMA_VERSION` (`context/source_graph/mod.rs`, currently 2) is stamped
into every `GraphLayer.schema_version`. A layer whose version differs is never current and never a
reuse source: `layer_is_current`, `build_layer` (as a previous layer) and `build_worktree_graph`
(treats it as absent) all refuse it. A layer file that fails to deserialize is **corrupt**, not an
error that wedges the cache: `read_layer` returns `Ok(None)` after a `tracing::warn!` naming the
path, so `ensure_base` rebuilds it and `load_newest_base` skips it. A stale or unparseable base is
replaced through `GraphStore::replace_base` (atomic temp file plus rename; a write-denied cache
falls back to memory), never by `path.exists()` then `remove_file`, which bypasses the read-only
fallback and races concurrent snapshots.

**Extractor.** `ExtractorIdentity` (`context/extract/mod.rs`) has `dialect`, `grammar_version`,
`query_digest` (`sha256:<hex>` over the embedded query) and `extractor_version` (`u32`, **bumped by
hand** whenever the walking logic changes shape). `to_parser_version()` renders
`{dialect}:{grammar}+{digest12}+v{n}`, stored on every node as `SourceNode::parser_version`, so
TypeScript and TSX never share one. The grammar and query halves are automatic;
`extractor_version` is not. Changing the walk without bumping it serves stale cached extractions
with no error anywhere. The same applies to `RESOLVER_VERSION` for resolution rules.

**Currency predicate.** Gap, Unknown and `Oversized` file nodes carry `LEXICAL_PARSER_VERSION`
(`lexical+v1`), not an extractor identity. The currency check (`entries_are_current` in
`refresh/source_graph/layer.rs`, with `parser_version_matches`) must cover every coverage variant
that stamps it in ONE function: comparing an Oversized node to the extractor identity made a layer
holding a >512 KiB file never current ([Source Graph Delivery Mistakes](../mistakes/source-graph-delivery.md)).

Content identity is `body_hash(bytes)` = `sha256:<hex>`. `build_layer` tries two reuses before it
parses a file, each against the previous layer and the base: **by git blob id** (`reuse_by_oid`:
path not dirty and its blob id equals the recorded `blob_index` entry, no read, no hash) and **by
content hash** (`reuse_by_hash`). Both also require `parser_version_matches`, so bumping an
identity forces a re-parse. An unreadable file's entry and an overlay tombstone carry an empty
`content_hash`, which never equals a real `body_hash`, and `reuse_by_oid` skips empty hashes; that
is what makes keeping those entries safe.

## Coverage: Nothing Ever Vanishes

`FileCoverage` (`context/source_graph/node.rs`) records why a file got less than full
treatment. No path silently disappears (the `context::extract` module doc):

| Situation | Result |
| --- | --- |
| no dialect for the extension (`Lookup::Unknown`) | file node, `FileCoverage::LexicalOnly` |
| dialect known, pack not compiled or no extractor (`Lookup::Gap`) | file node, `LexicalOnly`, named detail |
| file over `MAX_EXTRACTED_FILE_BYTES` (512 KiB) | file node, `FileCoverage::Oversized` |
| grammar reports a syntax error | file node, `FileCoverage::ParseError`, no symbol nodes |
| a `@definition.*` match had no `@name` | named definitions still emitted, `FileCoverage::Partial` |
| `source-graph` cargo feature disabled | file node, `FileCoverage::LexicalOnly` |
| unreadable file | entry with no nodes and an empty `content_hash`, `LexicalOnly` ("unreadable: …"), see `unreadable_entry` |
| file deleted in the working tree | overlay tombstone with no nodes, `FileCoverage::Deleted` |

`FileCoverage::Full` means the configured syntax pass completed without unnamed definition
matches. It does **not** mean the call graph is exhaustive. `Deleted` records a deletion rather
than a degraded extraction, and only an overlay carries it (see [Source Graph Build](source-graph-build.md)). A
degraded file is *reported as degraded*, never omitted; when you add an extractor, test the
degraded paths, because the happy path fails loudly and the degraded paths fail silently.

`context::coverage::CoverageReport` (`context/coverage/{mod.rs,dialects.rs}`) aggregates it and
adds byte totals and a symbol-level share of bytes, `unsupported_files`/`unsupported_bytes`,
`by_dialect` (per dialect: files, bytes, symbol-level files and bytes, `files_by_status`,
`edges_by_provenance`, `unresolved_edges`, `ambiguous_edges`, the extractor status `registered` /
`pack {feature} not compiled` / `no extractor`, and `capabilities`) and `gaps`. The `loom map`
footer prints `52% of files, 61% of bytes symbol-level` plus a short `gaps:` clause; `footer_json`
carries every field. `loom map --census` ([Source Graph Evaluation](source-graph-evaluation.md))
reports the same dimensions across projects.

## Stack

The grammar crates, their exact pins, the three cargo features (`source-graph`,
`source-graph-wave-b`, `source-graph-wave-c`) and the per-pack capability-gap rule are in
[Stack](../stack.md#tree-sitter-source-extraction). The short form: every grammar is an optional,
`=`-pinned dependency; a pack that is not compiled reports its dialects as named gaps
(`Lookup::Gap`) instead of failing the build; `--no-default-features` is the degraded mode that
yields file-level lexical nodes only.

## Lifecycle: Who Builds It, and When

Nothing in the normal path asks a human to build the graph, and every entry point
is **advisory** — it reports failure and continues, because a missing graph must
degrade retrieval, never block a run.

**One decision path: `ensure_snapshot(store, graph_store, project_root, policy)`**
(`context/refresh/snapshot.rs`). It inspects the working tree once, then:

| `SnapshotPolicy` | Does |
| --- | --- |
| `BaseOnly` | ensure the base for `HEAD` |
| `LocalCurrent` | ensure the base for `HEAD`; when the tree is dirty (its generation differs from `clean_generation(HEAD)`), also bring the checkout's `_local` overlay (`local_overlay_key`) current |
| `StageOverlay { plan, stage }` | bring that stage's overlay current; no base publish |

A base for `HEAD` is reused when it exists and is `layer_is_current` (current schema version
and every entry current under `entries_are_current`); a stale or corrupt base is replaced in
place through `replace_base`. An overlay is reused when its `generation` equals the tree's and
it is `layer_is_current`. The result is a
`SnapshotOutcome { action, reason, revision, generation, overlay, counters, elapsed, persisted,
serving }` whose `SnapshotAction` is `Reused`, `Updated` (some files reused), `Rebuilt` or
`Unavailable`. `persisted` is false when the memory fallback served the layer, when
`state.json` refused a write, or when a view fell back to memory; `serving` names the older
base a failed build fell back to. `state()` maps the outcome to a `GraphState`, and
`describe()` renders the single `source graph: …` advisory line every surface prints,
appending `; not persisted (cache read-only)` or `; serving stale base <rev8>`. After it
publishes or reuses a layer, `ensure_snapshot` also materializes the resolved view so the
prompt hook never builds one on its hot path ([Source Graph Resolved View](source-graph-view.md)).

| When | Call site | Policy or scope |
| --- | --- | --- |
| `loom init` | `commands/init/execute.rs`, via `advisory_source_graph_preflight` | `BaseOnly` |
| `loom run` (daemon) | `commands/run/mod.rs::prepare_background_run` | `BaseOnly` |
| `loom run --foreground` | `commands/run/foreground.rs` | `BaseOnly` |
| `loom map`, `loom knowledge sync` | `commands/map.rs`; `refresh::semantic::try_reconcile_semantic` | `LocalCurrent` |
| prompt-hook self-heal | `loom hook reconcile-graph` (`commands/hook/reconcile_graph.rs`), spawned detached when `wants_rebuild` says so (a stale graph, or a current one with a degraded pack); guarded by a lease | `HookTarget::snapshot_policy`: `StageOverlay` in a stage, `LocalCurrent` in a checkout |
| before a stage's signal is written, and before a merge | `MergeLifecycle::reconcile_overlay`, from `stage_executor.rs` (fresh spawn), `skip_retry.rs` (recovery), `merge_handler.rs` and `progressive_complete.rs` | `reconcile_source_graph` with `Overlay { plan, stage }` |
| after a merge | `MergeLifecycle::reconcile_base` | `reconcile_source_graph` with `Base { revision }` of the merged revision |

`advisory_source_graph_preflight(repo_root, work_dir)` (`commands/run/checks.rs`)
never returns a `Result`, so it cannot bail startup: it prints the `describe()` line
unless the base was simply reused, and one `source graph: unavailable (...)` line on
error. It is modelled on `advisory_codex_lane_preflight`.

**Ordering in `loom run`.** The preflight runs AFTER
`plan_inputs::mark_plan_in_progress`, which commits the plan-file rename. The base is
therefore published against the committed `IN_PROGRESS-` filename and the revision
stages inherit. A dirty tree does not block this, because bases are built from
committed content.

**Recovery signals need their own call.** Signal bytes are embedded once at write
time and `start_stage` later re-uses them verbatim from disk, so a crash/hang retry
that did not reconcile first would hand the agent a stale overlay (`skip_retry.rs`).
`start_knowledge_stage` deliberately has no reconcile call — it runs in the main
repo with no worktree, and `reconcile_overlay` returns early when the stage has no
worktree directory.

## Stage Worktree Cache Is Read-Only: In-Memory Fallback

Inside a sandboxed stage worktree the shared context cache (`<main>/.loom/cache/context-v1`)
is read-only to the stage session.

`GraphStore::fall_back_to_memory` (`loom/src/context/graph_store/fallback.rs`) treats a
permission-denied or read-only-filesystem write as this call's success:

- The freshly built layer is kept in `GraphStore.memory_fallback` for the rest of the process,
  and `GraphStore::read_layer_or_memory` prefers it over disk.
- `publish_base` and `write_overlay` (`graph_store/mod.rs`) route their write failures through
  it, so `reconcile_with_working_tree` still returns a normal `Built`/`Reused` outcome.
- As a result, a stage session's `loom map` and `loom knowledge context` answer with the full
  graph even when the host never published a base for the worktree's `HEAD`. The graph is not
  persisted, so the next process starts over.
- Only a genuine bug (malformed path, serialization failure) propagates and produces the
  "DEGRADED: source graph base ... missing" state.
- Resolved views fall back the same way (`view_fallback`, `fall_back_view_to_memory`), and a
  denied `state.json` write is carried as `state_persisted = false` into
  `SnapshotOutcome.persisted`. In a stage sandbox `loom map` therefore reports `persisted: false`
  and `view: "built"`, and every process is cold: nothing persists.

A from-scratch base build of this repository is a cold parse of every tracked file. The
`loom map --impact` footer at `67e442d5` reported 3447 files, 25198 nodes and 117583 edges. That
build takes tens of seconds, well over the 15 s `GIT_READ_TIMEOUT`. The timeout bounds each git
invocation, not the refresh, so a cold build still completes.
