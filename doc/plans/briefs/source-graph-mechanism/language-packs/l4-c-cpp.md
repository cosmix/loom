# L4: C and C++

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside L1–L3. L0 registered declaration-level `c.rs` and `cpp.rs`
  behind `source-graph-wave-c`.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 3.4,
  4, 5.1 (the `.h` rule), 6.1, 6.2 and 7.1 (the c/cpp row).

## Goal

Extract C and C++ declarations and `#include`s first, and calls as syntax captures.
Preprocessor, template and overload behaviour stay reported gaps, never fabricated
edges.

## Files you own (write)

- `loom/src/context/extract/c.rs`, `cpp.rs` (with query submodules if needed)
- new fixtures under `loom/tests/fixtures/source/c/` and
  `loom/tests/fixtures/source/cpp/`

Read-only: the harness, `source_graph`, `extract/mod.rs`, `dialect.rs`, and
`loom/tests/language_packs_contracts.rs` (frozen; it pins `run` in C and `Widget::run`
in C++).

Stage-wide rules: `@definition.*` goes on the whole declaration node, never on a
declarator; every new extractor starts at `extractor_version` 1 and honours its
dialect's `self_receivers` and `bare_calls_reach_members` columns; a namespace or
package `Module` node (Java, C#, PHP) has ONE scope segment holding the dotted name,
which does not touch the C and C++ dialects.

## Steps

1. **C query.** Start from the crate's `queries/tags.scm`.
   - `@definition.*` always goes on the whole declaration node (`function_definition`,
     `struct_specifier`, `class_specifier`, ...), never on a declarator. The crate's
     `tags.scm` puts `@definition.function` on the `function_declarator`; copying that
     makes prototypes definitions and shrinks the span to `run()`, so calls in a body
     are attributed to the enclosing class or file. Spans drive scope and call
     attribution.
   - Declarations:
     - `function_definition` (the `@definition.function` capture), named by its
       `declarator: (function_declarator declarator: (identifier) @name)`;
     - `struct_specifier`, `union_specifier` and `enum_specifier` with a
       `name: (type_identifier)` body present, as `type`;
     - `type_definition` with `declarator: (type_identifier)`, as `type`.
     - Prototypes (`declaration` with a `function_declarator`) are **not**
       definitions. Do not capture them, or every header prototype becomes a
       duplicate of its definition.
   - Imports: `preproc_include` with `@import.statement`, and
     `path: (string_literal) @import.path` or `path: (system_lib_string) @import.path`.
     `import_bindings` gives one glob binding with the quotes/brackets stripped:
     `#include "x.h"` is a whole-file binding (`glob: true`, path `x.h`), and
     `#include <x>` is external, encoded as a leading `<` kept in `path`
     (`<stdio.h>`), so the resolver treats it as external.
   - Calls: `call_expression` with `function: (identifier) @call.name`, and with
     `function: (field_expression argument: (_) @call.receiver field: (field_identifier) @call.name)`,
     which is a call through a function pointer member.
   - `self_receivers`: the dialect's `[]`. C has `bare_calls_reach_members == false`.
2. **C++ query.** Everything in C (C++ grammar node names), plus:
   - `class_specifier` as `type`;
   - `namespace_definition` as `module` (named by `name`);
   - `function_definition` whose declarator is a `qualified_identifier` or a
     `field_identifier` (in-class methods) as `function`. An in-class `void run();`
     has no body and is not a definition.
     - A `qualified_identifier` definition (`void W::run() {}` outside the class) must
       scope as `W::run`. Capture its `scope:` (`W`) as `@definition.qualifier` and its
       `name:` as `@name`; the stage-1 harness inserts the qualifier into the scope
       (design 6.1), so the scope is `[W, run]`. Inside it, `this->m()` binds through
       graph-contract's rule-1 handling of `@definition.qualifier` (design 6.2): `T` is
       the function's scope minus its last segment. A test asserts both the scope and
       that binding.
   - `template_declaration` wrapping a definition captures the inner definition.
   - Calls:
     - `call_expression` with `function: (qualified_identifier) @call.name`, which
       gives a symbol like `ns::f` or `W::m` once `normalize_call` has run;
     - `field_expression argument: (_) @call.receiver field: (field_identifier) @call.name`
       for `obj.m()` and `this->m()`.
   - `self_receivers`: the dialect's `["this"]`. C++ has
     `bare_calls_reach_members == true`.
3. **Metadata.**
   - `capabilities()`: declarations, imports, import_bindings and calls true;
     receivers true for C++ only; references false.
   - `extractor_version` stays 1: a new extractor starts at 1.
4. **Fixtures and tests.** Cover:
   - a `.h` header parsed by the C++ grammar (`dialect_for_path` maps `.h` to cpp)
     containing C declarations;
   - a function prototype in a header plus its definition in a `.c` file: one node,
     no duplicate;
   - overloads in C++ (distinct ids);
   - a `this->m()` receiver binding;
   - a namespace-qualified call;
   - a macro-generated function, which is a gap and must not be invented;
   - a syntax-error file.

## Traps

- The `.h` rule: a C header processed by the C++ grammar should parse. Include one
  such fixture.
- Include guards and `#ifdef` branches parse as `preproc_*` nodes. Definitions inside
  both branches of an `#ifdef` are two declarations of one name, which the id
  disambiguation keeps distinct. Assert that.
- Keep each file under 400 lines.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::extract:: 2>&1 | tail -20`

## Report

Report:

- the files changed and the captures;
- known gaps (macros, templates, overload resolution).
