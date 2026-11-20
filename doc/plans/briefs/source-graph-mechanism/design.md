# Source graph mechanism: settled design

This file is the shared specification for every stage of
`doc/plans/PLAN-source-graph-mechanism.md`. Each worker brief names the sections it
implements; read those sections in full before writing code. Where a brief and this
file disagree, this file wins; where this file and the plan YAML disagree, the YAML
wins for acceptance, files and contracts.

Line numbers below are advisory anchors from the tree at `67e442d5`. Anchor every
edit by symbol name and re-read the range before editing: earlier stages of this
plan move them.

## 1. Scope and non-goals

In scope:

- Evidence-bearing edges (call sites, evidence classes, candidate sets), duplicate
  node identities, split coverage reporting, graph schema versioning.
- A dialect registry decoupled from `DetectedLanguage`, with grammar packs.
- New dialects: TSX (`tree-sitter-typescript` `LANGUAGE_TSX`), JavaScript/JSX
  (`tree-sitter-javascript`), Java, C#, Ruby, PHP (pack `source-graph-wave-b`), C and
  C++ (pack `source-graph-wave-c`). Every grammar comes from `github.com/tree-sitter`.
- Language-aware cross-file resolution (imports, aliases, qualifiers, package scope,
  receiver context) with explicit refusal rules.
- `loom map` API changes, freshness states, the reconcile lease, the portfolio
  census, source windows, timings.
- A persisted, versioned resolved view with an incremental relink proven equal to a
  cold build. Persisted name and adjacency indexes are deferred to the storage-engine
  decision (section 12).
- Retrieval: intent routing, anchored windows, explained neighbours, token-capped
  expansion, weak-coverage caveats, a narrow natural-language symbol fallback.
- An offline edge-quality evaluator with labelled corpora and published thresholds,
  plus an operator protocol for the cross-project agent-task comparison.

Out of scope (decided):

- Kotlin and Swift: their grammars are not maintained under `github.com/tree-sitter`.
- Shell and configuration languages (proposal wave D).
- A storage engine other than canonical JSON. `--timings` measures warm cost so a
  later plan can decide from data.
- Running the agent-task comparison. It needs consenting projects and live agent runs;
  this plan ships its protocol (`doc/source-graph-evaluation.md`) and the offline
  tools it uses.
- Compatibility periods, migration maps, or dual CLI behaviour. The project is
  unreleased (repository `CLAUDE.md`): old cached layers are discarded and rebuilt
  through the schema version, and CLI defaults change in place.

## 2. Schema version and cache invalidation

- `pub const GRAPH_SCHEMA_VERSION: u32 = 2;` in `context/source_graph/mod.rs`.
- `GraphLayer` gains `#[serde(default)] pub schema_version: u32`. Every layer the
  builder writes sets it to `GRAPH_SCHEMA_VERSION`; a file written before this plan
  deserializes as `0`.
- A layer whose `schema_version != GRAPH_SCHEMA_VERSION` is never current and never a
  reuse source: `layer_is_current` (`refresh/snapshot.rs`) returns false,
  `build_layer` (`refresh/source_graph/layer.rs`) ignores it as a previous layer, and
  `build_worktree_graph` (`context/worktree_graph.rs`) treats it as absent.
- A layer file that fails to deserialize is **corrupt**, not an error that wedges the
  cache. `read_layer` (`graph_store/mod.rs`) returns `Ok(None)` for it after logging a
  `tracing::warn!` naming the path, so `ensure_base` rebuilds and overwrites it and
  `load_newest_base` skips it. A corrupt overlay is rebuilt the same way.
- `build_worktree_graph` additionally re-extracts any base entry whose first node's
  `parser_version` differs from the current extractor identity for its dialect (today
  it trusts stale entries).
- Every existing extractor's `extractor_version` increases by one in the stage that
  changes the walk (`graph-contract`); `layer_is_current` then rebuilds old bases.

## 3. Evidence model

### 3.1 Evidence classes

`EdgeProvenance` (keep the type name and the `provenance` field name) becomes seven
classes, strongest first. Serde and `as_str()` use the kebab-case names shown.

| Variant | `as_str` | Meaning | Confidence (constant) |
| --- | --- | --- | --- |
| `Structural` | `structural` | Containment: both endpoints are declarations the grammar placed in one file | `1.0` (`STRUCTURAL_CONFIDENCE`) |
| `Compiler` | `compiler` | Bound by a compiler or language server. Reserved: nothing emits it | `1.0` ceiling |
| `Receiver` | `receiver` | A `self`/`this`/`Self`/`$this`/`static` call bound to a member of the enclosing type | `0.85` (`RECEIVER_CONFIDENCE`) |
| `Import` | `import` | Bound through an import, alias, module-qualified path, or package/namespace scope to exactly one definition | `0.85` (`IMPORT_CONFIDENCE`) |
| `LocalName` | `local-name` | Same-file spelling with exactly one in-scope definition | `0.8` (`LOCAL_NAME_CONFIDENCE`) |
| `UniqueName` | `unique-name` | The only same-family definition of the name in the graph, with no stronger evidence and no refusal rule firing | `0.6` (`UNIQUE_NAME_CONFIDENCE`) |
| `Syntax` | `syntax` | Captured at a site; target unresolved or ambiguous | calls `0.3`, imports and references `0.5`; ceiling `MAX_SYNTAX_CONFIDENCE = 0.5` |

- Constants live in `context/source_graph/mod.rs`, each with a docstring stating that
  numeric confidence is an evidence ranking, not a calibrated probability.
- `AMBIGUOUS_CANDIDATE_CONFIDENCE = 0.2`: the trust of a traversal step through one
  member of a candidate set (section 3.3). `MAX_CANDIDATES = 8` (section 3.3) lives
  beside it.
- `fn syntax_confidence(kind: SourceEdgeKind) -> f32` returns `0.3` for `Calls` and
  `0.5` otherwise. Every `SourceEdge::syntax` caller and `SourceEdge::unbind` take
  their confidence from it.
- Remove `MAX_INFERRED_CONFIDENCE`, `MAX_RESOLVED_INFERRED_CONFIDENCE` and
  `resolve::UNIQUE_MATCH_CONFIDENCE`; every reference to them moves to the new names.
- Only `Structural` (and the reserved `Compiler`) may carry `1.0`. A contract test in
  `graph-contract` pins that.
- `EdgeProvenance::ceiling(self) -> f32` returns the column above.
  `EdgeProvenance::rank(self) -> u8` orders strength (`Structural` 6 ... `Syntax` 0) for
  "weakest provenance" reporting.

### 3.2 Edge shape

```rust
pub struct SourceEdge {
    pub from: String,
    pub to: String,                 // node id or UNRESOLVED_TARGET
    pub kind: SourceEdgeKind,
    pub provenance: EdgeProvenance,
    pub confidence: f32,
    #[serde(default)]
    pub symbol: String,             // the spelling written at the site
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sites: Vec<Span>,           // every reference site, sorted by start_byte, deduplicated
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,    // sorted node ids; only on an ambiguous Syntax edge
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,   // receiver text of a member call (`self`, `obj`, `ns`)
}
```

- An edge lives in the `FileEntry` of the file it was extracted from, so a site's path
  is that entry's key. `pub fn site_id(path: &str, span: &Span) -> String` returns
  `"{path}@{start_byte}-{end_byte}"`. The id is stable within one snapshot and is
  never persisted separately.
- `Contains` edges carry no sites. `Calls`, `Imports` and `References` edges carry at
  least one.
- Deduplication (`extract/treesitter/build.rs::dedupe`) groups edges on
  `(from, to, kind, provenance, symbol, receiver)` and **merges** their `sites`
  (sorted, deduplicated), so two calls to one callee are one edge with two sites.
  Traversal stays at symbol level. `site_count()` is `sites.len()`.
- Constructors replace direct construction:
  - `SourceEdge::structural(from, to, symbol)` gives `Contains` at `1.0`.
  - `SourceEdge::syntax(from, kind, symbol, site, confidence)` gives `to = UNRESOLVED_TARGET`
    with confidence clamped to `MAX_SYNTAX_CONFIDENCE`; callers pass
    `syntax_confidence(kind)`.
  - `SourceEdge::bound(from, to, kind, symbol, site, provenance)` sets confidence to
    `provenance.ceiling()`. It is refused (debug-asserted) for `Structural` and `Syntax`.
  - Builders `.with_receiver(text)` and `.with_candidates(ids)`.
- `SourceEdge::bind(&mut self, target, provenance) -> bool` replaces `resolve_to`. It
  succeeds only on an unresolved `Syntax` edge and only for `Receiver`, `Import` or
  `UniqueName`. It sets `to`, `provenance` and `confidence = provenance.ceiling()`,
  clears `candidates`, and returns `true`. Anything else is left untouched and
  returns `false`.
- `SourceEdge::unbind(&mut self)` restores the extraction-time `Syntax` state. The
  confidence goes back to `syntax_confidence(kind)` (`0.3` for `Calls`, `0.5`
  otherwise), and `candidates` is cleared. The incremental relink (section 12) uses it.

