# contract-collection: shared context

Read this first, then your own brief. The plan (`doc/plans/IN_PROGRESS-PLAN-contract-phase-autonomy.md`, section Decisions 1-7) is authoritative where this file is silent. All code paths are relative to `loom/` unless they start with `../`.

## Why

A contract writer in liquidmon could not freeze: its contract file `src/device_lock/contract_tests.rs` needed `mod device_lock;` in `src/main.rs` and a new `src/device_lock/mod.rs`, harness allowed only `src/main.rs`, and the freeze's format gate (the stage's format-check acceptance entries, run at freeze by `commands/stage/contracts/freeze.rs` through `verify::contracts::format_gate`) ran `cargo fmt --all -- --check`, which failed on the unresolved module. Separately, completion runs a Rust contract only by test name (`testrun/adapters/cargo_test.rs` `single_test_command` runs `cargo test <name> -- --exact` across every target; `cargo_nextest.rs` runs `test(=<name>)`), so an implementer who owns a module root can redirect the contract module with `#[path]`, or remove it with an inner `#![cfg(not(test))]` and add a same-named shadow test in another target, and the frozen file is never compiled.

## Pinned public API (W1 builds it, W2 calls it, the contract tests call it)

`src/verify/contracts/mod.rs` gains `pub mod collection;` (and a line for it in the module doc at L1-13).

```rust
// src/verify/contracts/collection/mod.rs
/// A path relative to the source root, '/'-separated, to the file's content;
/// None when the file is absent or unreadable.
pub type FileSource<'a> = &'a dyn Fn(&str) -> Option<String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldModule {
    pub name: String,
    /// The contract file's own module: the signal suggests #[cfg(test)] on
    /// its declaration. The diff accepts #[cfg(test)] above any derived one.
    pub test_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScaffoldEdit {
    /// Add one out-of-line `mod <module.name>;` item to the existing file `path`.
    Declare { path: String, module: ScaffoldModule },
    /// Create exactly one of `paths` (`<dir>/mod.rs` first, then `<dir>.rs`
    /// unless that path is itself an auto target root), holding only the
    /// declarations of `modules`.
    CreateRoot { paths: Vec<String>, modules: Vec<ScaffoldModule> },
}

pub mod rust; // chain_problems, run_dir_problems, derive_scaffold, scaffold_problems

/// Existing files matched by a harness glob that are production source:
/// `context::extract::dialect::dialect_for_path(Path::new(path)).is_some()`
/// and not a test path (`context::rank_source::is_test_path`, re-exported
/// pub(crate) from the private `paths` module). Globs and files are relative
/// to the same directory; matching uses `super::harness_matches`.
pub fn harness_source_matches(harness: &[String], files: &[String]) -> Vec<String>;

/// Crate-internal: the stage's merged scaffold derived over the stage base,
/// paths relative to `working_dir`. The one base reader every caller outside
/// `crate::verify` uses (the contract signal, stage 2's adjudication prompt
/// and verdict validation): `changes::git` is `pub(in crate::verify)`, so
/// they cannot read base blobs themselves.
pub(crate) fn stage_scaffold(
    repo: &crate::git::worktree::WorktreeGit,
    worktree_root: &std::path::Path,
    working_dir: &std::path::Path,
    base: &str,
    contracts: &[crate::plan::schema::ContractSpec],
) -> Result<Vec<ScaffoldEdit>, String>;

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
/// Empty when the nearest `Cargo.toml` at or above `run_dir` ("" or "." is the
/// source root) is the manifest of the package that owns `contract_file`;
/// otherwise one problem naming both directories.
pub fn run_dir_problems(source: FileSource<'_>, contract_file: &str, run_dir: &str) -> Vec<String>;
```

All four take paths relative to the source root. Integration (W2) uses the worktree root as source root and converts working_dir-relative contract files (`ContractSpec.file`) by prefixing the working_dir's path relative to the worktree root; it converts scaffold paths back before comparing with `changes::changed_paths` (working_dir-relative). `chain_problems` does not require `contract_file` to be present in the source (the chain ends at its declaration).

