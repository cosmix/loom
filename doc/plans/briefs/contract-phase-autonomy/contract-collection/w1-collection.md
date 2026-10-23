# W1: the collection module

Tier: opus, effort xhigh (loom-senior-software-engineer). Read `common.md` in this directory first: it pins the API and every rule. You build the module; W2 wires it in.

## Files you own

- `src/verify/contracts/mod.rs`: add `pub mod collection;` only.
- `src/verify/contracts/collection/mod.rs`: `FileSource`, `ScaffoldModule`, `ScaffoldEdit`, `pub mod rust;`, `harness_source_matches`, `ScaffoldCheck`, `freeze_scaffold`.
- `src/verify/contracts/collection/harness.rs`: `harness_source_matches` body (re-exported from mod.rs).
- `src/verify/contracts/collection/rust/mod.rs`: `chain_problems`, `derive_scaffold`, `scaffold_problems` (thin; logic in the files below).
- `src/verify/contracts/collection/rust/targets.rs`: `Cargo.toml` target model.
- `src/verify/contracts/collection/rust/items.rs`: tree-sitter parse of top-level `mod` items and their attributes.
- `src/verify/contracts/collection/rust/scaffold.rs`: derivation and the strict diff.
- `src/verify/contracts/collection/tests.rs`, `src/verify/contracts/collection/rust/tests.rs`: unit tests (`#[cfg(test)] #[path = "tests.rs"] mod tests;` as `completion.rs:204-206` does).
- `src/context/rank_source/paths.rs`: make `is_test_path` `pub(crate)`; no other change.

## Steps

1. **items.rs.** Parse with `tree_sitter::Parser` and `tree_sitter_rust::LANGUAGE.into()` (as `context/extract/rust.rs:140` does; the shared `treesitter::parse` is private, so build the parser locally). Return, for a source string, every top-level `mod_item`: name, whether it has a body, its line range, and the text of each preceding `attribute_item` (`#[...]`) and `line_comment`/`block_comment` doc comments. Top-level only: items inside an inline module or a function are not module declarations of this file. Under `#[cfg(not(feature = "source-graph"))]` the module exposes the same function returning an error.
2. **targets.rs.** `fn package_root(source, file) -> Option<String>` (nearest ancestor with a `[package]` Cargo.toml) and `fn test_roots(source, package, file) -> Vec<Root>`, where `Root { path: String, test_target: bool }`, per common.md "Targets". Parse with `toml::Table`. Unit-test: default lib, default bin, `autobins = false` with an explicit `[[bin]] path`, `[[bin]]` without path, `[lib] test = false`, `[[test]] harness = false`, a workspace member (root Cargo.toml with only `[workspace]`).
3. **Module paths and chain (rust/mod.rs + a private helper).** From each root, compute the expected module path of the contract file; choose the roots whose path is a prefix of `test`. For each link, find the declaring file at the conventional location and check the declaration with items.rs per common.md "Chain rules". `chain_problems` passes when at least one test-target root yields a clean chain; otherwise return the problems of the best candidate (the root whose module path matches `test`), or "no test target of package <p> compiles <file> as <module path>".
4. **scaffold.rs.** `derive_scaffold` and `scaffold_problems` per common.md. Derivation reads only `base`.
5. **harness.rs.** `harness_source_matches` per common.md.
6. **freeze_scaffold (mod.rs).** Build the base source from `crate::verify::contracts::changes::git(repo, &["show", &format!("{base}:{path}")])` (it is `pub(in crate::verify)`; a non-zero git exit means absent), paths relative to the worktree root; the current source reads files with `crate::fs::safe_read::read_bounded` beneath `worktree_root` (follows no symlink; cap at `store::MAX_FROZEN_FILE_BYTES`). Select Rust contracts with `super::resolve_adapter(contract, working_dir)` and `language() == "rust"`; prefix each `contract.file` with `working_dir` relative to `worktree_root`. Call `derive_scaffold` (an Err becomes one problem), then `scaffold_problems`; return the scaffold paths relative to `working_dir` (drop any that leave it) and the problems. No Rust contracts: empty check.

## Tests you write (unit, not the contracts)

Besides the target cases in step 2: a `cfg_attr(test, path = ...)` link is refused; `#[cfg(not(test))]` refused; `#[allow(dead_code)]` accepted; an inline `mod device_lock { mod contract_tests; }` refused; a duplicated declaration refused; a test path that does not start with the module path refused by both `chain_problems` and `derive_scaffold`; an integration contract `tests/x_contract.rs` with test `rejects_x` passes the chain with no scaffold; `[[test]] harness = false` on it fails; a two-level new module (`src/a/b/contract_tests.rs`, neither `a` nor `b` at base) derives `Declare a` plus two `CreateRoot`s; two contracts in one new module merge into one `CreateRoot`; `harness_source_matches` flags `src/main.rs`, not `tests/fixtures/a.json` or `src/device_lock/contract_tests.rs`. Run `cargo test --lib verify::contracts::collection::` and `cargo clippy --all-targets -- -D warnings` once at the end.

## Do not

Touch completion, freeze, signals or lints (W2). Add a dependency. Change `testrun/`.
