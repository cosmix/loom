# W3: Rust, TypeScript, Python and Go queries

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 3, after W2. The harness supports `@import.statement`, `@call.receiver`
  and `@reference.name`, candidate sets and sites.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 3.4,
  5.3 (`Capabilities`, `ExtractorIdentity`) and 6.

## Goal

Teach the four existing extractors the new captures, and bring their tests in line
with the new evidence model:

- receivers on member calls;
- import statements with bindings (names, aliases, globs);
- `extractor_version` bumped by one each;
- `capabilities()` telling the truth.

## Files you own (write)

- `loom/src/context/extract/rust.rs`, `rust/tests.rs`
- `loom/src/context/extract/typescript.rs`, `python.rs`, `go.rs` (their inline test
  modules included)
- `loom/tests/integration/source_graph_fixtures.rs`
- `loom/tests/fixtures/source/{rust,typescript,python,go}/**`: add fixtures only.
  Never change an existing fixture's bytes.

Read-only:

- `loom/src/context/extract/treesitter/**` (W2) and `loom/src/context/source_graph/**`
  (W1);
- `loom/tests/graph_contract_contracts.rs` (frozen).

## Steps (per language)

1. **Rust** (`rust.rs`). Anchor every edit on the pattern text, never on a line number.
   - Receivers: in the `field_expression` call patterns, capture
     `value: (_) @call.receiver`.
   - `Self::helper()`: capture it as `@call.receiver` = `Self` plus `@call.name` =
     `helper` with
     `(call_expression function: (scoped_identifier path: (identifier) @call.receiver name: (identifier) @call.name) (#eq? @call.receiver "Self"))`,
     and exclude `Self` paths from the whole-path call pattern, so the harness binds it
     through design 6.2 rule 1.
   - Imports: change the `use_declaration` pattern to
     `(use_declaration argument: (_) @import.path) @import.statement`.
   - Implement `import_bindings` for Rust use trees by parsing the argument text:
     - `a::b::c` gives `{path: "a::b::c", name: Some("c"), alias: None}`;
     - `a::b::c as d` gives `alias: Some("d")`;
     - `a::b::*` gives `{path: "a::b", glob: true}`;
     - a brace group `a::{b, c as d, e::*}` expands to one binding per leaf with the
       prefix applied, recursively for nested braces;
     - `self` inside braces (`a::{self, b}`) binds the module `a` itself (`name: None`);
     - trim whitespace and newlines.
   - `capabilities`: `import_bindings: true`, `receivers: true`.
   - `extractor_version` becomes 3.
2. **TypeScript** (`typescript.rs`).
   - Receivers: capture `object: (_) @call.receiver` on the `member_expression` call
     pattern.
   - Imports: capture `(import_statement source: (string) @import.path) @import.statement`,
     and the same for `export ... from` (re-export).
   - `import_bindings` parses the statement text:
     - `import { a, b as c } from "x"` gives `name: a` and `name: b` with `alias: c`;
     - `import d from "x"` gives `name: Some("default")` with `alias: Some("d")`;
     - `import * as ns from "x"` gives `{name: None, alias: Some("ns")}`;
     - `import "x"` (a side-effect import) gives one binding with no name and
       `alias: Some("")`;
     - `export { a } from "x"` and `export * from "x"` give bindings with `glob: true`
       for `*`.
   - `capabilities`: `import_bindings: true`, `receivers: true`.
   - `extractor_version` becomes 2.
   - Keep `.tsx` out: TSX is a separate dialect built in stage `language-packs`.
3. **Python** (`python.rs`).
   - Receivers: capture `object: (_) @call.receiver` on the `attribute` call.
   - Imports: `import_statement` and `import_from_statement` with `@import.statement`.
   - Bindings:
     - `import a.b` gives `{path: "a.b", name: None, alias: Some("a.b")}`, so the
       receiver text `a.b` in `a.b.f()` matches the binding (resolver rule 2; W1's
       `local_name()` rule);
     - `import a.b as c` gives `alias: c`;
     - `from x import y, z as w` gives names `y` and `z` with `alias: w`;
     - `from x import *` gives `glob: true`;
     - relative `from . import y` and `from ..m import y` keep the dots in `path`.
   - `self_receivers` stays the default, the dialect's `["self", "cls"]`.
   - `extractor_version` becomes 2.
4. **Go** (`go.rs`).
   - Imports: `(import_spec path: (interpreted_string_literal) @import.path) @import.statement`.
   - Bindings: `import "a/b"` gives local name `b` (last segment), and
     `import x "a/b"` gives alias `x`. `import . "a/b"` is a glob, and
     `import _ "a/b"` (a side-effect import) gives `alias: Some("")`.
   - Receivers: capture `operand: (_) @call.receiver` on the selector call. Go
     receivers are named per method, so the Go dialect's `self_receivers` column is
     empty and a Go member call is never receiver-bound at extraction.
   - `capabilities`: `import_bindings: true`, `receivers: false`.
   - `extractor_version` becomes 2.
5. **Tests.** Update every expected edge table (`EXPECTED_EDGES` and the like) to what
   the new code emits, with provenance strings, confidences, sites (line numbers)
   and candidates.
   - Keep the test names.
   - Add one test per language for `import_bindings`: alias, glob, and the brace
     group or named imports.
   - Add one test per language (Go excepted) for a receiver-bound in-file call at
     `receiver` 0.85. For Rust, add a second one: `Self::helper()` inside an `impl`
     binds `Receiver` to `W::helper`.
   - `tests/integration/source_graph_fixtures.rs` keeps its four-element arrays in
     this stage. Update any provenance/confidence assertion there.

## Traps

- The query must compile for every language. `extract/tests.rs` runs each extractor
  on empty input.
- A capture name the harness does not know is ignored. Spell `@call.receiver`,
  `@import.statement` and `@reference.name` exactly.
- Do not relabel expected tables to whatever the code emits without reading it. If an
  emitted edge contradicts design 6.2 (for example a 1.0 call edge, or a member call
  bound by name), that is a harness defect: report it to the main agent rather than
  pinning it.
- Changing an existing assertion line in `rust/tests.rs` raises a test-integrity
  event. That is expected for this stage; the main agent disputes the events in one
  batch. Never delete an assertion to avoid one.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::extract:: 2>&1 | tail -20`

## Report

Report the files changed, each language's binding cases covered, and anything that
contradicted the design.
