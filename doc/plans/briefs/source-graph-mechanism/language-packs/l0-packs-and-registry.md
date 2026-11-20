# L0: grammar packs, dependencies, and declaration-level extractors

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 1, alone. L1–L4 extend your modules in wave 2.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 5.1,
  5.3, 5.4 and 6.1.

## Goal

Every dialect row in `DIALECTS` gets a registered, working extractor behind its pack
feature. In this wave each new extractor emits **declarations only**
(functions/methods, types/classes, interfaces, modules/namespaces), so it compiles,
parses, and reports `Full` coverage. L1–L4 then add imports, calls, receivers,
bindings and references in their own files.

## Files you own (write)

- `loom/Cargo.toml`, `loom/Cargo.lock`, via `cargo add` for dependencies. The
  `[features]` block is edited by hand (no cargo command manages it).
- `loom/src/context/extract/mod.rs`: module declarations and `registry()`
- `loom/src/context/extract/dialect.rs`: `GrammarPack::compiled` only
- `loom/src/context/extract/tests.rs`
- new files `loom/src/context/extract/{tsx,javascript,java,csharp,ruby,php,c,cpp}.rs`,
  each with a declaration-level query and an inline `#[cfg(test)] mod tests`
  containing a single declaration test. L1–L4 take ownership of these files in
  wave 2.

Read-only: `loom/src/context/extract/treesitter/**`, `loom/src/context/source_graph/**`,
`loom/src/context/extract/typescript.rs`, and
`loom/tests/language_packs_contracts.rs` (frozen).

## Steps

1. **Dependencies.** From `loom/`, run each of these once:
   - `cargo add tree-sitter-javascript@=0.25.0 --optional`
   - `cargo add tree-sitter-java@=0.23.5 --optional`
   - `cargo add tree-sitter-c-sharp@=0.23.5 --optional`
   - `cargo add tree-sitter-ruby@=0.23.1 --optional`
   - `cargo add tree-sitter-php@=0.24.2 --optional`
   - `cargo add tree-sitter-c@=0.24.2 --optional`
   - `cargo add tree-sitter-cpp@=0.23.4 --optional`

   Read stderr after each. A network failure means stop and report: never
   hand-write a dependency line.
2. **Features.**
   - Edit `[features]` to exactly design 5.4. `default` lists all three packs,
     `source-graph` gains `dep:tree-sitter-javascript`, and two new pack features are
     added.
   - `cargo add --optional` writes per-dependency feature lines into `[features]`:
     remove every one of them (for example `tree-sitter-java = ["dep:tree-sitter-java"]`),
     so that only the pack features name the dependencies.
3. **Pack flags.** In `dialect.rs::GrammarPack::compiled`:
   - `WaveB` becomes `cfg!(feature = "source-graph-wave-b")`;
   - `WaveC` becomes `cfg!(feature = "source-graph-wave-c")`.