### 3.3 Candidate sets

- A `Syntax` edge whose target is ambiguous keeps `candidates`: sorted node ids, at
  most `MAX_CANDIDATES = 8`. When more than 8 exist the list stays empty and the edge
  is plain unresolved.
- Traversal consumers treat each candidate as a reverse edge. `impact`, `reachable`
  and impact-selected tests all go through `impact_with`, and each candidate step
  trusts `AMBIGUOUS_CANDIDATE_CONFIDENCE`. The hit's weakest provenance is then
  `syntax`, and the hit is flagged `via_candidates`.
  - This keeps today's recall. Today a cross-file method call such as
    `store.resolved()` is bound by graph-wide name uniqueness at 0.75; under section 7
    it becomes a one-member candidate set instead.
  - A plan's `reachable` check that must not rest on candidates sets
    `min_confidence` above 0.2.
- `--callers`/`--callees` list candidate edges flagged `candidate (N total)`.
- Retrieval expansion never follows candidates, because they fall below its 0.5 floor.

### 3.4 Import bindings

```rust
pub struct ImportBinding {
    pub path: String,            // module spec as written, quotes stripped
    pub name: Option<String>,    // imported member (`parse` in `import { parse as p }`); None = whole module/namespace
    pub alias: Option<String>,   // local alias (`p`); None = bound under `name` (or the module's last segment)
    pub exported_as: Option<String>, // name a re-export exports under: `b` in `export { a as b } from "x"`, `ns` in `export * as ns from "x"`; None otherwise
    pub glob: bool,              // `use x::*`, `from x import *`, `import a.b.*`, `using A.B;`, `#include`, `require`
    pub site: Span,
}
impl ImportBinding { pub fn local_name(&self) -> Option<&str> } // alias, else name, else last path segment; None for glob and for alias Some("")
```

- The type lives in `context/source_graph/imports.rs` and is re-exported from
  `source_graph`.
- Harnesses set `alias` explicitly whenever the bound local name is not the last path
  segment. Python `import a.b` gives alias `Some("a.b")`, so the receiver text `a.b`
  in `a.b.f()` matches the binding (resolver rule 2).
- A side-effect import binds no local name and gives alias `Some("")`: TS `import "x"`,
  a statement-level `require("x")`, Go `import _ "p"`.
- A re-export binds no local name (alias `Some("")`) and records the name it exports in `exported_as` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
- Go `import . "p"` is a glob.
- `path` is a lossless module spec: two statements that resolve differently never
  share a spec. C/C++ `#include <x>` keeps its leading `<` (path `<x>`, external).
  Ruby `require_relative 'x'` is written `./x`; a spec already starting `./` or `../`
  is kept as written. `require 'x'` and `load 'x'` keep `x` verbatim.
- `FileExtraction` gains `pub imports: Vec<ImportBinding>`.
- `FileEntry` gains
  `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub imports: Vec<ImportBinding>`.
- One `Imports` edge per import statement (symbol = `path`, one site) stays as today,
  now `Syntax` with a site.

## 4. Node identity

- The id is computed by `node_id(path, kind, scope)` as today.
- After all definitions of a file are collected, `build.rs` groups them by computed
  id. For every group of two or more:
  - each member's id becomes `format!("{base}@{sig8}")`, where `sig8` is the first 8
    hex digits of `sha256` over the signature with runs of whitespace collapsed to one
    space;
  - members that still collide append `.{n}` (1-based, source order), giving
    `{base}@{sig8}.{n}`.
- A node's id is unchanged when it collides with nothing. A member gains the suffix as
  soon as a second declaration of that id appears.
- `SourceNode` gains `#[serde(default, skip_serializing_if = "String::is_empty")] pub symbol_key: String`.
  It holds the un-suffixed base id when the node was disambiguated, and is empty
  otherwise.
- `Contains` edges, the spelling map and every later lookup use the disambiguated id.
  Spellings come from `scope`, so duplicates share spellings and a call to them is
  ambiguous (section 6).
- `@` is the delimiter because `render.rs::parse_source_identity` and
  `pack/twins.rs::tier1_twin` split ids on `#` and `:`. Line numbers never enter an id.
- A Go method `func (w *Widget) run()` (receiver `T`, `*T`, `T[P]` or `*T[P]`) has scope `[Widget, run]` and id `<file>#function:Widget::run`, with no parent node; the receiver type is captured as `@definition.qualifier`, as for a C++ out-of-line `void W::run()`. Every Go `type_spec` and `type_alias` declares a `Type` node — `type stack []int`, `type celsius float64` and `type Widget struct{}` alike — except one whose type is an interface literal, which declares an `Interface`; so every method's receiver type has a node.

## 5. Dialect registry and grammar packs

### 5.1 Table

`context/extract/dialect.rs` (always compiled, no `cfg`):

```rust
pub enum GrammarPack { Core, WaveB, WaveC }
impl GrammarPack {
    pub fn feature(self) -> &'static str;   // "source-graph" | "source-graph-wave-b" | "source-graph-wave-c"
    pub fn compiled(self) -> bool;          // cfg!(feature = ...) per pack
}
pub struct DialectSpec {
    pub id: &'static str,                 // equals NodeLanguage::as_str()
    pub language: NodeLanguage,           // tag stamped on nodes
    pub family: &'static str,             // resolution family (5.2)
    pub extensions: &'static [&'static str], // lowercase, no dot
    pub grammar: &'static str,            // crate and version, e.g. "tree-sitter-java 0.23.5"
    pub pack: GrammarPack,
    pub self_receivers: &'static [&'static str], // receiver spellings that mean "the enclosing type"
    pub bare_calls_reach_members: bool,       // an unqualified call may name a member of the enclosing type
}
pub static DIALECTS: &[DialectSpec];
pub fn dialect_for_path(path: &Path) -> Option<&'static DialectSpec>; // by lowercase extension
pub fn dialect_by_id(id: &str) -> Option<&'static DialectSpec>;
```

| id | NodeLanguage | family | extensions | grammar | pack |
| --- | --- | --- | --- | --- | --- |
| `rust` | `Rust` | `rust` | `rs` | `tree-sitter-rust 0.24.2` | Core |
| `typescript` | `TypeScript` | `ecmascript` | `ts`, `mts`, `cts` | `tree-sitter-typescript 0.23.2` | Core |
| `tsx` | `Tsx` | `ecmascript` | `tsx` | `tree-sitter-typescript 0.23.2 (tsx)` | Core |
| `javascript` | `JavaScript` | `ecmascript` | `js`, `mjs`, `cjs`, `jsx` | `tree-sitter-javascript 0.25.0` | Core |
| `python` | `Python` | `python` | `py`, `pyi` | `tree-sitter-python 0.25.0` | Core |
| `go` | `Go` | `go` | `go` | `tree-sitter-go 0.25.0` | Core |
| `java` | `Java` | `java` | `java` | `tree-sitter-java 0.23.5` | WaveB |
| `csharp` | `CSharp` | `csharp` | `cs` | `tree-sitter-c-sharp 0.23.5` | WaveB |
| `ruby` | `Ruby` | `ruby` | `rb`, `rake`, `gemspec` | `tree-sitter-ruby 0.23.1` | WaveB |
| `php` | `Php` | `php` | `php` | `tree-sitter-php 0.24.2 (php)` | WaveB |
| `c` | `C` | `c` | `c` | `tree-sitter-c 0.24.2` | WaveC |
| `cpp` | `Cpp` | `c` | `cc`, `cpp`, `cxx`, `hh`, `hpp`, `hxx`, `h` | `tree-sitter-cpp 0.23.4` | WaveC |

- The two columns beyond the pack are used at extraction (6.2) and resolution (7):

  | id | `self_receivers` | `bare_calls_reach_members` |
  | --- | --- | --- |
  | `rust` | `self`, `Self` | false |
  | `typescript`, `tsx`, `javascript` | `this` | false |
  | `python` | `self`, `cls` | false |
  | `go` | none | false |
  | `java`, `csharp` | `this` | true |
  | `ruby` | `self` | true |
  | `php` | `$this`, `self`, `static` | false |
  | `c` | none | false |
  | `cpp` | `this` | true |

- A namespace or package `Module` node has ONE scope segment holding the dotted name:
  Java `package a.b;` gives `["a.b"]`, C# `namespace A.B` (block or file-scoped) gives
  `["A.B"]`, PHP `namespace A\B;` gives `["A.B"]`. The namespace index (section 7)
  reads that segment.
- `.h` belongs to `cpp`. The C++ grammar accepts nearly all C headers, while the C
  grammar rejects C++ headers outright. A C header the C++ grammar rejects is
  reported as `ParseError`.
- Every extension appears in exactly one row, and a unit test pins that.
- `NodeLanguage` keeps `Other(String)` and gains `Tsx`, `JavaScript`, `Java`, `CSharp`,
  `Ruby`, `Php`, `C` and `Cpp`. Every unit variant gets an explicit
  `#[serde(rename = "<id>")]` equal to its `as_str()` (this also fixes today's
  `"type-script"` spelling).
