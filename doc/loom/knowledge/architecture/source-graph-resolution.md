# Source Graph Resolution

>

Cross-file binding of source-graph edges: what extraction decides alone, what `context::resolve`
decides with the whole graph, and the refusal rules that keep both honest. Edge classes and
constants are in [Source Graph](source-graph.md#the-honesty-contract).

## Extraction-Time Local Binding

Done in `extract/treesitter/build.rs::call_edges` and `reference_edges`, per file. The spelling map
is `BTreeMap<String, Vec<String>>`: every id that answers to a spelling, in source order.

1. **Receiver call** (`self.m()`, `this.m()`, `Self::m()`, `$this->m()`). The receiver is in the
   dialect's `self_receivers`. `T` is the scope of the innermost enclosing `Type`, `Implementation`
   or `Interface` definition (or, for C++ `void W::run()`, the function's `@definition.qualifier`
   scope). Exactly one id for `T::name` with a scope that EQUALS `T` plus the name binds `Receiver`;
   two or more become candidates; none emits `Syntax` with the receiver kept. When no `T` exists
   (Python module-level `def attach(self)`, JS `function Widget() { this.render(); }`) nothing
   binds, except Ruby, whose `top_level_self()` hook treats a top-level `self` as `main`.
   ECMAScript `this` inside a nested ordinary function is dynamically bound: a matching outer
   member remains a candidate instead of a resolved receiver, including during graph resolution.
2. **Other member call** (`obj.m()`). Never bound at extraction: `Syntax` with `receiver` kept.
3. **Plain or qualified call.** An import-bound first segment, or an owner carried in the file only
   by `Implementation` nodes (`String::from` beside `impl From<Name> for String`), emits `Syntax`
   with NO candidates, because `resolve::rules::eligible` skips any edge that already has
   candidates (extraction candidates are final); otherwise the resolver could never bind split-impl
   Rust (`struct Widget` in one file, `impl Widget` and `Widget::new()` in another). Otherwise the
   spelling is looked up with scope eligibility applied before counting: an id is eligible when
   its anchor scope (scope minus the spelling's segments) is a prefix of the caller's scope. Exactly
   one eligible id with the longest anchor binds `LocalName`; otherwise `Syntax` with all ids as
   candidates, so one ineligible id is a one-member candidate list, never a bind.
4. **Bare calls and members.** When the dialect's `bare_calls_reach_members` is false (Rust, Go,
   TS/JS, Python, PHP, C), ids whose innermost enclosing definition is a type, or that carry a
   `@definition.qualifier` (Go methods), are dropped before counting: a Rust `parse()` inside
   `impl W` does not bind `W::parse`, and a Go bare `run()` does not bind `Widget::run`.
5. **References** (`@reference.name`: JSX element names, PHP trait `use`, C/C++ prototypes) follow
   rule 3 with kind `References` and unresolved confidence 0.5. A C/C++ prototype is matched only
   directly under `translation_unit`, `declaration_list`, `linkage_specification` or a
   `preproc_if*` block (C++ adds template and friend declarations), because tree-sitter-cpp parses
   a local `Foo w(x);` as a function declarator and queries cannot negate a parent.

## Cross-File Resolution Rules

`context/resolve/` acts only on unresolved `Syntax` edges with no candidates on entry, never
rewrites a bound edge, and never crosses a family. Indexes are keyed `(family, name)`: `SymbolIndex`
(definitions), `PathIndex` (module paths) and a namespace index (C# `namespace`, Java `package`, PHP
`namespace` to files). Rules run in order for each unresolved `Calls`/`References` edge in file `F`:

1. **Qualified spelling.** An import-bound first segment decides first: an external module is a gap
   (`Step::Refused`, edge stays unresolved), module files lacking the rest fall through to the
   scope match (`Step::Next`). Then match against node scopes, longest first; a leading segment may
   be dropped only when it names something real (`crate`, `self`, `super`, the empty global segment,
   a module path the conventions map to files). Exactly one hit binds `Import`, unless the owner
   (segment before the name) is carried in the family only by `Implementation` nodes: then hits
   become candidates and the edge records `name:{fam}:{owner}`. An owner with NO node at all (C++
   out-of-line `W::run` with the header outside the graph) is absence of evidence, not a refusal.
   A qualifier naming nothing in the graph stays a gap: `std::io::Error::new()` never binds a
   local `Error::new`.
2. **Receiver not in `self_receivers`.** A receiver that is the local name of an import (a whole
   module, or a named item tried as `item::member` then `member`), of a `using`/`import` alias of a
   type, or of a type from a glob import resolves the module to files and binds exactly one member
   `Import`. Otherwise it is a dynamic receiver: never bound; same-family candidates named like the
   member are recorded (at most 8).
3. **Self receiver.** Definitions ending in `T::m` within the family, in the files that may hold part
   of `T`: same crate for Rust `impl` (the deepest crate-root directory), same namespace for C#
   `partial` classes, anywhere for Ruby reopened classes and C++ out-of-line definitions, only `F`
   for every other dialect (an inherited member stays unbound). One binds `Receiver`; several are
   candidates; PHP trait `use` edges are searched across files; none falls back to same-family
   candidates named `m`, never a bind.
4. **Named or aliased import.** A binding whose `local_name()` equals the called name: resolve
   `binding.path` to files and look up `binding.name` (or the called name). One hit binds `Import`.
   An unresolvable module is external: leave the edge unresolved and **refuse** rules 6 and 7. When
   the module files define nothing of that name, a non-glob binding there whose `local_name()` or
   `exported_as` matches is followed, at most 4 hops (renamed and star re-exports). A member looked
   up through an imported item matches `item::m`; the bare `m` is tried only for a Rust binding
   whose files are the item-named module file or its `mod.rs` form. A relative spec (`crate::`, `self::`, `super::`,
   `./`, `../`, `.x`) that matches no file neither binds nor refuses: only non-relative specs count
   as external, since `use super::*` is unplaceable and ubiquitous in test files.
5. **Package or namespace scope.** Go and Java: same directory; C# and PHP: same declared namespace.
6. **Glob imports.** Look up the name in the files of every resolvable glob binding; one hit binds
   `Import`. An unresolvable glob refuses rule 7, except that a C-family header prototype in an
   included local header lifts the refusal (a system include does not).
7. **Unique name.** Exactly one definition of the name in the family and no refusal fired: bind
   `UniqueName`; two to eight become candidates. Rules 5 to 7 drop member definitions when the
   edge's dialect has `bare_calls_reach_members == false`. A call binds a type's own constructor
   over the type (a type has no body that runs for `new T(...)`), and never a `File` node; a
   `References` edge keeps both.

A refused edge still records one to eight same-family candidates so impact recall survives an
external glob. A lone candidate equal to the edge's own `from` resolves nothing.

Narrowing one candidate in favour of another (a constructor yielding to its type) must key on the
DEFINING FILE: keying on `(family, scope)` let one constructor turn namesake types of other files
into a false `UniqueName` bind, and keying on the file alone broke C++ header/implementation pairs,
so a constructor pairs with a type only when exactly one type in the family has that scope
([mistakes](../mistakes/source-graph-delivery.md)).

## Path Conventions

`resolve/paths/` maps a module spec to candidate files and returns only files of the importer's
family. Rust: anchored `crate::`/`self::`/`super::`; a non-anchored path is internal only when its
first segment names a module file or directory under the citing file's own crate root. From a file
outside every crate-root directory (`tests/`, `benches/`, `examples/` beside `src/`) an
unmatched first segment reads as `crate::<rest>` against the sibling root, because integration
tests import the own crate by package name. ECMAScript: relative specs against the importer's
directory then `x`, `x.ts`, `x.tsx`, `x.js`, `x.jsx`, `x.mjs`, `x.cjs`, `x/index.*`; bare specifiers
are external. Python: dotted `a.b` to `a/b.py` or `a/b/__init__.py` (suffix match; a one-segment
spec matches anywhere). Go: import path suffix matched against directories. Java: `a.b.C` to
`a/b/C.java`, `a.b.*` to the directory. C#: the namespace index. Ruby: `./x` against the importer's
directory, bare specs to `lib/x.rb` else any `x.rb` suffix. PHP: `use A\B\C` to `A/B/C.php`. C and
C++: `#include "x.h"` against the including directory then suffix match; `<x>` is external.

## Recording and Versioning

`resolve_graph`, `resolve_graph_recording` and `resolve_edges` (only a chosen edge set) share one
engine; `resolution_stats` counts (`by_provenance`, `retargeted`, `ambiguous`, `unresolved`). The member filter of rules 5 to 7 makes
an edge depend on its candidate's owner type, which may be declared in another file (a Go `type Widget` in one file, method
`Widget::run` in another); `Site::bare_reachable` records `name:{family}:{owner}` so relink re-resolves when the type changes.
Recording returns each edge's consulted keys: `name:{family}:{name}`, `ns:{family}:{namespace}` and
`pathset:{family}` (touched only when a file is added or removed). One function in
`resolve/record.rs` computes the keys a node answers to, used by `SymbolIndex::build` and
`touched_keys`, so relink selects exactly what a cold build would re-resolve. Any change to rules 1
to 7 or to path conventions bumps `RESOLVER_VERSION` (`context/view/identity.rs`, currently 3); a
walk change bumps that dialect's `extractor_version`.

