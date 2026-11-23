# git-preservation / w2-untracked-quarantine: move displaced untracked files into a private quarantine; probe without following links

Tier: opus
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/git.md`, sections G5 (untracked half), CR-39.

## Findings covered
- G5 (untracked): `remove_files` unlinks by pathname after several git spawns separate it from the byte comparison, so a file the operator rewrote in that window is deleted; `restore_files` can only rebuild the old blob.
- CR-39: `DiskProbe::disk`, `DiskProbe::equals_blob` and `remove_files` resolve `repo.join(path)`, following every parent component, so a file under an ignored symlinked directory is compared and unlinked outside the checkout.

## Files
Own (write), relative to `loom/`:
- `src/git/merge/checkout_files.rs`
- `src/git/merge/checkout_files/tests.rs`
- `src/git/merge/checkout_files/displace.rs` (new), declared from `checkout_files.rs` as `mod displace;`
- `src/git/merge/checkout_files/displace/tests.rs` (new)

Read only: `src/git/merge/checkout_state.rs` (w1 owns it; CR-39 needs no change there), `src/git/merge/checkout_apply.rs` and `src/git/merge/fast_forward.rs` (w1), `src/git/merge/tree.rs` (w1; `MergeBlock`, `RestoreFailure`), `src/git/merge/in_progress.rs` (`git_dir_for_repo_path`), `src/fs/safe_fs/` (`safe_open_dirfd`, `open_safely`), `src/fs/no_follow_tree.rs` (`remove_entry`), `src/git/merge/test_support.rs`.
Frozen (contract tests, never edit): `src/git/merge/landing_contract_tests.rs`.

## Changes

### 1. G5 untracked half: `displace_files` and `Displaced` (new file `displace.rs`)
Delete from `checkout_files.rs`: `remove_files`, `restore_files`, `restore_one`, `RemoveFailure`, `RestoreReport`, `inject_before_restore`, and the `RollbackChanges` import. Add `pub(super) use displace::{displace_files, Displaced};` and, under `#[cfg(test)]`, `pub(super) use displace::inject_before_displace;`.

Quarantine layout: `<git dir>/loom-displaced/<stage_id>-<unix nanos>/`, each moved entry at its checkout-relative path inside it. `<git dir>` is `crate::git::merge::in_progress::git_dir_for_repo_path(repo)`. `loom-displaced` and every directory loom creates under it are `mkdirat` mode `0o700`.

`pub(in crate::git::merge) fn displace_files(repo: &Path, stage_id: &str, rev: &str, paths: &[String]) -> Result<Displaced, MergeBlock>`, in this order:
1. `paths` empty: return `Ok(Displaced::default())`, creating nothing.
2. Validate every path with `checkout_relative` (section 2). An invalid one: `Err(MergeBlock::UncommittedOverlap { paths: paths.to_vec() })`, nothing moved.
3. Open the git dir with `safe_open_dirfd`; `mkdirat` `loom-displaced` (EEXIST is fine) and open it with `open_safely(.., O_DIRECTORY | O_RDONLY, 0)` so a planted link is refused; `mkdirat` the attempt directory (EEXIST is a refusal) and open it the same way. Any failure here: `tracing::warn!`, then `Err(UncommittedOverlap { paths })`, nothing moved.
4. `#[cfg(test)]` call the `inject_before_displace` hook with `repo`.
5. For each path: `open_parent(repo, rel)`; create the mirror parents inside the attempt directory (`mkdirat` per component, opened no-follow); `libc::renameat(checkout_parent, name, quarantine_parent, name)`. Plain `renameat` is safe here: the destination is a fresh name in a directory loom just made.
6. After each rename, verify the moved entry through the quarantine parent descriptor: `openat(O_RDONLY | O_NOFOLLOW | O_CLOEXEC)`, `fstat` must be a regular file, read at most `blob.len() + 1` bytes and compare with `read_blob(repo, rev, path)`. A non-regular entry, a mismatch, or a blob or read error marks the path "kept".
7. A rename error stops the loop. Then, and when any path is "kept": put back every moved path that is not kept (the `restore` algorithm below). If nothing is kept and every put-back worked, remove the attempt directory and return `Err(UncommittedOverlap { paths })`. Otherwise return `Err(MergeBlock::CheckoutRestoreIncomplete { quarantine, failures })` with one `RestoreFailure` per kept or not-put-back path. Reasons, exactly: `changed while the merge moved it aside`, `could not be compared with the merge's blob: <error>`, `could not be put back: <error>`.
8. Otherwise `Ok(Displaced { .. })`.