- `impl From<DetectedLanguage> for NodeLanguage` is removed.
- `crate::language::DetectedLanguage` is not touched: stage and skill behaviour stays
  keyed to it.

### 5.2 Families

Cross-file resolution never binds across families: a Python call cannot bind a Go
definition.

- `ecmascript` joins TypeScript, TSX and JavaScript.
- `c` joins C and C++.
- Every other dialect is its own family.

### 5.3 Trait and lookup

```rust
pub trait SourceGraphExtractor {
    fn dialect(&self) -> &'static DialectSpec;       // replaces language()
    fn capabilities(&self) -> Capabilities;
    fn cache_identity(&self) -> ExtractorIdentity;
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction>;
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Capabilities { pub declarations: bool, pub imports: bool, pub import_bindings: bool,
                          pub calls: bool, pub receivers: bool, pub references: bool }
pub fn extractor_for<'a>(extractors: &'a [BoxedExtractor], path: &Path) -> Lookup<'a>;
pub enum Lookup<'a> { Extractor(&'a (dyn SourceGraphExtractor + Send + Sync)),
                      Gap { dialect: &'static DialectSpec, detail: String }, Unknown }
```

- `extract/mod.rs` declares `pub mod dialect;`, so
  `loom::context::extract::dialect::{DIALECTS, GrammarPack, dialect_for_path}` is public.
- `supports()` is removed. The single path to extractor lookup is `dialect_for_path`,
  then the registered extractor whose `dialect().id` matches.
- `extract_file`, `parser_version_matches` (`refresh/source_graph/layer.rs`),
  `layer_is_current` (`refresh/snapshot.rs`), the fallback scan in
  `context/worktree_graph.rs` and `verify/goal_backward/{reachable,definition_sites}.rs`
  all call `extractor_for`. `definition_sites.rs` builds the registry once per
  process: `std::sync::OnceLock`.
- A `Gap` produces a file-level node with `FileCoverage::LexicalOnly` and one of these
  details:
  - `"grammar pack {feature} not compiled in ({dialect})"` when the pack is not
    compiled;
  - `"no extractor registered for dialect {dialect}"` when the pack is compiled but
    nothing is registered for the dialect.
- A `Gap` file node carries `parser_version = LEXICAL_PARSER_VERSION` and
  `language = dialect.language`. `layer_is_current`, `parser_version_matches` and the
  `worktree_graph` currency check treat `Gap` exactly like `Unknown`, so a base holding
  a gap-dialect file is `Reused` on the next `ensure_snapshot`.
- `Unknown` keeps today's lexical fallback. `lexical::language_for_path` derives its
  tag from `dialect_for_path`, falling back to `NodeLanguage::Other(ext)`.
- `ExtractorIdentity` gains `pub dialect: &'static str`, and `to_parser_version()`
  renders `"{dialect}:{grammar}+{digest12}+v{n}"`. TS and TSX therefore never share a
  parser version.

### 5.4 Grammar packs

Cargo features:

- `default = ["source-graph", "source-graph-wave-b", "source-graph-wave-c"]`
- `source-graph = [<existing deps>, "dep:tree-sitter-javascript"]`
- `source-graph-wave-b = ["source-graph", "dep:tree-sitter-java", "dep:tree-sitter-c-sharp", "dep:tree-sitter-ruby", "dep:tree-sitter-php"]`
- `source-graph-wave-c = ["source-graph", "dep:tree-sitter-c", "dep:tree-sitter-cpp"]`

Dependency rules:

- Every new dependency is `optional = true`, added with `cargo add <crate>@=<version> --optional`
  and pinned exactly.
- The `[features]` table has no `cargo` command, so it is the one hand-edited block.
- `--no-default-features` stays the supported degraded build (lexical only).
- A pack that is not compiled reports its dialects as named gaps (5.3).

## 6. Extraction: capture protocol and local binding

### 6.1 Capture protocol additions

These are handled in `extract/treesitter/collect.rs`.

| Capture | Meaning |
| --- | --- |
| `@import.statement` | the whole import statement. Its text and the `@import.path` text go to `QueryHarness::import_bindings(statement, path, site) -> Vec<ImportBinding>` (default: one binding `{path, name: None, alias: None, glob: false}`) |
| `@call.receiver` | in the same match as `@call.name`: the receiver expression of a member call |
| `@reference.name` | a non-call use that becomes a `References` edge (JSX element names, PHP trait `use` names, and C/C++ function prototypes that sit directly under the translation unit, a declaration list, a linkage specification or a preprocessor block (C++ also a template or friend declaration)) |
| `@definition.qualifier` | in the same match as a `@definition.*`: a qualifier written on the definition itself (`W` in C++ `void W::run() {}`, `A` in Ruby `class A::B`). Its text, split on `::` and `.` after `normalize_call`-style cleanup, is inserted into the scope before the definition's own name |

`QueryHarness` gains three methods:

- `fn import_bindings(&self, statement: &str, path: &str, site: Span) -> Vec<ImportBinding>`,
  with the default above;
- `fn self_receivers(&self) -> &'static [&'static str]`, defaulting to the dialect's
  `self_receivers` column (section 5.1);
- `fn top_level_self(&self) -> bool`, default `false`, overridden to `true` by Ruby alone (a top-level `self` is the `main` object).

### 6.2 Local binding rules

These apply in `build.rs::call_edges` and a new `reference_edges`. The spelling map
becomes `BTreeMap<String, Vec<String>>`: every id that answers to a spelling,
deduplicated, in source order.

1. **Receiver call.** The receiver is in `self_receivers()`. Type `T` is the scope,
   joined with `::`, of the innermost enclosing definition whose kind is `Type`,
   `Implementation` or `Interface`. When no such definition encloses the call and the
   enclosing function carries a `@definition.qualifier`, `T` is that function's scope
   minus its last segment (C++ `void W::run() { this->m(); }` gives `W`). If `T::name`
   has exactly one id in the file, bind it with `Receiver`. Otherwise emit `Syntax`
   with the receiver kept, and with candidates when two or more ids exist. When
   neither yields a `T`, emit `Syntax` with the receiver kept and no candidates, as for any other receiver (rule 2): Python module-level `def attach(self): self.run()` and JS `function Widget() { this.render(); }` bind nothing. The one exception is a harness whose `top_level_self()` is true, which is Ruby alone: a top-level `self` is the `main` object, whose methods are the file's top-level `def`s, so there the call follows rule 3.
2. **Other member call** (`obj.m()`, `ns.m()`). Never bound at extraction. Emit
   `Syntax` with `receiver` kept; the resolver decides.
3. **Plain or qualified call** (`f()`, `Widget::new()`).
   - Import first: when the spelling's first segment equals the `local_name()` of a
     non-glob `ImportBinding` of the file, emit `Syntax` unresolved with no
     candidates. The resolver's rule 4 decides; it never revisits an edge that has
     candidates.
   - Owner carried only by impls: when a qualified spelling's owner (the segment
     before the name) answers in the file only to `Implementation` ids, emit `Syntax`
     unresolved with no candidates; the resolver's rule 1 decides with the whole
     graph. An `impl` does not define its type: `String::from("hi")` beside `impl
     From<Name> for String` is not bound, while `struct Widget` + `impl Widget { fn
     new() }` + `Widget::new()` binds `LocalName`.
   - Look up the spelling. None: emit `Syntax` unresolved.
   - Scope eligibility applies before counting, also for a single id: the spelling
     map answers a bare `helper` for a nested `hidden::helper`, so a hit alone proves
     nothing. An id is eligible when its anchor scope (its scope minus the spelling's
     segments; for a bare name, the parent scope) is a prefix of the caller's scope.
     Exactly one eligible id with the longest anchor binds with `LocalName`.
     Otherwise emit `Syntax` with all ids as `candidates`, so one ineligible id gives
     a candidate list of length one, never a bind.
4. **Reference (`@reference.name`).** Same as rule 3, with kind `References` and
   unresolved confidence `0.5`.

   Rules 3 and 4 for a spelling with no qualifier: when the file's dialect has
   `bare_calls_reach_members == false`, drop every id whose innermost enclosing
   definition is a `Type`, `Implementation` or `Interface`, or that is a function carrying a `@definition.qualifier` (a Go method), before counting. In Rust,
   `impl W { fn parse(&self) {} fn load(&self) { parse(); } }` gives a `Syntax` edge
   to `UNRESOLVED_TARGET` with no candidates. Qualified spellings (`W::parse()`) keep
   members.

   A Rust `Self::helper()` is captured as `@call.receiver` = `Self` plus
   `@call.name` = `helper` and follows rule 1. The whole-path call pattern excludes
   `Self` paths.
5. **Import statement.** Emit one `Imports` `Syntax` edge from the file to
   `UNRESOLVED_TARGET` (symbol = path, one site), plus `FileExtraction.imports` from
   `import_bindings`.

Every emitted `Calls`, `Imports` and `References` edge carries the span of its
`@call.name`, `@import.path` or `@reference.name` node in `sites`.

### 6.3 Coverage meaning

Two statuses change meaning:

