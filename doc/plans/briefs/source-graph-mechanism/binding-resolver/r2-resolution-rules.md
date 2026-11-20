# R2: language-aware resolution rules, refusal rules, recording API

- **Agent type / tier:** `loom-senior-software-engineer` (opus)
- **Wave:** 2, after R1. `PathIndex::{module_files, package_files, namespace_files}`
  exist.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 3
  (all), 5.2, 7 (all, including 7.0) and 12 (the dependency-index bullets, for why
  7.0 exists).

## Goal

Rewrite cross-file resolution to the ordered rules of design 7. That means:

- family-keyed indexes;
- import and alias binding through `FileEntry.imports`;
- cross-file receiver binding;
- package scope and glob imports;
- `UniqueName` only when no refusal rule fired;
- candidate sets instead of counting ambiguity;
- the recording API of design 7.0.

## Files you own (write)

- `loom/src/context/resolve.rs`: the module doc, the entry points, and the
  `mod`/`pub use` lines. Keep it an index plus the entry points, and move rules out.
- new `loom/src/context/resolve/rules.rs` (rules 1–7 in order), `resolve/bindings.rs`
  (rules 2 and 4: import-binding lookups), `resolve/receivers.rs` (rule 3),
  `resolve/record.rs` (`EdgeRef`, `EdgeKeys`, `touched_keys`, key formatting)
- `loom/src/context/resolve/symbols.rs`: `SymbolIndex` keyed by `(family, name)`
- `loom/src/context/resolve/tests_resolve.rs` and `tests_qualified.rs`: update
  existing assertions only, after step 0's split
- a second `tests_*.rs` file that step 0 splits out of `tests_resolve.rs`, declared
  from `resolve.rs`
- new `loom/src/context/resolve/tests_rules.rs`, `tests_recording.rs`
- `loom/src/context/resolve/fixtures.rs` (builders)

Read-only:

- `resolve/paths/**` (R1), `resolve/impact.rs` and `resolve/neighbors.rs` (stage
  `map-api-freshness` owns them);
- `source_graph/**`, `extract/**`;
- `loom/tests/binding_resolver_contracts.rs` (frozen). Read it first; its five
  contracts are your acceptance.

## Steps

0. **Split `resolve/tests_resolve.rs` first.** It is 398 lines and holds 8
   `ResolutionStats` literals. The maintainability baseline is an exact-count ratchet,
   so move part of it into a second `tests_*.rs` file BEFORE any edit to it (and
   before `ResolutionStats` changes shape). Binding-resolver runs beside
   language-packs, so the Java, C#, Ruby, PHP, C and TSX extractors do not exist in
   this worktree: tests for those dialects build `FileEntry` values by hand, using
   language-packs' Module-scope shape (one segment holding the dotted name: Java
   `["a.b"]`, C# `["A.B"]`, PHP `["A.B"]`).
1. **`SymbolIndex`** (`symbols.rs`).
   - Key definitions by `(family, name)`, with `family` from
     `dialect_for_path(node.path)`. Keep the `Implementation` filtering rule and the
     documented reason.
   - Add `definitions_in_family(family, name)`,
     `members(family, type_scope: &str, name) -> Vec<String>` (definitions whose scope
     ends in `type_scope::name`), and `definitions_in_files(name, files)`.
   - Every lookup appends its key to a `&mut BTreeSet<String>` the caller passes: this
     is the recording hook. The key is `name:{family}:{last "::" or "." segment of the
     looked-up spelling}`.
   - One shared function in `record.rs` computes the keys a node answers to.
     `SymbolIndex::build` and `touched_keys` both call it, for every node kind the
     index holds (File nodes by file name and stem, `Implementation` nodes included),
     so relink selects every edge a cold build would re-resolve.
   - Keep `node_names(&SourceNode) -> Vec<String>`: `map/views/mod.rs` and
     `verify/goal_backward/reachable.rs` use it.
2. **Rules** (`rules.rs`). For each unresolved `Syntax` edge of kind
   `Calls`/`References`, run design 7 rules 1–7 in order.
   - Resolution never touches an unresolved `Syntax` edge whose `candidates` is
     non-empty on entry. Same-file ambiguity found at extraction (design 6.2) is
     final, records no keys, and is never unbound by relink. Otherwise relink (which
     unbinds and clears candidates) and a cold build would differ whenever a name has
     more than 8 definitions graph-wide.
   - Return an `Outcome` of `Bound(target, provenance)`, `Candidates(ids)` or
     `Unresolved`, plus the consulted keys.
   - A refusal (rule 4's external module, rule 6's unresolved glob) is a flag carried
     through the ordered evaluation. It suppresses rule 7 only, and the refused edge
     still records one to eight same-family candidates named like the call, as rule 2
     does, so impact recall survives an external glob (more than eight leave it plain
     unresolved).
   - Rule 3 (`receivers.rs`): the self receivers come from the from-node's dialect
     (`dialect_for_path(path).self_receivers`). `T` is the innermost proper prefix of
     the from-node's scope that names a `Type`, `Implementation` or `Interface` node
     in the same file. Match candidates on that node's own last name segment, so
     `impl Widget` at top level in another file matches `example::Widget` in an inline
     module.
   - `Imports` edges resolve through `module_files`. Exactly one file binds with
     `Import`; more than one become candidates (at most 8).
   - The resolver keeps the rule that a lone candidate equal to `edge.from` resolves
     nothing.