## Source Graph Evaluation

Offline tools that say what the graph can and cannot see, and how well its edges hold up: the
portfolio census, source windows and the edge-quality evaluator. Views and flags are in
[Context and Source Graph entry points](../entry-points/context-and-source-graph.md).

## Census (`loom map --census`)

`context/census/` (`mod.rs`, `classify.rs`, `subprojects.rs`, `report.rs`) reports coverage across the
current project and any `--root <DIR>` (each must be a git work tree). Enumeration is
`git ls-files -s -z` through `git::runner::run_git_pinned` (untrimmed bytes, mode read per entry); a
symlink (`120000`) or gitlink (`160000`) is `unsupported` and never followed. Classification order is
`excluded` (first path segment in `EXCLUDED_ROOTS`), `vendored` (a path segment among `vendor`,
`third_party`, `third-party`, `node_modules`, `bower_components`, `Pods`, or `linguist-vendored`),
`generated` (`linguist-generated`, or `@generated`, `DO NOT EDIT`, `Code generated`, `autogenerated` in
the first 1024 bytes), `eligible` (the extension has a dialect), else `unsupported` grouped by extension.
`git check-attr --stdin -z` is the one git call outside the runner's checked helpers: stdin is written from a
separate thread while the caller reads stdout, so thousands of long paths neither exceed `ARG_MAX` nor
deadlock a full pipe. Subprojects group by the nearest ancestor manifest (`Cargo.toml`, `package.json`,
`go.mod`, `pyproject.toml`, `pom.xml`, `*.csproj`, `Gemfile`, `composer.json`, `CMakeLists.txt`, …),
falling back to the root. The current project reads its resolved graph; any other root is extracted in
memory through the registry, never cached, and reports `files_parsed`, `bytes_read` and elapsed time.
Percentages use eligible files and bytes as denominators with the excluded counts printed beside them.
`--json` emits `"schema": "loom-census/1"` with `totals` (`eligible`, `excluded`, `vendored`,
`generated`, `unsupported`, each `{files, bytes}`) and per-root `subprojects[]`.

