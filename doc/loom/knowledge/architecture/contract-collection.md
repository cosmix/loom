# Contract Collection

> Scaffold, chain check

## Contract Collection: Scaffold, Chain Check and Target Model

Completion runs a Rust contract by test name only (`cargo test <name> -- --exact`), so a frozen file counts
only if rustc compiles it under that name. `verify/contracts/collection/` (`mod.rs`, `harness.rs`, `rust/`)
proves that from the Cargo target model and the `mod` chain, reading the stage base through `git cat-file
blob` (never `git show`) and the worktree through bounded reads that follow no symlink. Without the
`source-graph` feature every Rust contract is refused (`NO_SOURCE_GRAPH`): the chain cannot be proven without
parsing Rust.

**Scaffold.** `derive_scaffold(base, [(file, test)])` computes, from the stage base alone, the edits that put
each contract file into a test target: `ScaffoldEdit::Declare` (one out-of-line `mod x;` added to an existing
file) and `ScaffoldEdit::CreateRoot` (exactly one of `<dir>/mod.rs`, `<dir>.rs`, holding only declarations).
The root needing the fewest edits wins; ties go to the lib, then `src/main.rs`, then path order
(`scaffold.rs::rank`). The contract's own module declaration is `test_only` (the signal suggests
`#[cfg(test)]`); a module some contract needs outside tests is not. An edit wholly outside the stage
`working_dir` is dropped (`within`), so the writer cannot satisfy the check for it and the refusal does not
name the dropped path. `stage_scaffold` is the one entry the signal, the freeze and the adjudication prompt
share; a contract whose chain cannot be derived is an error, and `plan verify` lints it over HEAD
(`v2_lints/contracts.rs`). The derivation walks leniently: it tolerates only a missing declaration or module
file, so any other broken link (a base root starting `#![cfg(not(test))]`) makes the scaffold an `Err`, never a
clean diff. A test of "the diff accepts but the chain check refuses" therefore puts the offending line in a
non-scaffold, harness-allowed chain file.

**Strict diff** (`scaffold_problems(edits, base, current)`, run first by `freeze_scaffold`): the worktree may
differ from the base only by those edits. Any other changed line is reported (20 lines named per file, the rest
counted), a reordered declaration or a declaration inserted directly under a kept `#[..]` line of the base is
refused (otherwise `#[cfg(test)]` or `#[allow]` moves from a base item onto the new `mod` line while the
line-for-line comparison still matches), and a created root must be exactly one of its alternatives. Only when
the diff is clean does the chain check run.

**Chain check** (`chain_problems(source, file, test)`): some libtest test target of the owning package reaches
the file through plain `mod` links, and the test path starts with the file's module path under that root.
Passing requires ONE clean candidate root; otherwise the problems of the chain that got furthest are reported.

- A link is one out-of-line `mod name;`. Its outer attributes may be `#[cfg(test)]`, `#[allow(..)]`,
  `#[expect(..)]` and doc comments; `#[path]`, `#[cfg_attr]`, any other `cfg` and any attribute with an
  unreadable path are refused.
- A chain file may carry no inner attribute that cfg's it out: of the cfg attributes only `#![cfg(test)]`
  passes, any `#![cfg_attr(..)]` is refused (crate roots included; a contract behind one belongs in a `tests/`
  file, see `skills/loom-plan-writer/references/v2-contracts.md`), and an inner attribute whose path cannot be
  read, or a shebang, is refused. A file that does not parse is refused.
- A module needs exactly one file (the flat file XOR the directory's `mod.rs`); a root's children and a `mod.rs` file's live in its
  own directory, any other module file keeps them in a directory named after it (the rustc rule, not the simplified one in the plan
  prose).
- Test target model (`targets.rs`, `Cargo.toml` read with the `toml` crate, no `cargo metadata`, which spawns
  a process and can write `Cargo.lock`): lib, `src/main.rs`, `src/bin/*`, `tests/*`, `[[bin]]` and `[[test]]`
  entries, `autobins`/`autotests`. Auto roots under `tests/` and `src/bin/` are only those the contract file's
  own path names (`tests/<n>.rs`, `tests/<n>/main.rs`), because a `FileSource` cannot list directories, so a
  contract in `tests/support/util.rs` reached only from an auto root `tests/it.rs` is refused unless that root is
  a `[[test]]` entry. A model miss can only refuse, never pass. A root with `test = false`, `harness = false`
  or non-empty `required-features` is not a libtest target and is named as the reason.
- `run_dir_problems(source, file, run_dir)`: the nearest `Cargo.toml` at or above the run directory must be the
  owning package's manifest and must select it (a workspace root whose `default-members` leave the package out
  does not). The freeze checks it only for contracts that set `cwd`; completion checks it for the directory
  the contract runs from (`collection_problems`).

**Parsing gotchas** (`rust/items.rs`): tree-sitter-rust 0.24 lexes `#!/**/[..]` and `#! [..]` at a file's start
as a shebang node while rustc reads both as inner attributes, and comments are named-child extras that shift
positional `named_child(0)` reads. Read nodes by kind or field, never by position; normalise `r#` per path
segment; treat an unreadable path or a shebang as refused. See
[mistakes/verification-v2-delivery.md](../mistakes/verification-v2-delivery.md) for the positional-read bypass.

**Harness hygiene.** `harness_source_matches(harness, files)` finds existing files a harness glob matches that
are production source (a parsed language, no test-file convention). `plan verify` warns over HEAD; the
freeze-refusal verdict refuses such a glob at recording. A harness is for test-only files; module wiring comes
from the scaffold. The test_support.rs and conftest.py helpers count as production source for this warning.