- `FileCoverage::Full` now means the configured syntax pass completed without unnamed
  definition matches. It does not mean the call graph is exhaustive. Update its
  docstring to say so.
- `FileCoverage::ParseError` keeps its current behaviour: no symbol nodes.

## 7. Cross-file resolution

This section applies to `context/resolve*`, stage `binding-resolver`.

Resolution acts only on `Syntax` edges that are unresolved (`to == UNRESOLVED_TARGET`).
It never rewrites a bound edge. It never touches an unresolved `Syntax` edge whose
`candidates` is non-empty on entry: same-file ambiguity found at extraction (6.2) is
final, records no keys, and is never unbound by relink.

Indexes are keyed by `(family, name)`:

- `SymbolIndex` covers definitions;
- `PathIndex` covers module paths;
- a namespace index maps declared namespaces and packages (C# `namespace`, Java
  `package`, PHP `namespace`) to files.

Rules run in order for each unresolved `Calls`/`References` edge in file `F`, with
family `fam`:

1. **Qualified spelling** (`a::b::n`, `A.B.n` after normalisation). When the first segment is the `local_name()` of a non-glob binding in `F.imports`, rule 4 decides first. If the module resolves to no file (external), the edge stays unresolved with no candidates. If the files do not define the rest, the edge is handed on. Then match the spelling against node scopes, longest first. A leading segment may be dropped only when the dropped part names something in the graph: a Rust root (`crate`, `self`, `super`), the empty global segment, or a module path the dialect conventions map to files. Failing that, map the qualifier to module files through the dialect's path conventions and look up `n` inside them. Exactly one hit binds with `Import`, unless the matched spelling's owner (the segment before `n`) is carried in `fam` only by `Implementation` nodes: then the hits become candidates and nothing binds, and the edge records `name:{fam}:{owner}`. An `impl` block does not define its type, so `String::from` never binds the `from` of a local `impl From<Name> for String`. A qualifier naming nothing in the graph stays a gap: `std::io::Error::new()` never binds a local `Error::new`.
2. **Receiver not in `self_receivers`.**
   - If it is the local name of a whole-module binding (`name == None`, not glob) in
     `F.imports`, resolve the module to files and look up the member. Exactly one hit
     binds with `Import`.
   - Otherwise it is a dynamic receiver: never bind. Record same-family candidates
     named like the member (at most 8, else none).
3. **Self receiver** (`self.m()`). Take the definitions whose scope ends in `T::m`
   within the family, in the files that may hold part of `T`:
   - Rust `impl` blocks: within one crate, meaning the same deepest crate-root
     directory.
   - C# `partial` classes: within one namespace, meaning files sharing a declared
     namespace, or both declaring none.
   - Ruby reopened classes and C++ out-of-line definitions: anywhere.
   - Every other dialect: only `F` itself, and only definitions whose scope is exactly `T`'s whole scope plus `m`, so an inherited member stays unbound and a nested namesake (`A.Meta` beside `B.Meta`) is never `T`.

   Exactly one binds with `Receiver`; two or more become candidates. Failing that,
   the traits a PHP type uses (`References` edges from the type node) are searched
   across files. No other dialect reads those edges as trait uses. With no member
   found, same-family definitions named `m` become candidates.
   - The self receivers come from the from-node's dialect
     (`dialect_for_path(path).self_receivers`).
   - `T` is the innermost proper prefix of the from-node's scope that names a `Type`,
     `Implementation` or `Interface` node in the same file. In a dialect that splits
     types across files, candidates match on that node's own last name segment, so
     `impl Widget` at top level in one file matches `example::Widget` in an inline
     module of another file of the same crate; every other dialect matches `T`'s whole
     scope.
4. **Named or aliased import.** `F.imports` has a binding whose `local_name()` equals
   the called name. Resolve `binding.path` to module files and look up
   `binding.name.unwrap_or(called name)`. Exactly one binds with `Import`. If the
   module does not resolve to any file, it is external: leave the edge unresolved and
   **refuse** rules 6 and 7. When the module files define nothing named `n`, a non-glob binding in those files whose `local_name()` or `exported_as` equals `n` is followed (at most 4 hops). A member looked up through an imported item matches `item::m` only; the bare `m` is tried only for a Rust binding whose files are `item.rs` or `item/mod.rs`.
5. **Package or namespace scope.** Look up the name among the files of `F`'s package:
   the same directory for Go and Java, the same declared namespace for C# and PHP.
   Exactly one binds with `Import`.
6. **Glob imports.** Look up the name in the files of every glob binding whose module
   resolves. Exactly one binds with `Import`. If some glob binding in `F` does not
   resolve (external), refuse rule 7.
7. **Unique name.** Exactly one definition of the name in `fam`, and no refusal fired:
   bind with `UniqueName`. Two to eight definitions become `candidates`.

Rules 5, 6 and 7, and the candidates a refusal records, drop every member definition when the edge's dialect has `bare_calls_reach_members == false`, as extraction does (6.2 rule 3). A definition is a member when its owner scope (its scope minus its last segment) is the scope of a same-family `Type`, `Implementation` or `Interface` node in any file, or when such a node contains it, or when it is a Go function scoped under an owner (a method, whether or not the graph holds its receiver type's node). So a bare Go `run()` never binds the method `Widget::run`, and a Rust bare `parse()` never binds `W::parse` by unique name. The edge records `name:{family}:{owner}` for each owner consulted.

A refused edge (rule 4 or 6 refusing rule 7) still records one to eight same-family
candidates named like the call, as rule 2 does, so impact recall survives an external
glob. More than eight leave the edge plain unresolved.

Imports edges resolve to file node ids through the dialect path conventions as today
(`Import` provenance).

The self-candidate exclusion from `resolve_edge` stays: a lone candidate equal to the
edge's `from` resolves nothing.

`ResolutionStats` drops `Copy`, derives `Serialize` and `Deserialize`, and gains
`by_provenance: BTreeMap<String, usize>` (owned keys, so it deserializes). `ambiguous`
counts edges left with `candidates`. `retargeted` counts edges whose provenance is
`Import`, `Receiver` or `UniqueName`, extraction-time `Receiver` binds included; the
field's docstring says so.

### 7.0 Resolution API (stage `binding-resolver`; used by stage `resolved-view`)

```rust
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EdgeRef { pub path: String, pub index: usize }            // index into FileEntry.edges
pub type EdgeKeys = BTreeMap<EdgeRef, BTreeSet<String>>;             // lookup keys each edge consulted
pub fn resolve_graph(graph: &mut ResolvedGraph) -> ResolutionStats;   // returns resolution_stats(graph) computed after resolving
pub fn resolve_graph_recording(graph: &mut ResolvedGraph) -> (ResolutionStats, EdgeKeys);
pub fn resolve_edges(graph: &mut ResolvedGraph, edges: &BTreeSet<EdgeRef>) -> EdgeKeys; // only these edges; indexes over the whole graph
pub fn resolution_stats(graph: &ResolvedGraph) -> ResolutionStats;   // pure count over the current edges
pub fn touched_keys(entry: &FileEntry, added_or_removed: bool) -> BTreeSet<String>;
```

Lookup keys:

- `name:{family}:{name}`: every definition-name lookup (rules 1–7). `{name}` is the
  last `::` or `.` segment of the looked-up spelling.
- `ns:{family}:{namespace}`: namespace-index lookups.
- `pathset:{family}`: every module-path or package-scope lookup. Path resolution
  depends only on which files exist, so a key is touched only when a file is added or
  removed.

Every path-convention function (section 7.1) takes `keys: &mut BTreeSet<String>` and
records `ns:{family}:{namespace}` whenever it consults the namespace index and
`pathset:{family}` for every module-path or package-scope lookup.

One shared function in `resolve/record.rs` computes the keys a node answers to.
`SymbolIndex::build` and `touched_keys` both call it, for every node kind the index
holds (File nodes by file name and stem, Implementation nodes included), so relink
selects every edge a cold build would re-resolve. `touched_keys` returns:

- the `name:` keys of every node in `entry`, from that function;
- the `ns:` key of every `Module` node;
- `pathset:{family}` when `added_or_removed`.

`resolution_stats` and the stats `resolve_graph` returns are equal for the same graph
by construction. `node_names(&SourceNode) -> Vec<String>` stays: `map/views/mod.rs`
and `verify/goal_backward/reachable.rs` use it.

### 7.1 Path conventions per dialect

These live in `resolve/paths/mod.rs` (moved from `resolve/paths.rs` with `git mv`),
split into further `resolve/paths/` files as needed. `module_files` returns only files
whose dialect is in the importer's family, so a verbatim `./styles.css` probe never
matches.