Decision on a mismatch (settled): the displaced file stays in quarantine and the merge is blocked naming the path and the quarantine location. Bytes that differ from the incoming blob are never deleted and never renamed back over a path.

`#[must_use] #[derive(Debug, Default)] pub(in crate::git::merge) struct Displaced`: the absolute quarantine path, an owned descriptor of the attempt directory (`Option<OwnedFd>`; `None` in the default), the `loom-displaced` descriptor and the attempt directory's name (to remove it), and the moved paths in order.
- `pub(in crate::git::merge) fn discard(self)`: the fast-forward landed and the checkout holds the same bytes, so remove the attempt directory with `crate::fs::no_follow_tree::remove_entry(base.as_fd(), name, 0)`; an error only warns. The default does nothing.
- `pub(in crate::git::merge) fn restore(self) -> Option<MergeBlock>`: for each moved path, last moved first: `open_parent(repo, rel)` in the checkout, `libc::linkat(quarantine_parent, name, checkout_parent, name, 0)`, then `unlinkat` the quarantine entry. `linkat` never replaces an existing entry (EEXIST), on Linux and macOS alike, so a file the operator or git wrote at the path in the meantime survives and the quarantined copy is kept and reported. No `renameat2`/`renameatx_np`: hard links keep one portable code path. Any failure keeps that entry and adds `could not be put back: <error>`. When every entry went back, remove the attempt directory and return `None`; otherwise `Some(CheckoutRestoreIncomplete { quarantine, failures })`. The default returns `None`. `restore` therefore needs the checkout root: store `repo` in `Displaced`.
- `#[cfg(test)] pub(in crate::git::merge) fn inject_before_displace(hook: impl FnOnce(&Path) + 'static)`: a thread-local `RefCell<Option<Box<dyn FnOnce(&Path)>>>`, taken at step 4. The contract tests call `crate::git::merge::checkout_files::inject_before_displace`.

Every `unsafe` libc call gets a `// SAFETY:` line as in `restore_one` today. Keep each function under 50 lines and `displace.rs` under 360 lines.

### 2. CR-39: probe and read without following links
- New `fn checkout_relative(path: &str) -> Result<&Path>` in `checkout_files.rs`: the components check now inside `restore_one` (only `Component::Normal`, bail `invalid checkout-relative path`). `displace.rs` and the probe use it.
- `DiskProbe::disk`: walk every proper prefix of `path` with `symlink_metadata(self.repo.join(prefix))` before the final component. A directory continues. A symlink returns `DiskEntry::Other`, which `classify` reports as blocking. Any other non-directory returns `DiskEntry::Absent`, which keeps today's file-parent handling in `blocking_parent` (`a_parent_file_the_base_tree_has_does_not_block`). `NotFound` returns `Absent`; any other error returns `Other`. Then the final component as today. An invalid relative path returns `Other`.
- `DiskProbe::equals_blob`: after the mode and `disk` checks, read through `open_parent(self.repo, rel)` plus `openat(O_RDONLY | O_NOFOLLOW | O_CLOEXEC)`, with an `fstat` regular-file check and a read capped at `blob.len() + 1` bytes. Delete the `std::fs::read(self.repo.join(path))`. Put the descriptor read in one helper that `displace.rs` uses for step 6.