## Rules (Decisions 2-6 of the plan, restated for implementation)

- **Package**: nearest ancestor directory of the contract file whose `Cargo.toml` parses (`toml::Table`, `toml = "1"` is already a dependency) and has a `[package]` table. None: a problem / Err "no Cargo package owns <file>". A workspace-only manifest (`[workspace]`, no `[package]`) is skipped and the walk continues upward.
- **Targets** (Decision 4): `[lib] path` else `src/lib.rs` when present and `autolib` is not false; `[[bin]] path` entries (an entry without `path` defaults to `src/main.rs` when `name` equals the package name, else `src/bin/<name>.rs` or `src/bin/<name>/main.rs`), plus, unless `autobins = false`, `src/main.rs` and `src/bin/<n>.rs` / `src/bin/<n>/main.rs`; `[[test]] path` entries plus, unless `autotests = false`, `tests/<n>.rs` / `tests/<n>/main.rs`. Edition: a manifest without `edition` is 2015; in 2015 auto-discovery of a target kind is off when the manifest declares at least one target of that kind. Auto targets are only those whose file exists in the source, with one exception: a contract file that is itself an auto or `[[test]]` root counts as present though absent (it is new at base) and derives no edits; with `autotests = false` and no `[[test]]` entry naming it, it is no root and derivation errs. A target is a libtest test target unless its table sets `test = false`, `harness = false`, or a non-empty `required-features` (cargo skips that target silently when the features are off). An explicit `[[bin]]`/`[[test]]` entry whose path (given, or defaulted from its name) equals an auto root's path replaces that auto root: its `test`, `harness` and `required-features` apply. Discovery is relative to the contract file: only candidate roots that are ancestors-by-module of the file matter, so no directory listing is needed.
- **Module path of a file under a root R**: R's module directory is R's parent directory; a child `a` of a module file `F` resolves to `<dir(F)>/a.rs` or `<dir(F)>/a/mod.rs`, where `dir(F)` is F's parent for `mod.rs`/`lib.rs`/`main.rs`/a root, else `<F without .rs>/`. The contract's test path must start with the file's module path (empty when the contract file is itself a root, e.g. `tests/x.rs`); the rest of the test path lies inside the frozen file and is not checked.
- **Root choice** (Decision 5): candidates are the libtest test targets whose module path for the file prefixes `test`. `chain_problems` passes when any candidate yields a clean chain; otherwise it returns the problems of the candidate whose chain got furthest, or "no test target of package <p> compiles <file> as <test>". `derive_scaffold` derives from the candidate needing the fewest base edits; ties: lib, then `src/main.rs`, then path order. Derivation considers test targets only, so a `test = false` lib is never chosen.
- **Chain rules** (Decision 3): every link is an out-of-line `mod <name>;` item (`(mod_item name: (identifier) !body)` in tree-sitter-rust), top-level in its declaring file, declared exactly once there, outer attributes limited to `#[cfg(test)]`, `#[allow(..)]`, `#[expect(..)]` and doc comments; the backward walk over the item's preceding siblings skips every `line_comment`/`block_comment` and stops at the first non-attribute, non-comment node. Every chain file except the frozen contract file, the target root included, has no inner attribute (`inner_attribute_item`, `#![...]`) of kind `cfg` other than exactly `#![cfg(test)]`, and none of kind `cfg_attr`. A declaring file whose parse tree `has_error()` fails the check. Report `#[path ...]`, `#[cfg_attr(...)]`, any other `#[cfg(...)]`, any other attribute, an inline `mod name { ... }` on the chain, a missing or duplicated declaration, an inner `#![cfg...]`/`#![cfg_attr...]`, a parse error, a non-test target, and a test path that does not start with the module path. Each message names the file, the module and the rule, quoting the attribute as written: "src/device_lock/mod.rs declares contract_tests with #[path]: a contract module must be declared without #[path]"; "src/device_lock/mod.rs starts with #![cfg(not(test))]: a module on a contract's chain must not be cfg'd out".
- **Scaffold derivation**: walk from the chosen root to the contract file over base content. A link whose declaring file exists at base and declares the module yields nothing; one whose declaring file exists but lacks the declaration yields `Declare`; one whose declaring file is absent yields `CreateRoot` (paths `<dir>/mod.rs`, then `<dir>.rs` unless `<dir>.rs` is an auto target root such as `tests/<n>.rs` or `src/bin/<n>.rs`) carrying the declaration of the next link. The contract file's own module gets `test_only: true`. Merge across contracts: one `Declare` per (path, module), one `CreateRoot` per paths with the union of modules.
- **Strict scaffold diff** (Decision 2): compare with `str::lines()` and drop blank lines (after trim) on both sides. For `Declare`: current minus, per declared module, one line equal after trimming to `mod <name>;`, `pub mod <name>;` or `pub(crate) mod <name>;`, and at most one `#[cfg(test)]` line directly above it, must equal base line for line. For `CreateRoot`: exactly one of `paths` exists in current, none existed at base, and after the same removals nothing remains. Report each extra or missing line with its path.
- **Freeze runs the chain** (Decision 6): `freeze_scaffold` appends, when the diff problems are empty, `chain_problems` over the current tree for every Rust contract, and `run_dir_problems` for every Rust contract that sets `cwd` (worktree-relative, as `run_dir::resolve_run` reads it), each as `contract '<id>': <problem>`.
- **Run directory** (Decision 4): cargo picks the manifest nearest the directory it runs in; from a workspace root above the package it compiles `default-members` or every member, so a passing same-named test in another member satisfies an exact-name run while the frozen file is never compiled. `run_dir_problems` walks up from `run_dir` to the first `Cargo.toml` in the source and requires it to be the package's own (the package rule above); message: "cargo runs contract tests in <run_dir>, whose manifest <path> does not select package <dir>; set the contract's cwd to <dir>".
- **No tree-sitter** (Decision 7): when the `source-graph` feature is off, `chain_problems` returns one problem and `derive_scaffold` returns Err, both "cannot verify Rust contract collection: loom was built without the source-graph feature". Gate the parsing code with `#[cfg(feature = "source-graph")]` and give the feature-off build the same signatures (`run_dir_problems` and the target model parse no Rust and stay available); every unit test that parses Rust is `#[cfg(feature = "source-graph")]`, and `collection/tests.rs` holds `#[cfg(not(feature = "source-graph"))] #[test] fn collection_fails_closed_without_source_graph` asserting both messages (the stage runs it with `cargo test --no-default-features --lib`, which compiled at plan time); `cargo clippy --no-default-features --lib -- -D warnings` must stay clean (it is a stage gate).
- **Non-Rust contracts**: integration dispatches on the adapter's `language() == "rust"` (`cargo-test`, `cargo-nextest`); any other adapter or none has no scaffold and no chain check. Completion takes the adapter the freeze recorded (`FrozenContract.adapter` in `freeze.json`) for a contract without `runner`, never one re-detected from the implementer's tree.
- **Base reads**: read base blobs with `git cat-file blob <base>:<path>` through `changes::git` (non-zero exit = absent), never `git show` (it lists a tree for a directory path and may apply textconv).

## Known limits (recorded, not built)

A `macro_rules!` defined before a chain link shadows `assert!`/`assert_eq!` inside the frozen file; a constructor in the same test binary can exit it before the frozen tests run while a shadow test elsewhere passes; `build.rs` and `.cargo/config.toml` runner overrides alter the run. Closing them needs each run pinned to the chain's target, an adapter change outside this stage. Knowledge-distill records them.

## Limits

Every file at most 400 lines, every function at most 50 (`cargo test --test maintainability`). No new dependency. No change to `testrun/` adapters. Each worker runs exactly the one check its brief names; clippy and `--all-targets` belong to the stage's main agent.
