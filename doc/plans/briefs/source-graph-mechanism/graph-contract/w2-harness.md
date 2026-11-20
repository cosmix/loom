# W2: extraction harness semantics

- **Agent type / tier:** `loom-senior-software-engineer` (opus)
- **Wave:** 2, alongside W4 and W5. W1 has finished: the new types compile and every
  test passes under placeholder behaviour.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 3.2,
  3.3, 3.4, 4, 6.1 and 6.2 in full.

## Goal

Make the shared tree-sitter walk produce the evidence the design specifies:

- real reference sites, merged by dedupe;
- local binding by the rules in design 6.2 (receiver, lexical scope, candidate sets
  in place of last-spelling-wins);
- the new capture protocol (`@import.statement`, `@call.receiver`,
  `@reference.name`) with `QueryHarness::import_bindings` and `self_receivers`;
- disambiguated node ids with `symbol_key`.

## Files you own (write)

- `loom/src/context/extract/treesitter/mod.rs`, `build.rs`, `collect.rs`
- the new `loom/src/context/extract/treesitter/ids.rs` (disambiguation)
- the new `loom/src/context/extract/treesitter/binding.rs` (local binding rules), if
  `build.rs` would pass 400 lines
- the new `loom/src/context/extract/treesitter/tests.rs`, declared from
  `treesitter/mod.rs` with `#[cfg(test)] mod tests;`

Read-only:

- `loom/src/context/source_graph/**` and `loom/src/context/extract/mod.rs`, both W1's
  output. If an API there blocks you, stop and report; do not edit them.
- The language modules, which W3 owns next.
- `loom/tests/graph_contract_contracts.rs`, the frozen contracts. Read them; they are
  your acceptance.

## Steps

1. **`collect.rs`: capture protocol.**
   - `Reference` becomes `{symbol, site: Span, receiver: Option<String>}`. Record the
     span with the existing `span_of` helper in `collect.rs`.
   - `process_match` handles `call.receiver` in the same match as `call.name`.
     - Keep the receiver's text trimmed.
     - A receiver capture without a `call.name` in the match is ignored.
   - Handle `reference.name`, which produces a new `Collected.references`.
   - Handle `import.statement` together with `import.path` in one match: call
     `harness.import_bindings(statement_text, normalized_path, path_site)` and push
     the bindings into a new `Collected.bindings`. A match with only `import.path`
     yields the default single binding.
   - Handle `definition.qualifier` in a definition match (design 6.1). Store the
     qualifier segments on `Definition` (`qualifier: Vec<String>`). In `build.rs` the
     scope becomes `open names + qualifier + name`. Test it with the test-only harness
     from step 6: a definition with a qualifier `W` and name `run` gets scope
     `[W, run]`.
   - Keep the sort order deterministic: sort by `(site.start_byte, symbol)`.
2. **`treesitter/mod.rs`: harness trait.**
   - Add `import_bindings` (with the default implementation from design 6.1) and
     `self_receivers()` to `QueryHarness`. `self_receivers()` defaults to the harness
     dialect's `self_receivers` column (W1's `DialectSpec`), never a hard-coded list.
   - Update the capture-protocol table in the module doc.
   - Rewrite the module doc's provenance bullets to the new classes.
3. **`ids.rs`: node identity.** Add
   `pub(super) fn disambiguate(defs: &mut [(String /*base id*/, &str /*signature*/)]) -> Vec<(String /*id*/, String /*symbol_key*/)>`,
   or an equivalent, implementing design 4 exactly:
   - collapse whitespace runs in the signature;
   - `sha256`, first 8 hex digits;
   - suffix `@{sig8}` for every member of a collision group;
   - `.{n}` for members that still collide, 1-based in source order;
   - `symbol_key` = the base id for suffixed nodes, empty otherwise.

   `build.rs` computes all base ids first (the scope stack walk), then applies
   `disambiguate`, then emits nodes and `Contains` edges with the final ids. The
   parent id on a `Contains` edge must be the parent's *final* id.
