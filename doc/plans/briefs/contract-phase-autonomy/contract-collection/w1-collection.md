# W1: the collection module

Tier: opus, effort xhigh (loom-senior-software-engineer). Wave 1, beside W3 and W4. Read `common.md` in this directory first: it pins the API and every rule. You build the module; W2 wires it in.

## Files you own

- `src/verify/contracts/mod.rs`: add `pub mod collection;` and its line in the module doc only.
- `src/verify/contracts/collection/mod.rs`: `FileSource`, `ScaffoldModule`, `ScaffoldEdit`, `pub mod rust;`, `harness_source_matches` (re-exported), `stage_scaffold`, `ScaffoldCheck`, `freeze_scaffold`.
- `src/verify/contracts/collection/harness.rs`: `harness_source_matches` body.
- `src/verify/contracts/collection/rust/mod.rs`: `chain_problems`, `run_dir_problems`, `derive_scaffold`, `scaffold_problems` (thin; logic in the files below).
- `src/verify/contracts/collection/rust/targets.rs`: `Cargo.toml` target model.
- `src/verify/contracts/collection/rust/items.rs`: tree-sitter parse of top-level `mod` items, their outer attributes, the file's inner attributes, and `has_error()`.
- `src/verify/contracts/collection/rust/chain.rs`: module paths, candidate roots, chain walk.
- `src/verify/contracts/collection/rust/scaffold.rs`: derivation and the strict diff.
- Tests: `src/verify/contracts/collection/tests.rs` (mod.rs and harness.rs), `src/verify/contracts/collection/rust/tests_targets.rs`, `tests_chain.rs`, `tests_scaffold.rs` (each declared `#[cfg(test)] #[path = "<file>"] mod <name>;` from the file it tests, as `completion.rs:204-206` does; each under 400 lines).
- `src/context/rank_source/paths.rs`: make `is_test_path` (L144) `pub(crate)`; `src/context/rank_source.rs`: add `pub(crate) use paths::is_test_path;` beside the re-exports at L35 (`mod paths;` at L31 is private, so the function alone is unreachable). No other change to either.

## Steps

1. **items.rs.** Parse with `tree_sitter::Parser` and `tree_sitter_rust::LANGUAGE.into()` (as `context/extract/rust.rs:139` does; the shared `treesitter::parse` is private, so build the parser locally; `tree-sitter = "=0.27.0"`, `tree-sitter-rust = "=0.24.2"`, both optional under `source-graph`). Return, for a source string: every top-level `mod_item` (name, has body, line range, the text of each preceding `attribute_item`, skipping every comment node between them), every top-level `inner_attribute_item` text, and whether the tree has an error. Top-level only: items inside an inline module or a function are not module declarations of this file. Under `#[cfg(not(feature = "source-graph"))]` the module exposes the same function returning an error.
2. **targets.rs.** `fn package_root(source, file) -> Option<String>` and `fn test_roots(source, package, file) -> Vec<Root>`, where `Root { path: String, test_target: bool }`, per common.md "Package" and "Targets" (edition 2015 rule, `required-features`, explicit entries replacing auto roots, the contract-file-as-root exception), and `run_dir_problems` per common.md "Run directory".
3. **chain.rs.** Module path of a file under each root; candidates per common.md "Root choice"; the chain walk per "Chain rules", including the inner-attribute rule for every chain file except the contract file. `chain_problems` passes when any candidate yields a clean chain.
4. **scaffold.rs.** `derive_scaffold` (reads only `base`, chooses the root per "Root choice", omits a `<dir>.rs` alternative that is an auto target root) and `scaffold_problems` (diff per "Strict scaffold diff").
5. **harness.rs.** `harness_source_matches` per common.md.
6. **stage_scaffold and freeze_scaffold (mod.rs).** Base source: `crate::verify::contracts::changes::git(repo, &["cat-file", "blob", &format!("{base}:{path}")])` (it is `pub(in crate::verify)`; a non-zero git exit means absent), paths relative to the worktree root. Current source: `crate::fs::safe_read::read_bounded` beneath `worktree_root` (follows no symlink; cap at `store::MAX_FROZEN_FILE_BYTES`). Select Rust contracts with `super::resolve_adapter(contract, working_dir)` and `language() == "rust"`; prefix each `contract.file` with `working_dir` relative to `worktree_root`. `stage_scaffold` returns `derive_scaffold`'s result with paths converted back to working_dir-relative (drop any that leave it). `freeze_scaffold` calls `stage_scaffold` (an Err becomes one problem), then `scaffold_problems`, then, when that is empty, `chain_problems` over the current source for every Rust contract and `run_dir_problems` for every Rust contract with `cwd: Some(..)` (Decision 6; `cwd` is worktree-relative, as `run_dir::resolve_run` joins it to the worktree root). No Rust contracts: empty result.

## Tests you write (unit, not the contracts)

- targets: default lib; default bin; `autobins = false` with an explicit `[[bin]] path`; `[[bin]]` without path (package name and other name); `[lib] test = false`; `[[test]] harness = false`; `[[bin]] required-features = ["x"]` is not a test target; a 2015 manifest with one `[[bin]]` turns bin auto-discovery off; a workspace member under a root `Cargo.toml` with only `[workspace]`; an explicit `[[test]] name = "frozen"` with `required-features = ["secret"]` over the auto root `tests/frozen.rs` makes it no test target; `autotests = false` plus a `[[test]] path = "tests/new_contract.rs"` entry, with that file absent at base, derives `Ok(vec![])`, and without the entry derivation errs. run_dir: `run_dir_problems` is empty for the package directory and for a directory between it and the contract file, and non-empty for a virtual workspace root (`[workspace] members = ["a", "b"]`, `default-members = ["b"]`) above member `a` that owns the contract.
- chain: `cfg_attr(test, path = ...)` link refused; `#[cfg(not(test))]` refused; `#[allow(dead_code)]` accepted; `#[path]` with a `// note` line between it and the `mod` item still refused; an inline `mod device_lock { mod contract_tests; }` refused; a duplicated declaration refused; inner `#![cfg(not(test))]` in the crate root refused; inner `#![cfg(test)]` in a module root accepted; a declaring file with a syntax error refused; a test path that does not start with the module path refused by both `chain_problems` and `derive_scaffold`; an integration contract `tests/x_contract.rs` with test `rejects_x` passes with no scaffold and `[[test]] harness = false` on it fails; a contract in `src/bin/tool/contract_tests.rs` with test `contract_tests::x` in a lib+bin crate resolves to the `src/bin/tool/main.rs` root, not the lib.
- scaffold: a two-level new module (`src/a/b/contract_tests.rs`, neither `a` nor `b` at base) derives `Declare a` plus two `CreateRoot`s; two contracts in one new module merge into one `CreateRoot`; a `tests/support/x.rs` module offers only `tests/support/mod.rs`, never `tests/support.rs`; the diff accepts an added blank line beside the declaration and a CRLF current file; `harness_source_matches` flags `src/main.rs`, not `tests/fixtures/a.json` or `src/device_lock/contract_tests.rs`.

Your one check, at the end: `cargo test --lib verify::contracts::collection::`. Do not run clippy: `stage_scaffold`, `freeze_scaffold` and `ScaffoldCheck` have no caller until W2's wave, so `-D warnings` fails on dead code until then. If the build fails in a file you do not own (W4 builds in parallel), report it and stop.

## Do not

Touch completion, freeze, signals or lints (W2). Add a dependency. Change `testrun/`.
