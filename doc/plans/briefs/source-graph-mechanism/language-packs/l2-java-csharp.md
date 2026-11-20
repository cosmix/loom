# L2: Java and C #

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside L1, L3 and L4. L0 registered declaration-level `java.rs` and
  `csharp.rs` behind `source-graph-wave-b`.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 3.4,
  4, 5.1, 6.1, 6.2 and 7.1 (the java and csharp rows, for what bindings must carry).

## Goal

Extract Java and C# declarations, packages/namespaces, imports with bindings,
method calls with receivers, and overloads (which the stage-1 id disambiguation keeps
distinct).

## Files you own (write)

- `loom/src/context/extract/java.rs`, `csharp.rs` (with query submodules if needed)
- new fixtures under `loom/tests/fixtures/source/java/` and
  `loom/tests/fixtures/source/csharp/`

Read-only: the harness, `source_graph`, `extract/mod.rs`, `dialect.rs`, and
`loom/tests/language_packs_contracts.rs` (frozen; it pins that `Widget::run` is found in
Java and C# and that Java overloads get distinct ids).

Stage-wide rules: `@definition.*` goes on the whole declaration node, never on a
declarator; every new extractor starts at `extractor_version` 1 and honours its
dialect's `self_receivers` and `bare_calls_reach_members` columns; a namespace or
package `Module` node has ONE scope segment holding the dotted name (Java `["a.b"]`,
C# `["A.B"]`).

## Steps

1. **Java query.** Start from the crate's `queries/tags.scm`.
   - Declarations:
     - `class_declaration`, `interface_declaration`, `enum_declaration`,
       `record_declaration`;
     - `method_declaration`, `constructor_declaration` as `function`;
     - `package_declaration` as `@definition.module`, with its `scoped_identifier`
       text as `@name`, so a class inside becomes `pkg::Class`. Scope comes from span
       containment, and a package declaration does not contain the class. Therefore do
       not use it for scope: capture `package_declaration` only for the namespace
       index (a `Module` node). Its scope is ONE segment holding the dotted name:
       `package a.b;` gives `["a.b"]`, and a test asserts it.
     - Every `@definition.*` capture sits on the whole declaration node
       (`method_declaration`, `class_declaration`, ...), never on a narrower child.
   - Imports: `import_declaration` with `@import.statement`, and the
     `scoped_identifier` (or `asterisk` form) as `@import.path`.
   - Bindings:
     - `import a.b.C;` gives `{path: "a.b.C", name: Some("C")}`;
     - `import a.b.*;` gives `{path: "a.b", glob: true}`;
     - `import static a.b.C.m;` gives `name: Some("m")`.
   - Calls: `method_invocation` with `name: (identifier) @call.name`, plus
     `object: (_) @call.receiver` when present. An `object_creation_expression`
     `type: (type_identifier) @call.name` also counts: `new W()` calls `W`'s
     constructor, which is named after the class.
   - `self_receivers`: the dialect's `["this"]`. `super` is not self. Java has
     `bare_calls_reach_members == true`.
2. **C# query.**
   - Declarations:
     - `class_declaration`, `struct_declaration`, `record_declaration`,
       `interface_declaration`, `enum_declaration`;
     - `method_declaration`, `constructor_declaration`, `local_function_statement`
       as `function`, with `@definition.*` on the whole declaration node as in Java;
     - `namespace_declaration` and `file_scoped_namespace_declaration` as `module`,
       named by their `name` field. A block namespace contains its classes, so they
       scope under it; a file-scoped namespace does not. The `Module` node's scope is
       ONE segment holding the dotted name: `namespace A.B` (block or file-scoped)
       gives `["A.B"]`, and a test asserts it.
   - Imports: `using_directive` with `@import.statement`/`@import.path`. The alias, if
     any, is the `name:` field and the path is the unnamed type child of the
     directive: capture that child as `@import.path`, never any identifier.
   - Bindings:
     - `using A.B;` gives `{path: "A.B", glob: true}`;
     - `using X = A.B.C;` gives `{path: "A.B.C", name: Some("C"), alias: Some("X")}`;
     - `using static A.B.C;` gives `{path: "A.B.C", glob: true}`.
   - Calls: `invocation_expression` with `function: (identifier) @call.name`, or
     `function: (member_access_expression expression: (_) @call.receiver name: (identifier) @call.name)`.
     An `object_creation_expression` also counts. A generic call `M<T>()` or
     `obj.M<T>()` has a `generic_name` as its function or name: capture that node's
     `identifier` as `@call.name`.
   - `self_receivers`: the dialect's `["this"]`. C# has
     `bare_calls_reach_members == true`.
   - `partial class` needs nothing special at extraction: each part's declarations are
     scoped under the class name, and the resolver's receiver rule joins them across
     files.
3. **Metadata.**
   - `capabilities()`: declarations, imports, import_bindings, calls and receivers
     true; references false.
   - `extractor_version` stays 1: a new extractor starts at 1.
4. **Fixtures and tests** (inline test modules).
   - Per language, cover:
     - overloads (`void f(int)`, `void f(String)`) giving two ids with the `@sig8`
       suffix;
     - nested classes;
     - a `this.m()` receiver binding;
     - a dynamic receiver;
     - an import alias (C#) or static import (Java);
     - a glob import;
     - a syntax-error file.
   - Assert exact ids, edges, sites and bindings.

## Traps

- Java's generic invocation (`obj.<T>m()`) names the method in `name`, and
  `normalize_call` already drops `<...>`. C# generic calls are different: see the
  `generic_name` capture in step 2.
- Java annotations such as `@interface` declarations are types. Do not capture
  annotation usages.
- Keep each file under 400 lines.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::extract:: 2>&1 | tail -20`

## Report

Report the files changed, the captures, and known gaps (for example inherited `super`
calls or reflection).