4. **`build.rs`: local binding.**
   - `DefinitionScopes.by_spelling` becomes `BTreeMap<String, Vec<String>>`, with
     final ids deduplicated in source order.
   - Keep the node kind and scope per id in a side map, which rule 1 and the lexical
     scope rule need.
   - Implement design 6.2 rules 1–5 in `call_edges` and a new `reference_edges`.
     - Rule 1: when no enclosing `Type`/`Implementation`/`Interface` exists and the
       enclosing function carries a `@definition.qualifier`, `T` is that function's
       scope minus its last segment (C++ `void W::run() { this->m(); }` gives `W`).
     - Rules 3 and 4, for a spelling with no qualifier: when the file's dialect has
       `bare_calls_reach_members == false`, drop every id whose innermost enclosing
       definition is `Type`, `Implementation` or `Interface` before counting.
       Qualified spellings (`W::parse()`, `Self::parse()`) keep members.
   - Each emitted edge carries its site (`vec![reference.site]`) and its `receiver`.
   - Candidate lists are sorted and hold at most `MAX_CANDIDATES` (8). If more exist,
     leave the list empty.
   - Import edges carry their site. Emit `FileExtraction.imports` from
     `Collected.bindings`.
5. **`dedupe`.** Key on `(from, to, kind, provenance, symbol, receiver)`. Merge the
   `sites` of equal edges (sort by `start_byte`, dedupe). Keep the sort canonical so
   that two runs over one byte string serialize identically. The integration test
   `source_graph_fixtures.rs` checks determinism.
6. **Tests** (`treesitter/tests.rs`). Drive them through the Rust extractor
   (`crate::context::extract::rust::RustExtractor`) on inline sources. Cover:
   - two calls to one local function give one edge with two sites on distinct lines,
     `LocalName` at 0.8;
   - `mod a { pub fn helper() {} } mod b { pub fn helper() {} } fn run() { helper(); }`
     gives a `Syntax` edge with both candidates, and no edge at 1.0;
   - lexical scope: `fn outer() { fn helper() {} helper(); } fn helper() {}` binds
     `outer`'s call to `outer::helper` with `LocalName`;
   - `impl W { fn a(&self) { self.b(); } fn b(&self) {} }` gives `Receiver` at 0.85 to
     `W::b`. This test needs W3's receiver capture: write it `#[ignore]`-free by
     driving it through a tiny test-only `QueryHarness` defined in `tests.rs` that
     uses the Rust grammar and a minimal query with `@call.receiver`. W3's
     real-query tests come later;
   - `impl W { fn parse(&self) {} fn load(&self) { parse(); } }` gives a `Syntax` edge
     to `UNRESOLVED_TARGET` with no candidates (the Rust dialect has
     `bare_calls_reach_members == false`); `W::parse()` in the same position keeps the
     member;
   - a function with a `@definition.qualifier` `W` and no enclosing type, whose body
     calls `this.m()` (test-only harness), binds `Receiver` to `W::m`;
   - `obj.b()` with a local `b` stays `Syntax` with `receiver == Some("obj")`;
   - two `impl From<..> for W { fn from(..) }` blocks give distinct ids with suffix
     `@<8 hex>` and equal `symbol_key`, and the `Contains` parents point at the
     suffixed implementation ids;
   - a signature collision (two byte-identical duplicate functions under `#[cfg]`)
     gives the `.1`/`.2` suffixes;
   - `import_bindings` default: a match with only `import.path` gives exactly one
     binding with `glob: false`.

## Traps

- The existing extractor test tables in `rust/tests.rs` and the inline tests in
  `typescript.rs`, `python.rs` and `go.rs` belong to W3, who updates them after you.
  Do not edit them. Expect some of them to fail after your change; name them in your
  report.
- Never emit confidence `1.0` on anything but `Contains`. The frozen contract
  `only_containment_is_certain` fails otherwise.
- `enclosing()` must keep returning the smallest containing definition. Use final
  (disambiguated) ids there.
- A UTF-8 multi-byte identifier must not panic: `node_text` is lossy; keep using it.
- `build.rs` must stay under 400 lines and each function under 50. Split into
  `binding.rs`/`ids.rs`.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::extract::treesitter:: 2>&1 | tail -20`

## Report

Report:

- the files changed;
- the extractor tests that now fail and why (for W3);
- any design ambiguity you had to settle, with the value you chose.
