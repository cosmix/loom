# L1: TSX and JavaScript, with JSX

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside L2–L4. L0 registered declaration-level `tsx.rs` and
  `javascript.rs`.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 3.4,
  5.1, 6.1 and 6.2.

## Goal

TSX and JavaScript extract what TypeScript does and more. The TypeScript side covers
declarations, imports with bindings, calls with receivers, and re-exports. The
additions are:

- JSX element usage as `References` edges (`<Button/>` references `Button`);
- CommonJS `require()` as an import.

## Files you own (write)

- `loom/src/context/extract/tsx.rs`, `javascript.rs`
- `loom/src/context/extract/typescript.rs`: only to expose its query text for reuse
  (step 1)
- new fixtures under `loom/tests/fixtures/source/tsx/` and
  `loom/tests/fixtures/source/javascript/`

Read-only: the harness, `source_graph`, `extract/mod.rs`, `dialect.rs`, and
`loom/tests/language_packs_contracts.rs` (frozen). Read the frozen contract file first:
it pins `.tsx` symbol ids, a JSX `References` edge from a `.jsx` file, and `require`
as an import binding.

Stage-wide rules: `@definition.*` goes on the whole declaration node, never on a
declarator; every new extractor starts at `extractor_version` 1 and honours its
dialect's `self_receivers` and `bare_calls_reach_members` columns; a namespace or
package `Module` node (Java, C#, PHP) has ONE scope segment holding the dotted name,
which does not touch the ecmascript dialects.

## Steps

1. **Share the TypeScript patterns without duplicating them.**
   - In `typescript.rs`, turn the query string into
     `macro_rules! typescript_query { () => { "..." } }`, exported with
     `pub(super) use typescript_query;`.
   - `QueryHarness::query_source` returns `typescript_query!()`.
   - `tsx.rs` returns `concat!(typescript_query!(), JSX_PATTERNS!())`, with the JSX
     patterns in a second macro in `tsx.rs`.
   - Confirm the TypeScript query digest is unchanged: the TS `extractor_version` and
     identity must not move.
   - If a pattern in the TS query uses a node type the TSX grammar lacks, the query
     fails to compile. The L0 empty-input test catches that; fix it by moving that
     pattern out of the shared macro.
2. **JSX patterns** (TSX and JavaScript):
   - `(jsx_self_closing_element name: (identifier) @reference.name)`
   - `(jsx_opening_element name: (identifier) @reference.name)`
   - `(jsx_self_closing_element name: (member_expression property: (property_identifier) @reference.name))`

   Emit only capitalized names (components). Lowercase intrinsic tags (`div`) are not
   references: filter with `(#match? @reference.name "^[A-Z]")`.
3. **JavaScript query.** Write a full JavaScript query:
   - declarations: `function_declaration`, `generator_function_declaration`,
     `class_declaration`, `method_definition`, a `variable_declarator` with an
     arrow/function value, and `export`ed `const`;
   - imports: `import_statement` with `@import.statement`/`@import.path`;
   - `export ... from` re-exports;
   - CommonJS, as two disjoint patterns:
     - `(variable_declarator value: (call_expression function: (identifier) @_req (#eq? @_req "require") arguments: (arguments (string) @import.path))) @import.statement`
       (the declarator is the statement, so the binding text includes the left side);
     - `(expression_statement (call_expression function: (identifier) @_req (#eq? @_req "require") arguments: (arguments (string) @import.path)) @import.statement)`.

     `import_bindings` handles `const x = require("y")` (alias `x`) and
     `const { a, b: c } = require("y")` (names). The binding site is the
     `@import.path` span. A statement-level `require("y")` is a side-effect import
     (alias `Some("")`).
   - calls: `call_expression` with `function: (identifier) @call.name`, carrying
     `(#not-eq? @call.name "require")` so `require` is never a `Calls` edge, and
     `function: (member_expression object: (_) @call.receiver property: (property_identifier) @call.name)`.
4. **Bindings.** `import_bindings` for both dialects reuses TypeScript's statement
   parser. Move it to a shared private helper in `typescript.rs` as
   `pub(super) fn ecmascript_import_bindings`, then add the `require` forms.
5. **Metadata.**
   - `capabilities()` for both dialects:
     `{declarations, imports, import_bindings, calls, receivers, references}` all
     true.
   - `extractor_version` stays 1: a new extractor starts at 1. It honours its
     dialect's `self_receivers` (`["this"]`) and `bare_calls_reach_members` (false)
     columns from graph-contract W1.
   - `@definition.*` goes on the whole declaration node (`function_declaration`,
     `method_definition`, `class_declaration`, a `variable_declarator` holding an
     arrow function, ...), never on its `name` child or any node narrower than the
     body. Spans drive scope and call attribution.
6. **Fixtures and tests** (inline test modules).
   - Fixtures cover, per dialect:
     - nested scopes (a class method calling a module function);
     - aliases (`import { a as b }`);
     - re-exports (`export * from`);
     - duplicate same-file names (two `run` methods in two classes);
     - `this.m()` receiver binding;
     - a dynamic receiver `obj.m()`;
     - JSX usage;
     - a syntax-error file;
     - `require` (JavaScript).
   - Assert exact node ids, edge provenance, confidence, sites (lines), receivers,
     candidates and bindings.

## Traps

- `.jsx` belongs to the `javascript` dialect; `.tsx` alone uses the TSX grammar.
- `private_property_identifier` (`#x`) members are not captured. That is a documented
  gap: note it in your report and do not invent a node for them.
- Keep each file under 400 lines. Put a long query in its own `tsx/query.rs` or
  `javascript/query.rs` submodule if needed.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::extract:: 2>&1 | tail -20`

## Report

Report the files changed, the captures added, and known gaps.