## Source Windows (`loom map --window`)

`context/window.rs::read_window(graph, project_root, id, max_lines)` returns the exact source of a node id
or a site id `path@start-end` (byte offsets). It reads the file, requires `body_hash(bytes) ==
entry.content_hash`, slices whole lines with lossy UTF-8 (a span outside the file or inside a UTF-8
character never panics) and truncates to `max_lines` (`--window-lines`, default 60). `WindowError` has
`UnknownId`, `ChangedSinceSnapshot` and `Unreadable`; `commands/map.rs` maps them to exit 2 (`unknown id`)
and exit 3 (`changed since snapshot`, no bytes printed). Retrieval attaches windows through the same
function (see [Context Retrieval Routing](context-retrieval-state.md)).

## Edge-Quality Evaluation (`loom map --eval-edges`)

`context/eval_edges/` (`mod.rs`, `labels.rs`, `metrics.rs`, `build.rs`, `ids.rs`) scores labelled corpora in
`loom/tests/fixtures/source/labeled/<dialect>/` (one small multi-file project per dialect, all twelve, each
with a `labels.yaml`). Labels are written from the language's semantics before the extractor runs and are
never edited to match output; a label that exposes a defect is fixed at the source in the same stage.

- **Build.** The graph is built in memory from the directory (walk, not git; `EXCLUDED_ROOTS` skipped; `.txt`
  suffix stripped for `syntax_error_files`) and resolved through `view::build_cold`. Nothing is written.
