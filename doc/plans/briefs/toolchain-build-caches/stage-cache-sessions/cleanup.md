# Brief: remove a stage's cache root with the stage (codex unit, gpt-6.2-sol)

Stage `stage-cache-sessions`, worker `cleanup`. Repository root is the working directory. Do not run tests that need fixtures; do not touch git or `.loom/`.

## Context

`loom/src/stage_cache/mod.rs` (merged from the previous stage) defines `StageCache` with `resolve(repo_root, stage_id, home, xdg_cache_home) -> Result<StageCache>` and `root() -> &Path`. Each stage has one cache root, at `<cache base>/loom/stage-cache/<24 hex>`. When the stage's worktree is removed, the root must be removed with it.

The `session` worker adds `pub mod cleanup;` to `mod.rs` in parallel. Do not edit `mod.rs`.

## Files you own

1. `loom/src/stage_cache/cleanup.rs` (new): `pub fn remove_stage_cache(cache: &StageCache) -> anyhow::Result<bool>`.
   - Returns `Ok(false)` when the root does not exist.
   - Refuses with an error when the root itself is a symlink, checked with `symlink_metadata`, never `metadata`.
   - Otherwise removes the tree without ever following a symlink: a symlink entry is unlinked, never descended into.
   - Use a descriptor walk like `loom/src/verify/criteria/confine_linked_home.rs::remove_entry` / `remove_tree` (lines 127-156): `nix` `fstatat` with `AT_SYMLINK_NOFOLLOW`, `unlinkat`, `fchmod` to `S_IRWXU` before emptying a closed directory, and a depth bound of 64.
   - Those functions are private to that module, so copy their approach. Do not change that file.
   - Unit tests in `loom/src/stage_cache/cleanup_tests.rs` (`#[cfg(test)] #[path = "cleanup_tests.rs"] mod tests;` at the bottom of `cleanup.rs`):
     - a nested tree with a 0500 directory is removed;
     - a symlink inside pointing to an outside dir leaves the outside dir's file intact;
     - a symlinked root is refused;
     - an absent root returns false.
2. `loom/src/git/cleanup/batch.rs::cleanup_after_merge` (lines 28-63): after `cleanup_stage_worktree` succeeds, resolve the stage's cache and call `remove_stage_cache`.
   - Resolve with `StageCache::resolve(repo_root, stage_id, &home, xdg.as_deref())`: `home` from `HOME`, `xdg` from `XDG_CACHE_HOME`, read once via `std::env::var_os`.
   - A failure becomes a pushed warning string in the existing `warnings` vector ("stage cache not removed: <error>"), never an error return.
3. `loom/src/commands/clean/worktrees.rs` (around line 277) and `loom/src/commands/init/cleanup.rs` (around line 145): wherever a stage worktree directory is removed with `remove_dir_all`, remove that stage's cache the same way, with the same warning handling.
   - Read each site first. The stage id is the worktree directory's name under `.worktrees/`.
   - If a site has no stage id in scope, leave it and say so in your report.

## Constraints

- Files under 400 lines, functions under 50.
- Run `cargo build --manifest-path loom/Cargo.toml` and `cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings`.
- Report each changed site with file:line.