| Dialect | Module spec to candidate files |
| --- | --- |
| rust | `crate::`, `self::`, `super::` anchored as today. A non-anchored path (`x::y`) is internal only when its first segment names a module file (`<seg>.rs`, `<seg>/mod.rs`) or directory under the citing file's own crate root (the deepest directory holding `lib.rs` or `main.rs` that contains the file) or under the citing module's directory; otherwise `module_files` returns empty and the import is external (`use serde::de::*` never matches a local `de.rs`). In a workspace `use log::info;` in one crate never matches another crate's `log.rs`. |
| typescript, tsx, javascript | Relative `./x`, `../x` are resolved against the importing file's directory, then `x`, `x.ts`, `x.tsx`, `x.js`, `x.jsx`, `x.mjs`, `x.cjs`, `x/index.{ts,tsx,js,jsx}`. Bare specifiers (`react`) are external |
| python | Dotted `a.b` becomes `a/b.py` or `a/b/__init__.py` (suffix match). Relative `.x`/`..x` resolve against the importing package. A one-segment spec is suffix-matched anywhere, so a stdlib or third-party module sharing a project file's name is taken for that file. |
| go | Import path suffix matched against directories; candidates are every `.go` file in the matched directory |
| java | `a.b.C` becomes `a/b/C.java` (suffix match); `a.b.*` becomes every `.java` in `a/b/` |
| csharp | `using A.B;` becomes the files declaring namespace `A.B` (namespace index); `using X = A.B.C;` is an alias binding |
| ruby | A spec starting `./` or `../` (from `require_relative`, 3.4) resolves against the importing file's directory only (`x.rb`). A bare spec (`require`, `load`) resolves to `lib/x.rb` when it exists, else to every `x.rb` suffix match |
| php | `use A\B\C` becomes `A/B/C.php` (suffix match, PSR-4 shape); `require`/`include` string paths are resolved relative to the file |
| c, cpp | `#include "x.h"` (path `x.h`) is resolved against the including file's directory, then suffix match; `#include <x>` (path `<x>`, 3.4) is external |

## 8. Coverage dimensions

`context/coverage.rs` is split into `context/coverage/{mod.rs, dialects.rs}`.

`CoverageReport` keeps its current fields and adds:

- `bytes`, `symbol_level_bytes`, and `unsupported_files`/`unsupported_bytes` (no
  dialect for the extension);
- `by_dialect: BTreeMap<String, DialectCoverage>`, one entry per dialect with at least
  one file:
  - `files`, `bytes`, `symbol_level_files`, `symbol_level_bytes`,
    `files_by_status`;
  - `edges_by_provenance`, `unresolved_edges`, `ambiguous_edges`;
  - `extractor`: `"registered"`, `"pack {feature} not compiled"` or
    `"no extractor"`;
  - `capabilities: Option<Capabilities>`;
- `gaps`: for each gap dialect, `(dialect, detail, files, bytes)`.

A file's bytes come from its file node span (`end_byte`), and `Oversized` reports its
own `bytes`.

`Display` (the `loom map` footer) adds bytes to the symbol-level share
(`52% of files, 61% of bytes symbol-level`) and one short `gaps:` clause when gaps
exist. `footer_json` carries every new field.

## 9. Freshness states and the reconcile lease

`GraphState` lives in `context/freshness.rs`:

```rust
pub enum GraphState { Current, Stale, NeverBuilt, Unavailable }
impl GraphState { pub fn as_str(self) -> &'static str } // "current" | "stale" | "never built" | "unavailable"
impl Freshness { pub fn state(&self) -> GraphState; pub fn unavailable(detail) -> Self }
```

`Freshness` gains `#[serde(skip)] pub unavailable: bool`, never persisted. `state()`
returns:

- `Unavailable` if `unavailable`;
- else `NeverBuilt` if `revision` is empty;
- else `Stale` if `stale`;
- else `Current`.

The producer of `unavailable` is `refresh::semantic_freshness_against_head`: when
`source_graph::head_revision(project_root)` returns `None` it returns
`Freshness::unavailable(detail)` (revision kept) instead of the stored value.

Surfaces:

- The Knowledge Brief header (`orchestrator/signals/format/brief.rs::freshness_word`)
  and `loom knowledge context` print `state().as_str()`.
- `retrieve/graph.rs::degraded_reason` keeps its signature. Its never-built reason
  fires only when `semantic_revision` is empty AND `graph.files` is empty, with the
  fixed text `source graph never built; run loom map to build it`. An overlay-backed
  read with an empty semantic revision is not degraded.
- `SnapshotOutcome` gains:
  - `persisted: bool`, false when `GraphStore::fall_back_to_memory` served the layer;
  - `serving: Option<String>`: when the build failed and an older base exists,
    `ensure_snapshot` sets `revision` to that base and `serving` to its revision.
- `SnapshotOutcome::state() -> GraphState` returns:
  - `Current` for `Reused`/`Updated`/`Rebuilt`;
  - `Stale` when `serving` is set;
  - `NeverBuilt` when the build failed and no base exists;
  - `Unavailable` when the working tree cannot be inspected (not a git work tree, or
    git missing), which `ensure_snapshot` reports today as an inspect failure.
- `describe()` appends `; not persisted (cache read-only)` when `!persisted`, and
  `; serving stale base <rev8>` when serving.

Reconcile hook (`commands/hook/reconcile_graph.rs`, `.../lock.rs`):

- `pub fn wants_rebuild(freshness: &Freshness, degraded: Option<&str>) -> bool` has one
  truth table:

  | `freshness.state()` | `degraded` | result |
  | --- | --- | --- |
  | `Stale` | any | true |
  | `Current` | `Some` | true |
  | `Current` | `None` | false |
  | `NeverBuilt` | any | false |
  | `Unavailable` | any | false |

  `spawn_if_needed` calls it as
  `wants_rebuild(&pack.semantic_freshness, pack.degraded.as_deref())`, and nothing else
  decides the spawn.
- A never-built graph is built only by explicit commands: `loom init`, `loom run`,
  `loom map` and `loom knowledge sync`.
- The lock file is `"<epoch> <pid> <failures> <pending>"`. A line that does not parse
  as four fields is treated as no lock. Every lock write goes through
  `fs::locking::locked_write` (a directory lock plus an atomic rename), never through a
  remove, `create_new` and `write_all` sequence: between those steps a reader sees no
  file or an empty one, `decide` returns `Spawn`, and a second reconcile runs beside
  the live holder. Every write is read-modify-write under that one lock and carries
  `failures` and `pending` forward, so the backoff counts failures across spawns and a
  `pending` mark is never lost.
  - A finished run with `failures > 0` is debounced for
    `debounce_secs * 2^min(failures, 5)`, capped at 21600 s.
  - A success resets `failures` to 0.
  - `reconcile()` returns the `SnapshotOutcome`; a run counts as failed when
    `outcome.state() != GraphState::Current` (serving a stale base means the build
    failed).
  - A request skipped while a live holder runs sets `pending = 1` through
    `mark_pending`, which keeps the holder's epoch and pid.
  - The holder keeps its pid through every pass. After a pass, under the lock, it
    reads the line:
    - a line naming another pid (a takeover) is left as is, and the holder exits;
    - `pending == 1`: it writes its own pid, a fresh epoch, `pending 0` and the pass's
      failure count, then runs one more pass (a bounded queue of one);
    - `pending == 0`: it writes `pid 0` with the final `failures`.
  - Takeover of a dead or stale holder is a read-modify-write under the same lock.
  - Failure accounting is internal. `loom hook reconcile-graph` exits 0 whatever the
    outcome.
  - A failed build keeps `action: SnapshotAction::Unavailable`, with or without
    `serving`, so bootstrap's `require_snapshot` still refuses it.
- `loom hook reconcile-graph --cancel`:
  - it sends `SIGTERM` to a live holder pid only when the pid's NUL-split argv (from
    `/proc/<pid>/cmdline`, or `ps -p <pid> -o command=`) contains both `hook` and
    `reconcile-graph`, and the process started no later than the lock's epoch. Any
    other pid (a reused pid naming the daemon or `loom stage complete`) is left alone;
  - it records the run as finished (`pid 0`) with `failures` unchanged, also when it
    left the pid alone;
  - with no live holder it is a no-op, and exits 0 either way.
  The existing takeover of a dead or stale holder stays.

## 10. `loom map` API

Views:

- `--outline <PATH>`: unchanged row format `L<a>-L<b>  kind  name` (the contract of
  `loom-hooks/_read_discipline.sh`).
- `--find-all <SYMBOL|ID>`: honours `--limit` (default 50) and `--path`/`--lang`
  (applied while scanning). `FIND_ALL_CAP` is removed.
- `--callers <SYMBOL|ID>` / `--callees <SYMBOL|ID>`:
  - Scope is direct `Calls` edges only, one hop.
  - Each row is `caller path:<call line> → target path:<decl line>  symbol=<written>  <provenance> <confidence>  sites=<n>`.
  - Candidate edges are flagged `candidate (N total)`.
  - Help: "Print direct callers (one hop over call edges) with their call sites",
    and the callee form likewise.
- `--references <SYMBOL|ID>`: new. Direct incoming `References` edges, same row
  shape.
- `--impact <SYMBOL|PATH|ID>`:
  - Transitive reverse reachability.
  - Default `--kinds` is `calls,references,implements,extends`; `contains` and
    `imports` only when listed.
  - New `--evidence <LIST>` restricts traversal to the listed provenance classes.
  - `--min-confidence` stays. Both apply during traversal.
