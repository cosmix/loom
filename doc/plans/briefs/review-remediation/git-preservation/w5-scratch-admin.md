# git-preservation / w5-scratch-admin: descriptor-anchored removal and ordered publish in `ScratchAdmin`; exact copies

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/git.md`, sections CR-23 (narrowed), CR-40 (narrowed). CR-41 is refuted: no work on `pinned.rs`.

You run no `git` command, touch no path under `.loom/`, and run no tests. Your proof is `cargo check --all-targets` from `loom/`. You write the tests listed below; the orchestrator runs them.

## Findings covered
- CR-23 (narrowed): `publish_linked` removes a linked state directory that git deleted (`sequencer`, `rebase-*`) through the admin PATH with `remove_dir_all`, not through the descriptor opened at `prepare`, so a swap of the admin directory for a link redirects the recursive removal. The torn multi-file publish is not fixable by rollback; the narrowed fix is publish order plus reporting.
- CR-40 (narrowed): `copy_in` copies with `.take(limit)` after a length check made at open, so a file that grew is silently truncated in the private copy git then runs against.

## Files
Own (write), relative to `loom/`:
- `src/git/worktree/scratch_admin_publish.rs`
- `src/git/worktree/scratch_admin.rs`
- `src/git/worktree/scratch_admin_files.rs`
- `src/git/worktree/scratch_admin_publish_tests.rs`
- `src/git/worktree/scratch_admin_tests.rs`

Read only: `src/fs/no_follow_tree.rs` (`remove_entry`), `src/git/worktree/scratch_admin_link_race_tests.rs` and `scratch_admin_link_tests.rs` (fixture patterns).
No other worker edits these files. Other workers change `src/git/worktree/operations.rs` and `src/git/merge/**` at the same time; nothing here depends on them.

## Changes

### 1. CR-23: remove through the admin descriptor (`scratch_admin_publish.rs`)
- `ScratchAdmin::publish_linked`, the `NotFound` arm: replace `remove_dir(&self.admin_path.join(name))` with `crate::fs::no_follow_tree::remove_entry(self.admin.as_fd(), name, 0)` (signature `pub(crate) fn remove_entry(parent: BorrowedFd<'_>, name: &OsStr, depth: usize) -> nix::Result<()>`; it already treats a missing entry as success and never follows a link). Map the `nix::Errno` into `anyhow` with `std::io::Error::from(errno)` and context `cannot remove {}` naming `self.admin_path.join(name)`.
- Delete the free function `remove_dir` at the bottom of the file; it has no other caller (`rg -n 'remove_dir\(' src/git/worktree` to confirm).
- Add `use std::os::fd::AsFd;` if needed; `self.admin` is an `OwnedFd`.

### 2. CR-23: publish order and a named partial publish
- `scratch_admin.rs`, `ScratchAdmin::copy_state`: build `copied_names` as `[INDEX, HEAD]` instead of `[HEAD, INDEX]`, so `publish` swaps `index` before `HEAD` (git's own commit order: a lost race after the index swap leaves the old `HEAD` with the new index, which `git status` reports, instead of a new `HEAD` over an old index). Add a one-line comment saying so.
- `scratch_admin_publish.rs`, `ScratchAdmin::publish`: keep a `Vec<OsString>` of the names published so far (shared indexes, copied files, linked directories, created files, in order). When any step returns `Err`, return it with context `published before the failure: <names joined by ", ">` when the list is not empty (use `to_string_lossy`). A copied file whose stamp shows no change (`publish_copied` early return) is not "published". Keep `publish` under 50 lines: move the per-step loop bodies into helpers if needed. No rollback: a second compare-and-swap could race the same way.

### 3. CR-40: an exact copy or a refusal (`scratch_admin_files.rs`, `copy_in`)
- Replace `let limit = MAX_COPIED_BYTES.min(*budget);` and the `take(limit)` copy with `std::io::copy(&mut (&source).take(meta.len() + 1), &mut copy)` (or the existing `source` by value), then `ensure!(copied == meta.len(), "loom will not run git in the worktree: {} changed while it was being copied", admin_path.join(name).display())`. The two `ensure!` checks above already bound `meta.len()` by `MAX_COPIED_BYTES` and `*budget`, so the cap is unchanged. Then `*budget -= copied;` as today.
- Test seam, immediately before the copy: `#[cfg(test)] super::tests::before_copy(&admin_path.join(name));`.
- The file is 390 lines. Net growth must stay at or under 6 lines (the removed `limit` line offsets part of it). If `cargo fmt` would take it past 398, move `copy_in` with its seam call unchanged into a new `src/git/worktree/scratch_admin_copy.rs`, declared from `scratch_admin.rs` as `#[path = "scratch_admin_copy.rs"] mod copy;`, and import it there in place of `files::copy_in`; make only the items it needs from `files` `pub(super)`.

## Interfaces shared with other workers
None.

## Tests (write them; do not run them)
- `scratch_admin_tests.rs`: add the seam:
  `pub(super) fn before_copy(path: &Path)` calling a thread-local `RefCell<Option<Box<dyn FnOnce(&Path)>>>` taken once, plus `fn inject_before_copy(hook: impl FnOnce(&Path) + 'static)`. Add `a_file_that_grows_while_it_is_copied_refuses_the_command`: with `Fixture::with_extension(false)`, write `MERGE_MSG` into `fixture.admin` with 10 bytes; `inject_before_copy` a hook that appends 5 bytes when `path` ends with `MERGE_MSG`; `ScratchAdmin::prepare(&fixture.repo, &fixture.admin, &fixture.common, false)` errs and the error text contains `changed while it was being copied`. Today it is `Ok` with a 10-byte private copy. Add the shrink case `a_file_that_shrinks_while_it_is_copied_refuses_the_command` (the hook truncates the file to 3 bytes).
- `scratch_admin_publish_tests.rs`: add `a_linked_directory_removal_does_not_follow_a_swapped_admin_directory`: prepare a scratch over an admin that has a `sequencer/` directory (create `fixture.admin.join("sequencer")` with a file before `prepare`, so it is linked); remove the private `sequencer` link in `scratch.path()` (what git does when it ends a sequence); then rename `fixture.admin` aside and put a symlink at `fixture.admin` pointing at a `TempDir` containing `sequencer/keep`; call `scratch.publish()` (its result may be `Ok` or `Err`); assert `keep` still exists in the `TempDir`. Today `remove_dir_all` follows the swapped path and deletes it. Look up how `Fixture` builds `admin` and how `scratch_admin_link_race_tests.rs` plants links; reuse those helpers.
- `scratch_admin_publish_tests.rs`: add `a_failed_publish_names_what_was_already_published`: reuse the racing-HEAD setup of `a_moved_head_is_published_and_a_racing_one_is_reported`, with a staged change so the index is published first; the error text contains `published before the failure: index`.
- Update any existing test that asserts the order of `ScratchAdmin::copied` (search `copied[` and `copied.iter()` in the test files); report each one you change.

## Proof
`cargo check --all-targets` from `loom/`. The orchestrator then runs `cargo test --lib git::worktree::scratch_admin`.

## Traps
- `remove_entry(self.admin.as_fd(), ..)` must use the descriptor opened at `prepare`; re-opening `self.admin_path` reintroduces the race.
- `take(limit)` with `limit = meta.len()` cannot see growth; the read cap must be one byte past the expected length.
- Do not add a rollback of earlier swaps: the narrowed finding rejects it.
- `CONFIG_WORKTREE`, `LOGS` and `REFS` stay private and unpublished; do not add them to the published-names list.
