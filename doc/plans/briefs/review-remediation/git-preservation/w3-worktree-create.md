# git-preservation / w3-worktree-create: no global prune on remnant repair; a failed scaffold leaves no registration

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/git.md`, sections G6, G7.

## Findings covered
- G6: repairing one empty `.worktrees/<id>` remnant runs repository-wide `git worktree prune`, which drops unrelated registrations (and this stage's own, when its admin entry still exists).
- G7: `git worktree add` registers the worktree, then a failed `scaffold_worktree` returns an error with the registration in place; the retry reuses that worktree unscaffolded.

## Files
Own (write), relative to `loom/`:
- `src/git/worktree/operations.rs`
- `src/git/worktree/operations_recovery_tests.rs`
- `src/git/worktree/mod.rs`

Read only: `src/git/worktree/checks.rs` (`is_valid_git_worktree`), `src/git/worktree/settings.rs`, `src/git/cleanup/batch.rs` (why global prune is unsafe), `src/test_git` helpers, `src/fs/permissions/scratch_home.rs`.
Frozen (contract tests, never edit): `src/git/worktree/operations_contract_tests.rs`. Loom adds `mod operations_contract_tests;` to `src/git/worktree/mod.rs`; keep that line when you edit the file.
Not yours: `src/git/mod.rs` re-exports `clean_worktrees`; w1 owns that file and removes the name from its `pub use worktree::{..}` list. `src/commands/clean/worktrees.rs` has its own `clean_worktrees`, unrelated; do not touch it.

## Changes

### 1. G6: `remove_empty_remnant` (`operations.rs`, near line 283)
Today it calls `clean_worktrees(repo_root)` (`git worktree prune`, no path scope) before `remove_dir`.
- Keep the first check (not a directory, or not empty: bail with the "preserved" message, unchanged).
- Replace the prune with: `if is_valid_git_worktree(path, repo_root)? { bail!(..) }`. Message, exactly: `Worktree {path} is still registered with git but its directory is empty; the directory was preserved. Inspect its admin entry under .git/worktrees, then run \`git worktree remove --force {path}\` or \`git worktree prune\` before retrying` (with `path.display()`). A listing error propagates (`?`), so an unanswerable registration check refuses.
- Otherwise `std::fs::remove_dir(path)` as today.
- Delete `clean_worktrees` (`operations.rs`, about lines 234-238); it has no other caller. Remove it from `pub use operations::{..}` in `src/git/worktree/mod.rs`.

### 2. G7: `create_worktree` undoes the add when scaffolding fails (`operations.rs`, the `scaffold_worktree(&worktree_path, repo_root)?;` line)
- Replace that line with:
  - `if let Err(error) = scaffold_worktree(&worktree_path, repo_root) { .. }`; inside, run `remove_worktree(stage_id, repo_root, true)` and return the scaffold error. With `Ok` removal: `Err(error.context("Failed to scaffold the new worktree; it was removed so a retry starts clean"))`. With `Err(removal)`: `Err(error.context(format!("Failed to scaffold the new worktree, and removing it failed too ({removal:#}); remove it with \`git worktree remove --force {path}\` before retrying")))`.
  - The branch is kept. The retry's `worktree add -b` meets the existing branch and `recreate_or_reuse_branch` recreates it when it has no commits ahead and reuses it otherwise; do not delete it.
- The worktree is seconds old and holds only a checkout and loom's scaffold, so `--force` loses nothing of the operator's. Say this in a short comment above the removal.
- Test seam, in `operations.rs`: `#[cfg(test)] pub(super) fn fail_next_scaffold()`, setting a thread-local `Cell<bool>`. At the very top of `scaffold_worktree`, under `#[cfg(test)]`, when the flag is set clear it and `bail!("simulated scaffold failure")`. The contract test calls it as `super::operations::fail_next_scaffold()` from `git::worktree::operations_contract_tests`.
- `operations.rs` is 300 lines; it ends near 300 after both changes. Keep it under 360.

## Interfaces shared with other workers
- Adds `#[cfg(test)] pub(super) fn fail_next_scaffold()` in `git::worktree::operations` (used by the contract tests).
- Deletes `crate::git::worktree::clean_worktrees`; w1 removes its re-export from `src/git/mod.rs`.
Nothing else is shared.

## Tests
File `src/git/worktree/operations_recovery_tests.rs` (helpers there: `repo()`; also `crate::test_git::{git, git_command, TestRepo}`, `crate::fs::permissions::scratch_home::ScratchHome::set()`; tests that create a worktree carry `#[serial]` and hold a `ScratchHome`).
- Extend `recreates_empty_remnant_after_prune` (G6), renaming it `recreates_an_empty_remnant_without_pruning_other_registrations`: before the call, `git worktree add --detach <TempDir>/other`, commit `--allow-empty` in it, delete its directory; after `get_or_create_worktree("s", ..)` succeeds, assert `.git/worktrees/other/HEAD` still exists. Today it is pruned.
- Rewrite `prune_and_listing_errors_preserve_directory` as `listing_errors_preserve_directory`: in a non-repository the first call now fails on the registration listing; assert it errs, the message does not contain `Failed to prune`, and the directory is still there. The second half (`.git` file "invalid") stays as is. Report this as a changed test: its first assertion pinned the deleted prune.
- Add `#[serial] a_registered_empty_remnant_is_refused_and_kept` (G6): create the worktree with `get_or_create_worktree`, then empty its directory (remove every entry, including `.git`); the next call errs naming `still registered`, the directory and `.git/worktrees/s` both remain.
- Add `#[serial] a_failed_scaffold_removes_the_registration_and_a_retry_scaffolds` (G7): create `<repo>/.loom/work`; `fail_next_scaffold()`; `get_or_create_worktree("s", ..)` errs with `simulated scaffold failure` in its chain; `.worktrees/s` does not exist and `list_worktrees` has no entry ending in `.worktrees/s`; a second call succeeds and `.worktrees/s/.loom/work` is a symlink. Today the first call leaves the registration and the second returns it without the link.
- Contracts to turn green: `empty-remnant-repair-keeps-unrelated-registrations`, `failed-scaffold-leaves-no-reusable-registration`.

## Proof
From `loom/`: `cargo build --all-targets`, `cargo test --lib git::worktree::`.

## Traps
- `git worktree prune --expire` or a prune limited by time is still repository-wide; no prune of any kind belongs on this path.
- Re-running scaffold on reuse instead of removing on failure is the rejected design: a partial `.claude/` makes `setup_claude_directory` skip everything.
- Deleting `loom/<id>` on scaffold failure would destroy commits when the add reused an existing branch.
- `is_valid_git_worktree` matches canonical paths; call it with the same `path` the caller built, not a re-joined one.