- `--window <ID>`: new. Exact source of a node id or a site id
  (`path@start-end`), capped by `--window-lines` (default 60).
  - An id naming no graph node or graph file prints `unknown id: <id>` on stderr and
    exits 2. The exit-code mapping (2 unknown id, 3 changed file) lives in
    `commands/map.rs`.
  - Header:
    `path:L<a>-L<b>  <state> <base rev8>[+<generation8>]  hash <match|changed>`.
  - The file is read from disk and its `body_hash` compared with the entry's
    `content_hash`. On mismatch it prints `changed since snapshot` and exits 3 without
    printing bytes.
- `--census [--root <DIR>]...`: section 10.1.
- `--eval-edges <DIR> [--json] [--thresholds <FILE>]`: section 14.2, stage
  `edge-quality-eval`. It exits 2 when the directory holds no labelled corpus, 1 when
  any threshold fails, 0 otherwise, and always prints the summary line
  `<n> corpora, <f> failing`.
- `--timings`: prints phases to stderr (`snapshot`, `load`, `resolve`, `query`,
  `render`, `total`, and `peak_rss_kb` from `getrusage` `ru_maxrss`; the stderr text
  contains the word `total`), and adds a `timings` object to JSON. The object holds
  numeric `snapshot`, `load`, `resolve`, `query`, `render`, `total` and `peak_rss_kb`
  values, never an empty or partial object.

Matching:

- An argument containing `#` is an exact node id (or a file node id). It never
  substring-falls-back.
- Otherwise an exact name is tried, then the case-insensitive substring fallback. The
  fallback is labelled in every view: human `(substring matches)`, JSON
  `"match": "id" | "exact" | "substring"`.

Filters:

- `--path <PREFIX>` is component-aware (`src/a` does not match `src/ab`).
- `--lang <DIALECT>` is new.
- For impact, callers, callees and references both apply to displayed hits after
  traversal. JSON reports
  `"filters": {"path": {"value", "applied": "display", "filtered_out": n}, "lang": {...}}`;
  find-all reports `"applied": "scan"`. `ImpactOptions` keeps `limit` and `path_prefix`
  with their post-traversal semantics (`impact.rs`, after `walk.finish()`), and
  `ImpactResult` gains `filtered_out: usize`, the count `path_prefix` removed, which
  the view renders as `filters.path.filtered_out`.

Truncation: human and JSON both carry `suppressed_starts` (ambiguous starts beyond
`IMPACT_MAX_STARTS`) and `suppressed` (hits beyond `--limit`).

Neighbour rows in JSON (`views.callers.neighbors[]`, `callees`, `references`) keep
today's keys (`id`, `kind`, `path`, `edge_kind`, `confidence`, `provenance`,
`line_start`, where `line_start` is the neighbour's declaration line) and add:

- `symbol`;
- `site_path`;
- `sites`: an array of `{"line_start", "line_end", "start_byte", "end_byte"}`;
- `candidate_of`: a number or `null`.

Impact hits add `via_candidates`, and every view object adds `match`.

JSON top level:

```json
{"schema": "loom-map/2",
 "snapshot": {"state": "current", "base_revision": "...", "overlay": {"plan": "...", "stage": "..."} | null,
              "generation": "...", "built_at": "...", "persisted": true, "schema_version": 2,
              "extractors": {"rust": "rust:0.24.2+...+v3", ...}},
 "views": {...}, "coverage": {...}}
```

Stage `resolved-view` adds `"resolver_version"` and `"view": "materialized" | "built"` to
`snapshot`. `footer_json` renders `ResolutionStats.by_provenance` beside `retargeted`,
`ambiguous` and `unresolved`, so the field has a reader.

- On a failed build with an older base, map serves that base with
  `state: "stale"`.
- With no base it prints `source graph never built: <reason>` on stderr, emits
  `state: "never built"`, and exits 0 with empty views.

### 10.1 Census

The census lives in `context/census/`.

- **Roots.** Default is the current project. Each `--root` must be a git work tree.
- **Enumeration.** `git ls-files -s -z` through `git::runner::run_git` (untrimmed
  bytes, not `run_git_checked`), reading each entry's mode. A symlink (`120000`) or a
  gitlink (`160000`) is `unsupported` and never followed, as the graph builder already
  skips them (`refresh/source_graph/enumerate.rs`). A file's size comes from
  `symlink_metadata`, and at most 1024 bytes are read for the generated markers. The
  graph builder's Oversized cap applies before any in-memory extraction of another
  root. Then classify every path. `EXCLUDED_ROOTS` and `excluded` are reached through
  `crate::context::refresh::{excluded, EXCLUDED_ROOTS}`:
  - `excluded`: the first segment is in `EXCLUDED_ROOTS`;
  - `vendored`: a path segment is one of `vendor`, `third_party`, `third-party`,
    `node_modules`, `bower_components`, `Pods`, or `git check-attr linguist-vendored`
    is set;
  - `generated`: `git check-attr linguist-generated` is set, or the first 1024 bytes
    contain `@generated`, `DO NOT EDIT`, `Code generated` or `autogenerated`;
  - `eligible`: the extension has a dialect;
  - `unsupported`: everything else, grouped by extension.
- **Attributes.** `git check-attr --stdin -z linguist-vendored linguist-generated` is
  the one exception to `run_git_checked`, which cannot feed stdin. Spawn git with the
  runner's `NO_HOOKS_ARGS`, piped stdin and stdout, and write stdin from a separate
  thread while the caller reads stdout, so thousands of long paths neither exceed
  `ARG_MAX` nor deadlock on a full pipe. Do not copy
  `verify/criteria/cache_ignore.rs`, which feeds stdin but discards stdout.
- **Subprojects.** Group by the nearest ancestor directory holding a manifest:
  `Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`, `setup.py`, `pom.xml`,
  `build.gradle`, `build.gradle.kts`, `*.csproj`, `*.sln`, `Gemfile`, `composer.json`,
  `CMakeLists.txt`, `meson.build`. The root is the fallback.
- **Coverage.**
  - The current project uses its resolved graph.
  - Any other root is extracted in memory through the registry, never written to a
    cache, and reports `files_parsed`, `bytes_read` and elapsed time.
- **Report.** Per root and subproject, per dialect: files, bytes, symbol-level
  files/bytes, parse errors, extractor status. Totals report eligible, excluded,
  vendored, generated and unsupported, by files and by bytes. Coverage percentages
  use eligible files and eligible bytes as denominators, and the excluded counts are
  printed beside them.
- `--json` emits it with `"schema": "loom-census/1"`. The top-level `totals` object
  has one object per class, each shaped `{"files": n, "bytes": n}`:
  `{"eligible", "excluded", "vendored", "generated", "unsupported"}`.
  - Classification order is excluded, vendored, generated, eligible, unsupported. A
    vendored or generated file is never eligible.
  - `roots[]` holds per-root rows, and each root has `subprojects[]` with
    `by_dialect` objects and `unsupported_by_extension`.

Task frequency by language, and the share of source queries ending in `rg` or a
manual read, come from agent sessions. They are part of the operator protocol
(section 14.3), not the census.

## 11. Source windows

`context/window.rs`:

```rust
pub struct SourceWindow { pub path: String, pub span: Span, pub text: String, pub truncated: bool }
pub enum WindowError { UnknownId(String), ChangedSinceSnapshot { path: String }, Unreadable { path: String, detail: String } }
pub fn read_window(graph: &ResolvedGraph, project_root: &Path, id: &str, max_lines: usize) -> Result<SourceWindow, WindowError>
```

- The id is a node id, or a site id `path@start-end`, where `start` and `end` are byte
  offsets (`start_byte`, `end_byte`).
- The function reads the file, requires `body_hash(bytes) == entry.content_hash`,
  slices the span's lines (byte range expanded to whole lines), and truncates to
  `max_lines`.
- It never panics on a span that falls outside the file or splits a UTF-8 character:
  it slices on whole lines, using lossy UTF-8.

## 12. Resolved view

`context/view/` (`mod.rs`, `identity.rs`, `build.rs`, `incremental.rs`, `deps.rs`,
`store.rs`):

```rust
pub const RESOLVER_VERSION: u32 = 1;
pub struct ViewIdentity { pub schema_version: u32, pub base_revision: String, pub overlay_generation: String,
                          pub extractor_digest: String, pub resolver_version: u32 }
pub struct ResolvedView { pub identity: ViewIdentity, pub graph: ResolvedGraph, pub stats: ResolutionStats,
                          pub deps: DependencyIndex,
                          #[serde(skip)] pub origin: ViewOrigin }
pub enum ViewOrigin { Materialized, Built }   // canonical_bytes never sees it; map prints it as `"view"`
pub fn build_cold(graph: ResolvedGraph, identity: ViewIdentity) -> ResolvedView;
pub fn relink(previous: &ResolvedView, next: ResolvedGraph, identity: ViewIdentity) -> ResolvedView;
pub fn canonical_bytes(view: &ResolvedView) -> Result<Vec<u8>>;   // canonical_json of the whole view
impl ViewIdentity { pub fn current(base_revision: &str, overlay_generation: &str) -> Self } // current schema, extractor digest, RESOLVER_VERSION
impl GraphStore {
    pub fn view(&self, revision: &str, overlay: Option<(&str, &str)>) -> Result<ResolvedView>;
    pub fn view_path(&self, identity: &ViewIdentity, overlay: Option<(&str, &str)>) -> PathBuf;
}
```

