# R1: module path conventions, namespace index, package scope

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 1, alone. R2 builds the resolution rules on your API in wave 2.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 5.1,
  5.2, 7 (its rules, for what your API must answer) and 7.1 in full.

## Goal

Replace the single `MODULE_SUFFIXES` table with per-dialect module resolution, and add
the two indexes the resolver rules need:

- a namespace index (C#, PHP, Java packages);
- package-scope file sets (Go directory, Java package directory).

## Files you own (write)

- `loom/src/context/resolve/paths.rs` exists (282 lines). Move it with `git mv` to
  `resolve/paths/mod.rs` so the two never coexist, plus submodules
  `paths/{ecmascript,python,rust,go,jvm,csharp,ruby,php,c}.rs` as needed, each well
  under 400 lines. The `mod paths;` line in `resolve.rs` is yours.
- new `loom/src/context/resolve/tests_paths.rs`, declared from `resolve.rs` with
  `#[cfg(test)] #[path = "resolve/tests_paths.rs"] mod tests_paths;`. That
  declaration and the `mod paths;` line are your only edits to `resolve.rs`.

Read-only:

- `loom/src/context/resolve.rs` (except that line) and `resolve/symbols.rs`, both R2's;
- `loom/src/context/source_graph/**` and `loom/src/context/extract/dialect.rs`;
- `loom/tests/binding_resolver_contracts.rs` (frozen).

## API to provide (exact; R2 codes against it)

```rust
pub(super) struct PathIndex { /* existing fields + */ namespaces: BTreeMap<(String /*family*/, String /*namespace*/), Vec<String /*file id*/>>, dirs: BTreeMap<String /*dir*/, Vec<String /*file id*/>> }
impl PathIndex {
    pub(super) fn build(graph: &ResolvedGraph) -> Self;
    /// File node ids a module spec written in `from` names, only files whose dialect is in the importer's family
    /// (a verbatim `./styles.css` probe never matches); empty = not in the graph (external).
    pub(super) fn module_files(&self, spec: &str, from: &str, dialect: &DialectSpec, keys: &mut BTreeSet<String>) -> Vec<String>;
    /// File node ids sharing `from`'s package scope (Go: same directory, `.go` only; Java: same directory, `.java` only;
    /// C#/PHP: files declaring the same namespace as `from`); excludes `from` itself.
    pub(super) fn package_files(&self, from: &str, dialect: &DialectSpec, keys: &mut BTreeSet<String>) -> Vec<String>;
    /// File node ids declaring `namespace` in `family`.
    pub(super) fn namespace_files(&self, family: &str, namespace: &str, keys: &mut BTreeSet<String>) -> Vec<String>;
}
pub(super) fn import_candidates(symbol: &str, from: &str, paths: &PathIndex, keys: &mut BTreeSet<String>) -> Vec<String>; // keep, now delegating by the dialect of `from`
```

- Every path-convention function takes `keys: &mut BTreeSet<String>` (R2's recording
  hook, design 7.0). Each records `ns:{family}:{namespace}` whenever it consults the
  namespace index, and `pathset:{family}` for every module-path or package-scope
  lookup.
- The dialect of `from` is `dialect_for_path(from)`. A `from` with no dialect uses
  today's behaviour.
- Namespaces come from `Module` nodes. A `Module` node's scope is ONE segment holding
  the dotted name (Java `["a.b"]`, C# `["A.B"]`, PHP `["A.B"]`), and that segment is
  the namespace string. A PHP spec's `\` becomes `.` before the namespace lookup.
- The Java, C#, Ruby, PHP, C and TSX extractors do not exist in this worktree:
  binding-resolver runs beside language-packs. Tests for those dialects build
  `FileEntry` values by hand, using that Module-scope shape.

## Conventions

Implement design 7.1 exactly, one submodule per dialect family:

- rust keeps today's anchored `crate::`/`self::`/`super::` logic and `/mod.rs`,
  `/lib.rs`, `/main.rs`. A non-anchored path (`x::y`, none of `crate::`, `self::`,
  `super::`) is internal only when its first segment names a module file
  (`<seg>.rs`, `<seg>/mod.rs`) or directory under a crate root (a directory holding
  `lib.rs` or `main.rs`) or under the citing module's directory; otherwise
  `module_files` returns empty and the import is external. Today's
  `strip_foreign_root` plus suffix truncation turns `use serde::de::*` into `de` and
  matches any local `de.rs`: replace it;
- `ecmascript` resolves relative paths against the directory of `from` with the listed
  extension and index probes, and treats bare specifiers as external (empty);
- python handles dotted and relative (`.`, `..`) forms;
- go uses import-path suffix to directory;
- java uses a dotted path to `a/b/C.java`, with glob to directory files;
- csharp uses the namespace index;
- ruby: a spec starting `./` or `../` (language-packs writes `require_relative 'x'`
  as `./x`, design 3.4) resolves against the importing file's directory only; a bare
  spec (`require`, `load`) resolves to `lib/<spec>.rb` when it exists, else to every
  `<spec>.rb` suffix match;
- php maps `\` to `/`, then `A/B/C.php` suffix, and handles require/include paths;
- c/cpp handle `"x.h"` relative first, then suffix, with `<x>` external.

Every probe compares against file node ids in the graph. Never touch the filesystem.

## Tests (`tests_paths.rs`)

- Build graphs from hand-made `FileEntry`s: file nodes plus Module nodes where
  needed. `resolve/fixtures.rs` has builders; read them first.
- One test per dialect row in design 7.1, positive and negative (external):
  - TS `./util` resolves to `src/util.ts`, and `react` to nothing;
  - TSX `./Button` resolves to `src/Button.tsx`;
  - JS `../lib` resolves to `lib/index.js`;
  - Python `.models` from `app/views.py` resolves to `app/models.py`;
  - Go `example.com/m/pkg/util` resolves to every `.go` file in `pkg/util/`;
  - Java `a.b.C` resolves to `src/main/java/a/b/C.java`, and `a.b.*` to all files in
    that directory;
  - C# `A.B` resolves to the files declaring namespace `A.B`;
  - Ruby `require_relative "../lib/x"`;
  - PHP `App\Models\User` resolves to `src/App/Models/User.php`;
  - C `"util.h"` from `src/main.c` resolves to `src/util.h`, and `<stdio.h>` to
    nothing.
- `package_files`: Go same-directory files; Java same-directory files; C# same
  namespace across two directories.
- Rust negative test: `use serde::de::*` with `src/x/de.rs` present makes
  `module_files` return empty.
- Keys: a namespace lookup records `ns:{family}:{namespace}`, and a module-path or
  package-scope lookup records `pathset:{family}`.
- A verbatim `./styles.css` probe from a `.ts` file matches nothing, even with a
  `styles.css` file node in the graph (family filter).
- Existing path tests in `tests_resolve.rs` and `tests_qualified.rs` must stay green
  unchanged, with one exception: the `tests_resolve.rs` test that imports `./language`
  from `src/app.ts` and expects `stats.ambiguous == 1` between `src/a/language.ts` and
  `src/b/language.ts`. Design 7.1's relative resolution matches neither, so that
  assertion fails after your change. Leave it failing and report it; R2 updates it.

## Traps

- Suffix matching must be on path components (`util.ts` must not match
  `myutil.ts`).
- Keep candidate lists sorted and deduplicated. R2's "exactly one" rules depend on it.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::resolve:: 2>&1 | tail -15`

## Report

Report the files changed, the final API signatures, and any convention you could not
express.
