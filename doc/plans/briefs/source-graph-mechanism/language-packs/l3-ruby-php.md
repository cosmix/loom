# L3: Ruby and PHP

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside L1, L2 and L4. L0 registered declaration-level `ruby.rs` and
  `php.rs` behind `source-graph-wave-b`.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 3.4,
  5.1, 6.1, 6.2 and 7.1 (the ruby and php rows).

## Goal

Extract Ruby and PHP declarations, requires/uses with bindings, and calls with
receivers.

## Files you own (write)

- `loom/src/context/extract/ruby.rs`, `php.rs` (with query submodules if needed)
- new fixtures under `loom/tests/fixtures/source/ruby/` and
  `loom/tests/fixtures/source/php/`

Read-only: the harness, `source_graph`, `extract/mod.rs`, `dialect.rs`, and
`loom/tests/language_packs_contracts.rs` (frozen; it pins `Widget::run` in both).

## Steps

1. **Ruby query.** Start from the crate's `queries/tags.scm`.
   - Declarations: `class` and `module` with their `name: (constant)`. For a
     `scope_resolution` name (`class A::B`), capture `scope:` as
     `@definition.qualifier` and `name:` as `@name`. `method` and `singleton_method`
     count as `function`.
   - Imports: `(call method: (identifier) @_m (#match? @_m "^(require|require_relative|load)$") arguments: (argument_list (string (string_content) @import.path))) @import.statement`.
     The spec is lossless (design 3.4): `require_relative 'x'` gives path `./x`
     (a spec already starting `./` or `../` is kept), and `require`/`load` keep the
     spec verbatim, so capture `@_m` beside `@import.path`.
     `import_bindings` returns one glob binding: a require makes everything the file
     defines visible.
   - Calls:
     - `(call method: (identifier) @call.name receiver: (_)? @call.receiver)`, where
       a receiver that is absent means a plain call;
     - a bare `identifier` statement is ambiguous with a local variable in Ruby, so do
       not capture it as a call.
     - Exclude `require`/`require_relative`/`load`/`attr_accessor`/`attr_reader`/`attr_writer`/`include`/`extend`
       from call captures with a `#not-match?` predicate.
   - `self_receivers`: the dialect's `["self"]`. Ruby has
     `bare_calls_reach_members == true`.
   - Every `@definition.*` capture sits on the whole declaration node (`class`,
     `module`, `method`, `singleton_method`), never on a narrower child.
2. **PHP query** (`LANGUAGE_PHP`). Node types live at `php/src/node-types.json` in the
   `tree-sitter-php` crate, not `src/node-types.json`.
   - Declarations:
     - `class_declaration`, `interface_declaration`, `trait_declaration`,
       `enum_declaration`;
     - `function_definition` and `method_declaration` as `function`;
     - `namespace_definition` as `module`, named by its `name`. A braced namespace
       contains its classes; a statement-form namespace (`namespace A;`) does not. The
       `Module` node's scope is ONE segment holding the dotted name, with `\` written
       as `.`: `namespace A\B;` gives `["A.B"]`, and a test asserts it.
     - Every `@definition.*` capture sits on the whole declaration node, never on a
       narrower child.
   - Imports: `namespace_use_declaration` (`use A\B\C;`, `use A\B\C as D;`,
     `use function A\f;`, and group use `use A\{B, C as D};`) with
     `@import.statement`. `import_bindings` expands groups and reads `as` aliases.
     The path uses `\` separators as written: the resolver maps `\` to `/`.
   - Also imports: `require`/`require_once`/`include`/`include_once` expressions with
     a string argument, as one glob binding each.
   - Calls:
     - `function_call_expression` with `function: (name) @call.name`;
     - `member_call_expression` with `object: (_) @call.receiver name: (name) @call.name`;
     - `scoped_call_expression` with `scope: (_) @call.receiver name: (name) @call.name`,
       which covers `self::m()`, `static::m()` and `A::m()`. `A::m()` emits
       `@call.name` = `m` and `@call.receiver` = `A`, with no `A::m` symbol. The
       resolver treats `A` under rule 2: it binds through `use X\A;` when `A` is an
       import's local name, and otherwise yields candidates only.
   - `self_receivers`: the dialect's `["$this", "self", "static"]`. PHP has
     `bare_calls_reach_members == false`.
3. **Metadata.**
   - `capabilities()`: declarations, imports, import_bindings, calls and receivers
     true; references false.
   - `extractor_version` stays 1: a new extractor starts at 1.
4. **Fixtures and tests.** Per language, cover:
   - a reopened class (Ruby) or a trait (PHP);
   - nested modules or namespaces;
   - a self-receiver binding;
   - a dynamic receiver;
   - a PHP `A::m()` call (receiver `A`, symbol `m`);
   - require/use aliases;
   - a group use (PHP);
   - a syntax-error file.

   Assert exact ids, edges, sites and bindings.

## Traps

- A Ruby `(call ...)` with no receiver and no arguments may parse as `identifier`:
  such calls are not captured. That is a documented gap.
- PHP files embed HTML. `LANGUAGE_PHP` handles `<?php` tags; a fixture must start with
  `<?php`.
- Keep each file under 400 lines.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::extract:: 2>&1 | tail -20`

## Report

Report the files changed, the captures, and known gaps (`method_missing`,
`send`/`public_send`, variable functions `$f()`).