- `ResolvedGraph` passed to `build_cold`/`relink` holds extraction-time edges. Both
  functions run resolution themselves. `ResolvedGraph` derives `Serialize` and
  `Deserialize` (one derive line in `graph_store/layer_types.rs`; `FileEntry` already
  has serde), and `ResolutionStats` and `EdgeRef` carry serde from design 7.
- The module is public: `loom::context::view::{...}`.
- Any later change to resolution rules 1-7 bumps `RESOLVER_VERSION`.
- The view persists no name or adjacency index. Queries scan the parsed graph in one
  linear pass. A persisted derived index would grow the JSON every warm query parses,
  so the storage-engine decision, taken on `--timings` data, decides both.
- `relink` copies a previous entry whose `content_hash` is unchanged. That is sound
  only because the identity gate below guarantees the same extractor and resolver
  produced it: equal bytes under a different extractor digest can extract
  differently.

- `extractor_digest` is the `sha256` over the sorted `"{dialect}={parser_version}"`
  lines of the registered extractors.
- **Dependency index.** `resolve_graph_recording`/`resolve_edges` (design 7.0) return
  each edge's consulted keys. `DependencyIndex` inverts them, mapping each key to the
  `EdgeRef`s that consulted it, and is persisted with the view. After removing the
  `EdgeRef`s of changed, removed and re-resolved edges, `relink` drops every key whose
  list is empty; a cold build never has an empty key.
- **`relink`.** It relinks only when `previous.identity` has the same
  `schema_version`, `extractor_digest` and `resolver_version` as `identity`; with any
  difference it returns `build_cold(next, identity)`. A relink starts from `previous`
  for files whose `content_hash` is unchanged and from `next` for changed, added and
  removed files. The relinked graph takes `base_revision` and `overlaid` from `next`,
  never from `previous`. It then unbinds and re-resolves:
  - every edge originating in a changed or added file;
  - every edge whose consulted keys intersect `touched_keys` of the old and new
    entries of every changed, added or removed file;
  - every edge bound to, or listing as a candidate, a node in a changed or removed
    file.

  Extraction-time candidate edges (an unresolved `Syntax` edge with candidates at
  extraction, design 7) are never selected: relink never unbinds or re-resolves them.
  Stats come from `resolution_stats`, never from summing partial runs.
- `canonical_bytes(relink(...))` must equal `canonical_bytes(build_cold(...))` for
  every change. A property test drives scripted edit sequences through both, and a
  contract test pins one case.
- **Persistence.**
  - A base view goes to `<cache>/graph/view/<revision>-<identity digest12>.json`.
  - A stage or local overlay view goes to `<work>/context/<plan>/<stage>/view.json`.
  - Writes use `locked_write`, and permission-denied writes fall back to memory
    exactly as layers do: the memory fallback gains a
    `view_fallback: RefCell<HashMap<PathBuf, ResolvedView>>` field (initialized in
    `GraphStore::new`), and `fall_back_to_memory`, `read_layer_or_memory` and
    `is_write_denied` are `pub(crate)` so `view/store.rs` can use them. Paths come from
    the existing `pub` `base_dir()` and `overlay_dir(plan, stage)`; no `graph_root()`
    or `overlay_root()` accessor is added.
  - A view whose identity differs from the one requested, or that fails to parse, is
    rebuilt. It is never served.
  - A view is never persisted for a revision with no base layer: when
    `load_base(revision)` is `None` (the empty graph `resolved()` returns for a
    missing base), `GraphStore::view` builds in memory and returns it. `publish_base`
    removes `graph/view/<revision>-*.json` for the revision it publishes.
  - Pruning removes a base's views with the base. The prune also enforces a byte budget
    over `graph/base` plus `graph/view`, oldest unprotected revision first. The budget
    is `RetrievalConfig.graph_cache_budget_bytes` (`context/config.rs`, default 2 GiB),
    read the way `prune.rs` reads `keep_base_graphs`. It applies per main cache
    (`<main>/.loom/cache/context-v1`), shared by every worktree, and `prune_after_publish`
    calls the budget prune.
  - `discard_overlay` removes the overlay's `view.json` with its `graph.json`.
  - Materializing a view deletes the sibling `graph/view/<revision>-*.json` files whose
    identity digest differs (a `RESOLVER_VERSION` bump would otherwise leave orphans
    until the revision is pruned).
  - Concurrent builders of one view are allowed: the bytes are deterministic,
    `locked_write` takes a directory lock plus an atomic rename, and the last write
    wins.
  - Inside a stage sandbox the shared cache is read-only, so an in-stage `loom map`
    reports `persisted: false` and `view: "built"`. Warm-query numbers come from
    TempDir or host runs only.
  - `loom clean` (`commands/clean/base_graphs.rs`) removes `graph/view` entries with
    their base, counts their bytes, and does not return early when `graph/base` is
    empty but `graph/view` is not.
- **Lifecycle.**
  - `ensure_snapshot` materializes the base view after it publishes or reuses a layer
    and, whenever it wrote or reused an overlay layer, the overlay view, so the prompt
    hook never builds and persists a view on its hot path.
  - A base view relinks from the newest older base view when one exists, and is
    built cold otherwise.
  - An overlay view relinks from its base view.
  - Readers call `GraphStore::view(revision, overlay) -> Result<ResolvedView>`,
    which loads, relinks or builds as needed: `loom map`,
    `commands/knowledge/bootstrap/graph.rs`, `context/worktree_graph.rs`, and
    retrieval (stage `retrieval-delivery`).
  - The worktree graph stays read-only: `GraphStore` gains
    `pub(crate) fn load_view(&self, identity, overlay)`, which never writes, and
    `worktree_graph` relinks from it in memory.
  - One `loom map` process parses the view at most once: `ensure_snapshot` stores the
    view it materialized in the `GraphStore`'s in-process view cache, and
    `GraphStore::view` returns that entry when the identity matches.
  - Nothing outside `context/view` calls `resolve_graph` afterwards. The three reader
    files (`commands/map.rs`, `commands/knowledge/bootstrap/graph.rs`,
    `context/worktree_graph.rs`) do not name it, not even in a comment or a `use` line.

## 13. Retrieval

- **Intent.** `context/rank_source/intent.rs` provides
  `pub fn classify(query: &str) -> QueryIntent`:

  ```rust
  pub enum QueryIntent {
      Literal { text: String },
      Relationship { direction: RelationDirection, symbol: String },
      SymbolQuestion { symbol: String },
      General,
  }
  pub enum RelationDirection { Callers, Callees, References, Impact }
  ```

  - `Literal`: the query has a double-quoted span of at least 3 characters that
    contains a space or a non-identifier character.
  - `Relationship`: `(who|what) (calls|uses|references|invokes) X`,
    `callers of X`, `callees of X`, `what does X call`, `usages of X`,
    `impact of (changing )?X`, case-insensitive, with `X` an identifier or
    backticked name.
  - `SymbolQuestion`: the whole query matches
    `^(what does|what is|where is|where's|how does|explain|show me) \`?X\`? (do|does|defined|work|works|implemented)?\??$`,
    case-insensitive.
  - Otherwise `General`.
- **Routing in `rank_source`.**
  - `SymbolQuestion`: when `X` is an exact node name in the graph, admit that node
    with the new reason `SelectionReason::SymbolQuestion`, confidence `Low`, and
    `matched_term_count = 1`. Otherwise nothing extra. Every other query keeps
    today's candidacy rule unchanged; the existing negative tests stay green.
  - `Relationship`: seed on the exact node and add its direct neighbours in that
    direction as `GraphNeighbor` candidates with `via` set, before lexical ranking.
  - `Literal`: multiply source lexical scores by 0.5. The pack gains
    `text_search: Option<TextSearchHint { pattern, command }>`. `PackRequest` carries
    `text_search: Option<TextSearchHint>`, set by `build_pack_request` when
    `classify(&query.text)` is `Literal` (nothing sets it on the pack directly), and
    `pack()` copies it into `ContextPack.text_search`. `TextSearchHint` is defined in
    `schema/pack_types.rs` and re-exported from `schema`, so
    `loom::context::schema::TextSearchHint` resolves.
    - `TextSearchHint::for_pattern(pattern: &str) -> TextSearchHint` (public, in
      `intent.rs`) builds `command` as `rg -n -F -- '<pattern>'`, with every `'` in
      the pattern written as `'\''`. For example, `it's` gives
      `rg -n -F -- 'it'\''s'`.
    - It is rendered in the brief and in `loom knowledge context` as
      `Literal text: the graph does not index bodies; run <command>`.
  - `QueryIntent` and `RelationDirection` derive `Debug`, `Clone`, `PartialEq` and
    `Eq`.