3. **Applying outcomes.**
   - `Bound` becomes `edge.bind(target, provenance)`.
   - `Candidates` sets `edge.candidates`: sorted, at most `MAX_CANDIDATES`, left empty
     beyond that. Provenance stays `Syntax`, and confidence stays the extraction
     value.
   - `Unresolved` leaves the edge untouched.
4. **Entry points** (`resolve.rs`): `resolve_graph`, `resolve_graph_recording`,
   `resolve_edges` and `resolution_stats`, exactly as design 7.0 lists them.
   - `ResolutionStats` drops `Copy`, derives `Serialize` and `Deserialize`, and gains
     `by_provenance: BTreeMap<String, usize>` (owned keys: `&'static str` keys cannot
     deserialize), counting every non-`Contains` edge after resolution, keyed by
     `as_str()`. `EdgeRef` derives `Clone, PartialEq, Eq, PartialOrd, Ord, Serialize,
     Deserialize`.
   - `ambiguous` is now the count of edges with non-empty `candidates`.
   - `unresolved` counts `to == UNRESOLVED_TARGET`.
   - `retargeted` counts edges whose provenance is `Import`, `Receiver` or
     `UniqueName` (extraction-time `Receiver` binds included), documented on the
     field.
   - `resolve_graph` returns `resolution_stats(graph)` computed after resolving, so
     the two are equal by construction.
   - Bumping `RESOLVER_VERSION` is the rule for any later change to rules 1-7 (the
     constant lands in stage `resolved-view`).
5. **Docs.** Rewrite the module doc in `resolve.rs`: what resolution may claim, each
   rule, the refusal rules, candidate sets and families. Keep the "never a complete
   call graph" framing. Remove all history phrasing.
6. **Tests.**
   - `tests_rules.rs` covers one test per rule and per refusal, across dialects:
     - TS alias;
     - Python `import a.b as c` then `c.f()`;
     - Go same-package call;
     - Java same-package class method;
     - C# `using` then a call into a namespace type;
     - Ruby reopened class receiver across files;
     - PHP `use ... as` alias;
     - C `#include "x.h"` then a call to a function defined in `x.c`, where the
       definition lives in the `.c` beside the header. The header prototype is not a
       node, so the glob import of `x.h` finds nothing in `x.h`, and rule 7 binds
       `UniqueName` only if no system include (`<...>`) refused it. Assert
       `UniqueName` for a file with no system includes, and unresolved for a file with
       one;
     - dynamic receiver candidates;
     - candidate cap at 9 definitions (empty candidates);
     - cross-family refusal.
   - `tests_recording.rs`:
     - the keys recorded for an import-bound edge include its `pathset:` and
       `name:` keys;
     - `touched_keys` of an entry lists its definitions' names and namespaces;
     - `resolve_edges` over a subset leaves other edges untouched;
     - `resolution_stats` agrees with `resolve_graph`.

     Three tests carry these exact names, which acceptance runs with `--exact`:
     - `index_buckets_map_to_touched_keys`: every `SymbolIndex` bucket maps to a key
       `touched_keys` emits for the owning entry;
     - `resolve_edges_matches_full_resolution_on_selected_edges`: unbind a chosen edge
       set, run `resolve_edges`, and compare with a full `resolve_graph`;
     - `stats_equal_resolution_stats_after_resolving`.

     A wiring regex cannot prove these APIs: a pattern that matches only a definition
     line is excluded (`verify/goal_backward/wiring_v2.rs::file_match`).
   - A same-file ambiguous edge (candidates non-empty on entry) is untouched by
     `resolve_graph` and records no keys.
   - Update assertions in `tests_resolve.rs` and `tests_qualified.rs` only where a
     rule changed the outcome, and list each change in your report. One is known: the
     `tests_resolve.rs` test that imports `./language` from `src/app.ts` and expects
     `stats.ambiguous == 1` between `src/a/language.ts` and `src/b/language.ts`
     matches nothing under design 7.1's relative resolution. R1 left it failing; you
     update it (a test-integrity event the stage disputes once).

## Traps

- Never bind across families.
- Never bind a dynamic receiver (rule 2) by name.
- Never promote an edge to `LocalName`/`Structural`/`Compiler`: `bind` refuses them,
  so do not work around it.
- Rust `use crate::x::*` then a call to a name in `x.rs`: rule 6 binds it with
  `Import`. `use external::*` (the module resolves to no file) refuses rule 7. The
  frozen contract `external_glob_import_refuses_unique_name` pins the second case.
- Rule order matters: a qualified spelling (rule 1) wins over bindings, and bindings
  win over package scope.
- Every function under 50 lines; every file under 400.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::resolve:: 2>&1 | tail -15`

## Report

Report:

- the files changed;
- each changed existing assertion and why;
- the resolution stats on `loom/tests/fixtures/source/**` before and after, from a
  small test that prints them.
