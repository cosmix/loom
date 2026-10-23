# Source Graph Resolution

> Binding rules, paths

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