- **Explained neighbours.** `RankedCandidate` gains
  `via: Option<NeighborVia { seed: String, edge_kind: SourceEdgeKind, direction: EdgeDirection, provenance: EdgeProvenance, site_line: Option<usize> }>`.
  `direction` is the edge's direction relative to the neighbour:
  - `Outgoing`: the neighbour calls or references the seed;
  - `Incoming`: the seed calls or references the neighbour.
  `ContextItem` gains `explanation: Option<String>`, rendered as
  `called by \`seed\` at path:L<n>` or `calls \`seed\``, and the other kinds likewise.
- **Token-capped expansion.** `pub const MAX_EXPANDED_TOKENS: usize = 300` in
  `expand.rs`, re-exported as `loom::context::rank_source::MAX_EXPANDED_TOKENS`.
  Expansion stops once the estimated rendered tokens of admitted neighbours would pass
  it, counted with their explanation text. The existing count caps stay.
- **Public surface.**
  - `rank_source.rs` declares `pub mod intent;`.
  - `RankQuery`, `RankedCandidate`, `NeighborVia` and `EdgeDirection { Incoming, Outgoing }`
    live in `context/rank/candidate.rs` (the split keeps `rank.rs` at or below 380
    lines) and are re-exported from `rank.rs`, so they are public in
    `loom::context::rank`. `NeighborVia` derives `Debug`, `Clone` and `PartialEq`, as
    `RankedCandidate` does.
  - `QueryIntent` and `RelationDirection` are public in
    `loom::context::rank_source::intent`.
  - `RankedCandidate.via` is a public field.
- **Caveats.** `ContextItem` gains `caveat: Option<String>`.
  - A source node from a file whose coverage is not `Full` gets `partial coverage`
    and a score factor of 0.6.
  - Every source item in a pack whose semantic state is not `Current` gets
    `snapshot <state>` and a factor of 0.6.
  - Both are rendered.
  - The score factor is applied in `pack.rs` only.
- **Windows.**
  - Stage briefs and `loom knowledge context` attach at most 2 windows of at most 12
    lines each, through `read_window`, stored in `ContextItem.window`.
  - `StageQuery.surface: Surface { StageBrief, Cli, Hook }` tells retrieval which
    surface asked:
    - the stage signal is `Surface::StageBrief`;
    - `loom knowledge context` builds `StageQuery` by literal and sets
      `surface: Surface::Cli`;
    - the prompt hook (`commands/hook/user_prompt.rs`) and the subagent worker brief
      (`commands/hook/worker_brief.rs`) are `Surface::Hook`.
  - Only items with an `ExplicitId`, `ExactSymbol` or `ExactPath` reason qualify, from
    `Full` files, in a `Current` pack.
  - Windows are charged in the rendered token count.
  - The `Hook` surface attaches none.
- **Hook emit floor** (`commands/hook/user_prompt_compose.rs`, anchored on `admits` and
  `clears_item_floor`): an item whose reasons include `SymbolQuestion` passes the floor.
- **Graph source.** Retrieval loads the resolved view (section 12), so expansion sees
  cross-file edges at or above 0.5.

## 14. Edge-quality evaluation

### 14.1 Labelled corpora

The corpora live in `loom/tests/fixtures/source/labeled/<dialect>/`, one small
multi-file project per dialect (all 12). Each has a `labels.yaml`:

```yaml
dialect: rust
declarations:            # every declaration a compiler would see
  - {path: src/lib.rs, kind: function, scope: [Widget, new], line: 4}
references:              # every call/reference site worth labelling
  - {path: src/main.rs, line: 7, symbol: parse, expect: {target: "src/util.rs#function:parse"}}
  - {path: src/main.rs, line: 9, symbol: fetch, expect: {external: true}}
  - {path: src/main.rs, line: 11, symbol: save, expect: {ambiguous: ["...#function:A::save", "...#function:B::save"]}}
impact:
  - {start: "src/util.rs#function:parse", depth: 2, expect: ["src/main.rs#function:main"]}
```

- Labels are written from the language's semantics, meaning what a compiler would
  bind, before the extractor is run. They are never edited to match output.
- A label that exposes an extractor or resolver defect is fixed at the source in the
  same stage.
- Each corpus covers:
  - duplicate same-file names;
  - overloads, where the language has them;
  - aliases;
  - re-exports;
  - receiver calls (`self`/`this`/`Self`);
  - a dynamic receiver;
  - an external import;
  - nested scopes;
  - a syntax-error file;
  - a test file and a production file.

### 14.2 Evaluator and thresholds

`context/eval_edges/` (`mod.rs`, `labels.rs`, `metrics.rs`, `build.rs`):

```rust
pub fn evaluate_dir(dir: &Path) -> Result<EdgeQualityReport>;
pub struct EdgeQualityReport { pub dialect: String, pub declaration_recall: f64, pub target_precision: f64, pub target_recall: f64,
                               pub unresolved_rate: f64, pub ambiguous_rate: f64, pub false_high_confidence: usize,
                               pub impact_false_negatives: BTreeMap<usize, usize>, pub failures: Vec<String> }
```

- **Graph build.** The graph is built in memory from `dir`: every file with a dialect
  is walked (not git), `EXCLUDED_ROOTS` are skipped, files are extracted with
  `extract_file`, and the graph is built with `crate::context::view::build_cold`
  (nothing outside `context/view` calls `resolve_graph`, section 12). No cache is
  written.
- **Metrics.**
  - A declaration matches on `(path, kind, scope)`.
  - A reference label matches an edge from that path whose site covers the labelled
    line and whose `symbol` equals the label's symbol or ends with it after a `::` or
    `.` separator (edge symbols keep the written qualifier: `util::parse`,
    `Widget::new`).
  - A ratio whose denominator is 0 is a failure, never a pass.
  - `target_precision` = correct bound targets / bound labelled edges.
  - `target_recall` = correct bound targets / labels expecting a target.
  - An `external` or `ambiguous` label is correct when the edge stays unbound. For
    `ambiguous` the `candidates` must also equal the labelled set.
  - `false_high_confidence` counts bound edges with confidence at least 0.8 whose
    target differs from the label.
  - `impact_false_negatives[d]` counts expected ids missing from
    `impact_with(start, depth d)`.
- **Thresholds.** They are published in `loom/eval/edge-quality-thresholds.yaml`
  before any holdout project is examined:
  - `declaration_recall: 1.0`
  - `false_high_confidence: 0`
  - `target_precision: 0.95`
  - `target_recall: 0.5`
  - `impact_false_negatives_depth_1: 0`

  The same values apply to every dialect on its labelled fixture. The cargo test
  `every_dialect_meets_published_thresholds` enforces them for every dialect in
  `DIALECTS` whose pack is compiled. `Thresholds::check` also fails a corpus that lacks
  at least one target label, one external label, one ambiguous label, one impact label
  and one `syntax_error_files` entry.
- **CLI.** `loom map --eval-edges <DIR> [--json] [--thresholds <FILE>]` evaluates one
  corpus directory, or a directory of corpora. `--thresholds` defaults to
  `loom/eval/edge-quality-thresholds.yaml`, and `--json` and `--thresholds` are the
  only other flags. It exits 2 when the directory holds no labelled corpus, 1 when any
  threshold fails and 0 otherwise, and always prints the summary line
  `<n> corpora, <f> failing`. `MapArgs` gains `--eval-edges` through clap only.
- **Defect fixes.** A fix to a walk bumps that dialect's `extractor_version`; a fix to
  resolution rules bumps `RESOLVER_VERSION` (`context/view`). Without the bumps,
  cached layers and views keep serving the old output.

### 14.3 Operator protocol

`doc/source-graph-evaluation.md` documents the procedure (not automated) under these
headings:

- `## Census`: the census across consenting projects;
- `## Holdout labelling`: labelling holdout projects;
- `## Agent-task comparison`: the four-mode comparison (`rg`-first, graph alone, graph
  plus `rg`, current Loom guidance) with the same agent, budget and revision,
  randomized order and repeated runs;
- `## Metrics and stratification`: language, task class, coverage tier, freshness;
- `## Release rule`: thresholds are published before holdout results; a dialect or
  resolver capability ships to agent guidance only when fixtures and holdouts pass,
  and graph plus `rg` does not regress its cohort.

## 15. File-size discipline

`cargo test --test maintainability` enforces 400 lines per file and 50 per function.
`loom/maintainability-baseline.txt` is a ratchet file in this plan: no stage edits it.

Files at or near the limit that this plan grows must be split into submodules
**before** they grow:

| File | Lines |
| --- | --- |
| `context/graph_store/mod.rs` | 400 |
| `context/coverage.rs` | 394 |
| `context/schema.rs` | 394 |
| `context/pack.rs` | 390 |
| `context/retrieve.rs` | 399 |
| `map/views/mod.rs` | 389 |
| `context/resolve/tests_resolve.rs` | 398 |
| `context/rank.rs` | 398 |
| `commands/hook/tests_reconcile_graph.rs` | 394 |

New tests go in new `tests_*.rs` files beside the module, never appended to a file
over 350 lines.