## Interfaces shared with other workers
Gives to w1 (pinned, exact):
- `pub(super) use displace::{displace_files, Displaced};` in `checkout_files.rs`
- `pub(in crate::git::merge) fn displace_files(repo: &Path, stage_id: &str, rev: &str, paths: &[String]) -> Result<Displaced, MergeBlock>`
- `#[must_use] #[derive(Debug, Default)] pub(in crate::git::merge) struct Displaced` with `fn discard(self)` and `fn restore(self) -> Option<MergeBlock>`, both `pub(in crate::git::merge)`
- `#[cfg(test)] pub(in crate::git::merge) fn inject_before_displace(hook: impl FnOnce(&Path) + 'static)`, re-exported from `checkout_files.rs`
Uses from w1: `MergeBlock::CheckoutRestoreIncomplete { quarantine: PathBuf, failures: Vec<RestoreFailure> }` (w1 reshapes it in `tree.rs`); `MergeBlock::UncommittedOverlap { paths }` as today. `checked_stdout`, `porcelain_status` and `DiskProbe` keep their signatures.

## Tests
- `checkout_files/tests.rs`: delete `fail_restored_write`, `before_restore`, `inject_before_restore` and the tests `a_failed_removal_restores_the_files_already_removed`, `a_failed_removal_reports_a_recreated_file_without_overwriting_it`, `restoring_keeps_the_exec_bit_of_the_blob`, `restoration_refuses_an_occupied_file_and_a_symlink`, `restoration_reports_missing_blobs_without_creating_a_file`, `restoration_reports_a_blob_read_failure`, `restoration_reports_a_write_failure_and_keeps_the_partial_file_visible`, `restoration_does_not_follow_a_replaced_parent_directory` (each pins the deleted unlink-and-rebuild design). Keep `the_probe_reads_the_disk_and_compares_bytes`. Add `a_symlinked_ignored_parent_blocks_and_the_outside_file_survives` (CR-39): in `init_repo()`, commit `.gitignore` = `a\n`; `stage_branch(root, "s1", &[..])` cannot add an ignored path, so build `loom/s1` by hand with `git add -f a/b/f`; on `main`, make `a` a symlink to an outside `TempDir` holding `b/f` with the same bytes; `merge_stage("s1", "main", root, lock_dir().path(), MergeGate::Enforce)` is `Blocked(UncommittedOverlap)` and the outside `b/f` still exists. Today the outside file is unlinked.
- `displace/tests.rs` (new; fixtures `crate::git::merge::test_support::{init_repo, stage_branch, rev}`): `displaced_files_go_back_on_restore_and_the_quarantine_is_removed`; `discard_removes_the_quarantine`; `a_file_changed_before_the_move_stays_in_quarantine_and_blocks` (hook rewrites the file; the block names it and `quarantine.join(path)` holds the new bytes; the path is empty in the checkout); `restore_keeps_a_file_whose_path_was_reoccupied` (write the path after displacing; `restore` returns the block, the new file is untouched and the quarantined copy remains); `a_planted_link_at_the_quarantine_root_refuses_and_moves_nothing` (symlink `.git/loom-displaced` to a `TempDir`; `Err(UncommittedOverlap)`, the file still in the checkout, the link target empty); `a_failure_partway_puts_back_what_moved` (call `displace_files` directly with `["a.txt", "gone/b.txt"]` where `a.txt` equals its blob and `gone/` does not exist: the second step fails, the result is `Err(UncommittedOverlap)`, `a.txt` is back with its bytes and `.git/loom-displaced` holds no attempt directory); `exec_bit_survives_a_round_trip`.
- Contracts to turn green: `replaced-untracked-file-survives-the-window` (with w1's `land_displacing`).

## Proof
From `loom/`: `cargo build --all-targets`, then `cargo test --lib git::merge::checkout_files::`. Run `cargo test --lib git::merge::` once w1 reports done.

## Traps
- Comparing bytes before the move and trusting that comparison is the bug itself; the comparison that decides is the one made on the moved entry, through the quarantine descriptor.
- `renameat` back into the checkout replaces whatever is there now; the way back is `linkat` then `unlinkat`, never rename.
- `std::fs::remove_dir_all(quarantine_path)` follows a swapped path; use `remove_entry` on the held `loom-displaced` descriptor.
- Returning `Absent` for a symlinked parent (as `NotADirectory` would) hides the CR-39 case; only a regular-file parent maps to `Absent`.
- `git_dir_for_repo_path` on a `.git` file returns the linked target; the quarantine then lives there, which is still the checkout's git directory.
