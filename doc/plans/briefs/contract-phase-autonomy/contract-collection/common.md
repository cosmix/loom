# contract-collection: shared context

Read this first, then your own brief. The plan (`doc/plans/IN_PROGRESS-PLAN-contract-phase-autonomy.md`, section Decisions) is authoritative where this file is silent. All code paths are relative to `loom/` unless they start with `../`.

## Why

A contract writer in liquidmon could not freeze: its contract file `src/device_lock/contract_tests.rs` needed `mod device_lock;` in `src/main.rs` and a new `src/device_lock/mod.rs`, harness allowed only `src/main.rs`, and the freeze's whole-tree `cargo fmt --all -- --check` failed on the unresolved module. Separately, completion runs a Rust contract only by test name (`testrun/adapters/cargo_test.rs` `single_test_command` ignores the file), so an implementer who owns a module root can redirect the contract module with `#[path]` and the frozen file is never compiled.

## Pinned public API (W1 builds it, W2 calls it, the contract tests call it)

`src/verify/contracts/mod.rs` gains `pub mod collection;`.

```rust
// src/verify/contracts/collection/mod.rs
/// A path relative to the source root, '/'-separated, to the file's content;
/// None when the file is absent or unreadable.
pub type FileSource<'a> = &'a dyn Fn(&str) -> Option<String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldModule {
    pub name: String,
    /// The contract file's own module: its declaration may carry #[cfg(test)].
    pub test_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScaffoldEdit {
    /// Add one out-of-line `mod <module.name>;` item to the existing file `path`.
    Declare { path: String, module: ScaffoldModule },
    /// Create exactly one of `paths` (`<dir>/mod.rs` first, then `<dir>.rs`),
    /// holding only the declarations of `modules`.
    CreateRoot { paths: Vec<String>, modules: Vec<ScaffoldModule> },
}

pub mod rust; // chain_problems, derive_scaffold, scaffold_problems

/// Existing files matched by a harness glob that are production source:
/// `context::extract::dialect::dialect_for_path(path).is_some()` and not a
/// test path (`context::rank_source::paths::is_test_path`, made pub(crate)).
/// Globs and files are relative to the same directory; matching uses
/// `super::harness_matches`.
pub fn harness_source_matches(harness: &[String], files: &[String]) -> Vec<String>;

/// Crate-internal: what the freeze needs, computed once and shared by the
/// writer's pre-check (commands/stage/contracts/freeze.rs) and the daemon's
/// re-check (stage_service/contracts.rs).
pub(crate) struct ScaffoldCheck {
    /// Scaffold paths relative to the stage working_dir.
    pub paths: std::collections::BTreeSet<String>,
    pub problems: Vec<String>,
}
pub(crate) fn freeze_scaffold(
    repo: &crate::git::worktree::WorktreeGit,
    worktree_root: &std::path::Path,
    working_dir: &std::path::Path,
    base: &str,
    contracts: &[crate::plan::schema::ContractSpec],
) -> anyhow::Result<ScaffoldCheck>;
```

```rust
// src/verify/contracts/collection/rust/mod.rs
pub fn chain_problems(source: FileSource<'_>, contract_file: &str, test: &str) -> Vec<String>;
pub fn derive_scaffold(base: FileSource<'_>, contracts: &[(&str, &str)]) -> Result<Vec<ScaffoldEdit>, String>;
pub fn scaffold_problems(edits: &[ScaffoldEdit], base: FileSource<'_>, current: FileSource<'_>) -> Vec<String>;
```

All three take paths relative to the source root. Integration (W2) uses the worktree root as source root and converts working_dir-relative contract files (`ContractSpec.file`) by prefixing the working_dir's path relative to the worktree root; it converts scaffold paths back before comparing with `changes::changed_paths` (working_dir-relative).

## Rules (Decisions 2-5 of the plan, restated for implementation)

- **Package**: nearest ancestor directory of the contract file whose `Cargo.toml` parses (crate `toml`, already a dependency) and has a `[package]` table. None: a problem / Err "no Cargo package owns <file>".
- **Targets** (Decision 4): `[lib] path` else `src/lib.rs` when present and `autolib` is not false; `[[bin]] path` entries (an entry without `path` defaults to `src/main.rs` when `name` equals the package name, else `src/bin/<name>.rs` or `src/bin/<name>/main.rs`), plus, unless `autobins = false`, `src/main.rs` and `src/bin/<n>.rs` / `src/bin/<n>/main.rs`; `[[test]] path` entries plus, unless `autotests = false`, `tests/<n>.rs` / `tests/<n>/main.rs`. Auto targets are only those whose file exists in the source. A target is a test target unless its table sets `test = false` or `harness = false`. Discovery is relative to the contract file: only candidate roots that are ancestors-by-module of the file matter, so no directory listing is needed.
- **Module path of a file under a root R**: R's module directory is R's parent directory; a child `a` of a module file `F` resolves to `<dir(F)>/a.rs` or `<dir(F)>/a/mod.rs`, where `dir(F)` is F's parent for `mod.rs`/`lib.rs`/`main.rs`/a root, else `<F without .rs>/`. The contract's test path must start with the file's module path (empty when the contract file is itself a root, e.g. `tests/x.rs`); the rest of the test path lies inside the frozen file and is not checked.
- **Chain rules** (Decision 3): every link is an out-of-line `mod <name>;` item (`(mod_item name: (identifier) !body)` in tree-sitter-rust), declared exactly once in the declaring file, attributes limited to `#[cfg(test)]`, `#[allow(..)]`, `#[expect(..)]` and doc comments. Report `#[path ...]`, `#[cfg_attr(...)]`, any other `#[cfg(...)]`, an inline `mod name { ... }` on the chain, a missing or duplicated declaration, a non-test target, and a test path that does not start with the module path. Each message names the file, the module and the rule ("src/device_lock/mod.rs declares contract_tests with #[path]: a contract module must be declared without #[path]").
- **Scaffold derivation**: walk from the target root to the contract file over base content. A link whose declaring file exists at base and declares the module yields nothing; one whose declaring file exists but lacks the declaration yields `Declare`; one whose declaring file is absent yields `CreateRoot` (paths `<dir>/mod.rs`, `<dir>.rs`) carrying the declaration of the next link. The contract file's own module gets `test_only: true`. Merge across contracts: one `Declare` per (path, module), one `CreateRoot` per paths with the union of modules. Choose the target root as: `src/lib.rs`-style lib target if the file lies under `src/` and a lib target exists, else the bin target `src/main.rs`, else the root whose directory contains the file. If several roots qualify, derive from the first in that order.
- **Strict scaffold diff** (Decision 2): for `Declare`: current minus, per declared module, one line equal after trimming to `mod <name>;`, `pub mod <name>;` or `pub(crate) mod <name>;`, and at most one `#[cfg(test)]` line directly above it, must equal base line for line. For `CreateRoot`: exactly one of `paths` exists in current, neither existed at base, and after the same removals only blank lines remain. Report each extra or missing line with its path.
- **No tree-sitter** (Decision 5): when the `source-graph` feature is off, `chain_problems` returns one problem and `derive_scaffold` returns Err, both "cannot verify Rust contract collection: loom was built without the source-graph feature". Gate the parsing code with `#[cfg(feature = "source-graph")]`.
- **Non-Rust contracts**: integration dispatches on `resolve_adapter(contract, ..).map(|a| a.language()) == Some("rust")` (`cargo-test`, `cargo-nextest`); any other adapter or none has no scaffold and no chain check.

## Limits

Every file at most 400 lines, every function at most 50 (`cargo test --test maintainability`). No new dependency. No change to `testrun/` adapters.