- **Metrics.** A declaration matches on `(path, kind, scope)`; a reference label matches an edge from that path
  whose site covers the line and whose `symbol` equals the label's symbol or ends with it after `::` or `.`. A
  ratio with a zero denominator is a failure. `target_precision` is correct bound targets over bound labelled
  edges; `target_recall` is over labels expecting a target; `external` and `ambiguous` labels are correct when
  the edge stays unbound (ambiguous also needs the labelled candidate set); `false_high_confidence` counts bound
  edges at confidence 0.8 or more with a wrong target; `impact_false_negatives[d]` counts expected ids missing
  from `impact_with(start, depth d)`.
- **Thresholds** (`loom/eval/edge-quality-thresholds.yaml`, published before any holdout project was examined):
  `declaration_recall 1.0`, `false_high_confidence 0`, `target_precision 0.95`, `target_recall 0.5`,
  `impact_false_negatives_depth_1 0`. The cargo test `every_dialect_meets_published_thresholds` enforces them for
  every dialect whose pack is compiled. `Thresholds::check` also enforces per-corpus label floors
  (`MIN_DECLARATIONS 10`, `MIN_TARGETS 6`, `MIN_EXTERNALS 3`, `MIN_AMBIGUOUS 1`, `MIN_IMPACTS 4`,
  `MIN_IMPACTS_DEPTH_1 2`, plus one `syntax_error_files` entry) so deleting failing labels cannot pass.
  Labels naming a declaration path that does not exist fail as `unknown_label_ids`.
- **Exit codes.** 2 when the directory holds no labelled corpus, 1 when any threshold fails or the thresholds file
  is unreadable (indistinguishable today), 0 otherwise; the summary line `<n> corpora, <f> failing` always prints.
- **Fixing a defect** bumps that dialect's `extractor_version` (walk) or `RESOLVER_VERSION` (rules); without the
  bump, cached layers and views keep serving the old output.

The operator protocol (census across consenting projects, holdout labelling, the four-mode agent-task comparison,
metrics stratification, release rule) has no document in the tree. The comparison has not been run.

## Source Graph View

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
(`identity.rs`, currently 4) and the digest. `extractor_digest` is a `sha256` over the sorted
`{dialect}={parser_version}` lines of the registered extractors **plus a `lexical=<LEXICAL_PARSER_VERSION>`
line**: gap and unknown-type files are stamped with the lexical version, and relink copies unchanged
entries on an equal digest, so a lexical bump must change identity or relink would differ from a cold
build. `ViewOrigin` (`Materialized` | `Built`, default `Materialized`, no serde) says whether the view
was read from disk; `loom map` prints it as `"view"`. The view persists no name or adjacency index:
queries scan the parsed graph in one linear pass, and a persisted index would grow the JSON every warm
query parses. The storage-engine choice waits on `--timings` data
([Known Gaps](../concerns/source-graph-known-gaps.md)).

A graph read loads one overlay generation and binds its recorded base revision to both identity and composition. Without an overlay it uses the requested revision; an overlay without a recorded revision is never combined with a base or persisted as an identifiable view.

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
  base). A newly persisted `publish_base` invalidates revision views through `prune_after_publish` and `remove_views`. The `BaseWrite::Replace` refresh branch explicitly removes views after replacement. Denied-write publication held only in memory skips pruning and invalidation.
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