4. **Extractor modules.** Each module mirrors `typescript.rs`'s structure
   (`QueryHarness` impl plus `SourceGraphExtractor` impl delegating to `run_query`).
   - Struct names are exact: the stage wiring checks grep the `registry()` calls.
     They are `TsxExtractor`, `JavaScriptExtractor`, `JavaExtractor`,
     `CSharpExtractor`, `RubyExtractor`, `PhpExtractor`, `CExtractor` and
     `CppExtractor`, each with `pub fn new() -> Self`.
   - `dialect()` returns its `DIALECTS` row.
   - `capabilities()` is `{declarations: true, ..false}` (L1–L4 flip the rest).
   - `ExtractorIdentity` has `grammar_version` = the crate version, `dialect` = the
     id, and `extractor_version: 1`. Every new extractor starts at 1 and honours its
     dialect's `self_receivers` and `bare_calls_reach_members` columns (graph-contract
     W1); nothing here hard-codes them.
   - Grammar handles:
     - `tsx`: `tree_sitter_typescript::LANGUAGE_TSX`
     - `javascript`: `tree_sitter_javascript::LANGUAGE`
     - `java`: `tree_sitter_java::LANGUAGE`
     - `csharp`: `tree_sitter_c_sharp::LANGUAGE`
     - `ruby`: `tree_sitter_ruby::LANGUAGE`
     - `php`: `tree_sitter_php::LANGUAGE_PHP`
     - `c`: `tree_sitter_c::LANGUAGE`
     - `cpp`: `tree_sitter_cpp::LANGUAGE`
   - Write each declaration query from that crate's own `queries/tags.scm` (in
     `~/.cargo/registry/src/*/<crate>-<version>/queries/tags.scm`), mapping its
     definition captures onto loom's protocol:
     - `@definition.function` or `@definition.method` becomes `@definition.function`;
     - `@definition.class`, `@definition.struct`, `@definition.enum` or
       `@definition.type` becomes `@definition.type`;
     - `@definition.interface` becomes `@definition.interface`;
     - `@definition.module` or a namespace/package becomes `@definition.module`;
     - `@name` stays `@name`.
     - Check each node type against the crate's `src/node-types.json` (for PHP,
       `php/src/node-types.json`).
   - `@definition.*` always goes on the whole declaration node (`function_definition`,
     `method_declaration`, `class_specifier`, ...), never on a declarator. Spans drive
     scope and call attribution: the C and C++ `tags.scm` files put
     `@definition.function` on the `function_declarator`, and copying that shrinks the
     span to `run()` and turns prototypes into definitions, so calls in a body are
     attributed to the enclosing class or file. A declaration without a body (a
     prototype, an in-class `void run();`) is not a definition.
   - A namespace or package `Module` node's scope is ONE segment holding the dotted
     name: Java `package a.b;` gives `["a.b"]`, C# `namespace A.B` (block or
     file-scoped) gives `["A.B"]`, PHP `namespace A\B;` gives `["A.B"]`. The
     binding-resolver stage's namespace index reads that segment; the inline tests
     assert the shape.
   - Specific requirements:
     - C `function_definition` names come from the `function_declarator`'s
       `declarator: (identifier)`; the `@definition.function` capture sits on the
       `function_definition`.
     - C++ `qualified_identifier` definitions (`void W::run() {}`) name the last
       segment.
     - PHP method names are `(name)`.
     - Ruby `method`, `singleton_method`, `class` and `module` are required.
   - The Rust module declarations in `extract/mod.rs`:
     - `tsx` and `javascript` are `#[cfg(feature = "source-graph")]`;
     - `java`, `csharp`, `ruby` and `php` are `#[cfg(feature = "source-graph-wave-b")]`;
     - `c` and `cpp` are `#[cfg(feature = "source-graph-wave-c")]`.
     - `registry()` pushes each under the same `cfg`, in `DIALECTS` order.
5. **Tests** (`extract/tests.rs`).
   - Replace the stage-1 `.tsx` gap assertion with: every `DIALECTS` row whose pack is
     compiled has exactly one registered extractor.
   - Keep stage 1's runtime-guarded `.java` gap assertion
     (`!GrammarPack::WaveB.compiled()`).
   - Add a C gap assertion guarded the same way on `GrammarPack::WaveC`.
   - Add the test `uncompiled_wave_b_pack_is_a_named_gap` in `extract/tests.rs` under
     `#[cfg(not(feature = "source-graph-wave-b"))]`: under
     `--no-default-features --features source-graph` it asserts design 5.3's detail text
     for a `.java` file (`grammar pack source-graph-wave-b not compiled in (java)`).
     The feature is declared from this stage on, so the `cfg` is valid. Acceptance runs
     the test by that exact name.
   - Every extractor's query compiles on empty input: the existing loop covers this
     once they are registered.
   - Each module's inline test extracts a two-declaration sample and asserts the
     ids and `FileCoverage::Full`.
6. **Degraded builds.** Each is a lint, never a bare `cargo check` (which exits 0 with
   warnings). Run each once:
   - `cargo clippy --manifest-path loom/Cargo.toml --no-default-features --lib -- -D warnings`
   - `cargo clippy --manifest-path loom/Cargo.toml --no-default-features --features source-graph --lib -- -D warnings`
   - `cargo clippy --manifest-path loom/Cargo.toml --no-default-features --features source-graph-wave-b --lib -- -D warnings`
   - `cargo clippy --manifest-path loom/Cargo.toml --no-default-features --features source-graph-wave-c --lib -- -D warnings`

   All four must pass with zero warnings. `unexpected_cfgs` is a warning; every
   `cfg(feature = ...)` must name a declared feature.

## Traps

- Parser ABI:
  - `Parser::set_language` fails for a grammar ABI the core crate (0.27.0) does not
    support. The empty-input registry test surfaces it; never skip it.
  - If one grammar fails to load, stop and report the exact error. Do not pin a
    different version without the main agent.
- `tree-sitter-php` compiles both `php` and `php_only` parsers. Use `LANGUAGE_PHP`,
  which handles `<?php` tags inside HTML.
- The TSX grammar names class names `type_identifier`, as TypeScript does.
- Do not touch `typescript.rs`: L1 owns it in wave 2.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::extract:: 2>&1 | tail -20`

## Report

Report:

- the exact dependency lines `cargo add` wrote;
- the final `[features]` block;
- the four degraded-build `cargo clippy` results;
- any grammar that failed to load.
