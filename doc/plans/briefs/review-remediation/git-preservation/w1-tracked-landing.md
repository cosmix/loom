# git-preservation / w1-tracked-landing: refuse tracked overlaps, delete the stash path, pin the fast-forward to the target

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/git.md`, sections G5 (tracked half), G3, G4, CR-10.

## Findings covered
- G5 (tracked): the stash-and-reapply landing compares trees, then runs `reset --hard` on every tracked path, so an editor save between the two is erased.
- G3: `drop_stash_entry` drops `stash@{n}` by position in the shared `refs/stash`, which can drop another worktree's entry. It is deleted with the stash path; no stash-by-index drop survives.
- G4: the merge diff is decoded lossily, so a non-UTF-8 path the merge adds is probed under the wrong name and an ignored file there is overwritten.
- CR-10: the fast-forward advances whatever `HEAD` names; a branch switch or detach after the HEAD check advances the wrong ref and `record_merge` still moves the guard's accepted tip.

## Files
Own (write), relative to `loom/`:
- `src/git/merge/checkout_apply.rs`; `src/git/merge/checkout_apply/tests.rs`
- `src/git/merge/checkout_apply/tests/rollback.rs` (delete); `src/git/merge/checkout_apply/tests/stash_entry.rs` (delete)
- `src/git/merge/checkout_stash.rs` (delete); `src/git/merge/checkout_stash/tests.rs` (delete); `src/git/merge/checkout_stash/tests/end_to_end.rs` (delete)
- `src/git/merge/checkout_gitlink_tests.rs` (delete)
- `src/git/merge/stash_report.rs` (delete)
- `src/git/merge/checkout_state.rs`; `src/git/merge/checkout_state/tests.rs`
- `src/git/merge/fast_forward.rs`; `src/git/merge/fast_forward/tests.rs`
- `src/git/merge/tree.rs`; `src/git/merge/tree/tests.rs`
- `src/git/merge/mod.rs`; `src/git/merge/stage_tests.rs`
- `src/git/mod.rs`
- `src/orchestrator/core/merge_handler/blocked_retry.rs`; `src/orchestrator/core/merge_handler/blocked_retry_tests.rs`
- `src/orchestrator/core/merge_handler/blocked_retry_backup_ref_tests.rs` (delete); `src/orchestrator/core/merge_handler/blocked_retry_displaced_tests.rs` (new)

Read only: `src/git/merge/checkout_files.rs` and `src/git/merge/checkout_files/displace.rs` (w2 writes them), `src/git/merge/test_support.rs`, `src/git/branch/naming.rs` (`branch_ref`), `src/git/target_guard/record.rs`.
Frozen (contract tests, never edit): `src/git/merge/landing_contract_tests.rs`. Loom adds `mod landing_contract_tests;` to `src/git/merge/mod.rs`; keep that line when you edit the file.

## Changes

### 1. G5 tracked half and G3: refuse every tracked overlap, delete the stash path
- `checkout_state.rs`: delete `Classification::Reapply`. In `overlap_action`, the `(false, _)` arm returns `Classification::Blocked { paths: tracked }`. Update the `Classification` and `classify` doc comments: a tracked change on a touched path blocks, as an untracked file with other bytes does. `remove_untracked` is then dropped in that arm on purpose: a blocked merge moves nothing.
- `checkout_apply.rs`: delete `reapply`, `stash_and_land`, `stash_changes`, `reset_changed_nothing`, `dry_run_reapplies`, `backup_ref_name`, `create_backup_ref`, `drop_backup`, the `SystemTime` import and the module doc's stash paragraph (rewrite it: tracked overlap refuses; untracked files the merge replaces with identical bytes are moved aside by w2's `displace_files`).
- Delete `checkout_stash.rs` with its test tree, `stash_report.rs`, and `checkout_gitlink_tests.rs` (every test in it drives the stash path). Remove `mod checkout_stash;`, `mod stash_report;` and `mod checkout_gitlink_tests;` from `merge/mod.rs`.
- `fast_forward.rs`: delete `StashedChanges`, `pop_stash`, `stash_entry`, `drop_stash_entry`, `after_errored_fast_forward`, the `pop` failpoint field and its accessor, and the module doc's `refs/stash` paragraph.
- `tree.rs`: delete `MergeBlock::StashNotRestored` and `MergeBlock::StashResetRefused` and their `Display` arms, the `stash_report` imports and the `pub use super::stash_report::{RollbackChanges, StashReapply}` line. `Advance::Advanced { stash }` becomes the unit variant `Advance::Advanced`; update `update_ref` and the doc comments.
- Reshape `MergeBlock::CheckoutRestoreIncomplete` to exactly `CheckoutRestoreIncomplete { quarantine: PathBuf, failures: Vec<RestoreFailure> }` (w2 builds it). Doc: untracked files the merge moved aside could not all be put back, or changed while being moved; each listed file is in `quarantine` at its checkout-relative path. `Display`: `the merge did not land and untracked files it moved aside are kept in {quarantine} ({path}: {reason}, ...); inspect each, move it back to its path in the checkout, then delete the directory`. `RestoreFailure` stays as is.
- `merge/mod.rs`: drop `StashReapply` from the `pub use tree::{..}` list; `MergeResult::Success` loses its `stash` field and doc; in `merge_and_advance` match `Advance::Advanced => {}`.
- `git/mod.rs`: drop `StashReapply` from the `pub use merge::{..}` list. Also drop `clean_worktrees` from the `pub use worktree::{..}` list: w3 deletes that function (G6) and does not own this file.

### 2. G4: refuse a non-UTF-8 merge path; never overwrite an ignored file
- `advance_in_checkout`: decode the diff with `String::from_utf8(diff)`. On error return `Ok(Advance::Blocked(MergeBlock::FastForwardRefused { detail: "a path the merge touches is not valid UTF-8; merge it by hand".into() }))` before anything is written. Keep `String::from_utf8_lossy` for the status and put this comment above it: touched paths are then all UTF-8, and a non-UTF-8 status path can only overlap one through a UTF-8 prefix, which the lossy text keeps. Decoding the status strictly would block every merge while any unrelated untracked file has such a name.
- `try_fast_forward`: run `merge --ff-only --no-overwrite-ignore --quiet <new>`. `refused_block` already turns git's "untracked working tree files would be overwritten" listing into `UncommittedOverlap`.

### 3. CR-10: the fast-forward must advance the target and nothing else
- `advance_in_checkout` gains the target: `pub(super) fn advance_in_checkout(repo: &Path, target: &str, stage_id: &str, pending: &mut PendingMerge) -> Result<Advance>`. Update its one caller, `advance_target` in `tree.rs`.
- First check, before the existing `rev_parse(repo, "HEAD")` check: run `run_git(&["symbolic-ref", "-q", "HEAD"], repo)?`; unless it exits 0 with trimmed stdout equal to `branch_ref(target)`, return `Ok(Advance::Blocked(MergeBlock::TargetMoved))`.
- New `fast_forward` signature, infallible so that no `?` can leave moved files in quarantine:
  `pub(super) fn fast_forward(repo: &Path, target: &str, (old, new): (&str, &str), displaced: Displaced) -> Advance`.
  - `try_fast_forward` `Ok(Ok(()))`: if `rev_parse(repo, &branch_ref(target))` is `Ok(new)`, call `displaced.discard()` and return `Advance::Advanced`. Otherwise (other commit, or the read failed) return `displaced.restore().unwrap_or(MergeBlock::TargetMoved)` as a block. `merge_and_advance` then never reaches `record_merge`.
  - `Ok(Err(stderr))` (clean refusal): `if let Some(block) = displaced.restore()` return it; else if `rev_parse(repo, "HEAD")` is `Ok` and differs from `old`, `TargetMoved`; else `refused_block(&stderr)`.
  - `Err(error)` (git did not start or timed out): `displaced.restore().unwrap_or_else(|| refused_block(&format!("{error:#}")))`.
  - Rewrite the doc comments to match.
- Test hook, beside `failpoint`: `#[cfg(test)] pub(super) fn inject_before_fast_forward(hook: impl FnOnce(&Path) + 'static)`, a thread-local `RefCell<Option<Box<dyn FnOnce(&Path)>>>` taken and called with `repo` at the top of `fast_forward`, before `try_fast_forward`. The contract tests call it by this exact path: `crate::git::merge::fast_forward::inject_before_fast_forward`.
- `checkout_apply.rs` after the change: `NoOverlap` calls `land(repo, target, pending, Displaced::default())`; `RemoveUntracked { paths }` calls `land_displacing`, which runs `pending.commit()?` first, then `displace_files(repo, stage_id, pending.tree(), &paths)`: `Err(block)` returns `Ok(Advance::Blocked(block))`, `Ok(displaced)` goes to `land`; `Blocked { paths }` returns `overlap(paths)`. `land` is `let new = pending.commit()?; Ok(fast_forward(repo, target, (pending.old(), &new), displaced))`. Delete `land_removing`, and the `remove_files`/`restore_files`/`RollbackChanges` imports.

### 4. Blocked retry: drop the stash exclusions, pause while files sit in quarantine
`blocked_retry.rs`: delete `unrestored_backup_refs` and `holds_backup_stash` and the `run_git_bool` import. Add `fn holds_displaced_files(stage: &Stage) -> bool`: true when `stage.merge.block` is `Some(MergeBlock::CheckoutRestoreIncomplete { quarantine, .. })` and `std::fs::symlink_metadata(quarantine)` is anything but `Err` of kind `NotFound` (an unreadable path counts as present). In `retry_blocked_merge_at` it replaces the `holds_backup_stash` call, with debug text `Operator files are in a merge quarantine; retry paused`. Rewrite the doc paragraph: a retry would land over the paths whose bytes the quarantine holds, and the block is the operator's only pointer to it; deleting the directory resumes retries. `stage.merge.unrestored` is deleted by w6: do not read it.
`blocked_retry_tests.rs`: replace `#[path = "blocked_retry_backup_ref_tests.rs"] mod backup_ref;` with `#[path = "blocked_retry_displaced_tests.rs"] mod displaced;`.

## Interfaces shared with other workers
Uses from w2 (`src/git/merge/checkout_files/displace.rs`, re-exported by `checkout_files.rs` as `pub(super) use displace::{displace_files, Displaced};`):
- `#[must_use] #[derive(Debug, Default)] pub(in crate::git::merge) struct Displaced`; `Default` holds nothing and its methods do nothing.
- `pub(in crate::git::merge) fn displace_files(repo: &Path, stage_id: &str, rev: &str, paths: &[String]) -> Result<Displaced, MergeBlock>`
- `impl Displaced { pub(in crate::git::merge) fn discard(self); pub(in crate::git::merge) fn restore(self) -> Option<MergeBlock>; }`
w2 deletes `remove_files`, `restore_files`, `RemoveFailure`, `RestoreReport` and `inject_before_restore`; do not use them.
Gives to w2: the reshaped `MergeBlock::CheckoutRestoreIncomplete { quarantine: PathBuf, failures: Vec<RestoreFailure> }`.
Gives to w6: `MergeResult::Success { files_changed, insertions, deletions }` with no `stash`; `StashReapply`, `RollbackChanges`, `StashNotRestored`, `StashResetRefused` no longer exist anywhere.

## Tests
- `checkout_apply/tests.rs`: delete `before_stash_push`, `before_stash_reset`, `mod rollback;`, `mod stash_entry;` and the tests `a_refused_fast_forward_whose_pop_fails_names_the_stash`, `a_refused_fast_forward_with_a_working_pop_puts_everything_back`, `stash_failure_reports_a_recreated_untracked_file_and_keeps_the_backup`, `partial_removal_reports_an_incomplete_rollback`, `a_fast_forward_that_errors_leaves_the_stash_unpopped`, `backup_ref_names_differ_for_different_stashes_in_one_second`, `an_existing_backup_ref_is_not_overwritten`, `a_landed_merge_whose_pops_fail_is_a_success_with_the_stash_kept`, `an_errored_fast_forward_writes_removed_files_back_only_where_absent` and helper `errored_fast_forward`. Keep `a_fast_forward_that_errors_without_a_stash_is_a_refusal` and `a_failing_stash_snapshot_blocks_and_leaves_the_checkout_alone` unchanged (the intent-to-add entry is now a tracked overlap, same assertions). Add:
  - `a_tracked_overlap_blocks_and_keeps_the_local_bytes`: `overlapping_checkout(root)`; `merge(root)` is `Blocked(UncommittedOverlap { paths })` with `paths == ["f.txt"]`; `read(root)` equals the local edit; `git stash list` and `git for-each-ref refs/loom` are empty; `main` unchanged. Today: `Success` with a stash.
  - `#[cfg(not(target_os = "macos"))] a_non_utf8_path_the_merge_adds_blocks_and_keeps_the_ignored_file` (APFS rejects such names; pattern in `src/git/worktree/operations_recovery_tests.rs`): commit `.gitignore` = `x*\n` on `main`; on `loom/s1` write the raw name `b"x\xff"` and `git add -f` it through `crate::test_git::git_command(root)` with `OsStr` args, commit, return to `main`; write the same name with other bytes; `merge(root)` is `Blocked`, the bytes are intact, `main` unchanged. Today: `Success`, file overwritten.
  - `a_branch_switch_before_the_fast_forward_records_no_merge`: `stage_branch(root, "s1", &[("b.txt", "b")])`; `inject_before_fast_forward(|repo| { git_ok(repo, &["checkout", "-q", "-b", "other"]); })`; run `merge_stage` with a `lock_dir()` you keep; result is not `Success`, `rev(root, "main")` unchanged, `crate::git::target_guard::accepted_tip(work.path(), "main").unwrap()` is `None` or `Some(old)`. Today: `Success`, `other` advanced.
- `fast_forward/tests.rs`: every `fast_forward(root, (&old, &new), &[], None)` becomes `fast_forward(root, "main", (&old, &new), Displaced::default())` and drops `.unwrap()`; `Advance::Advanced { stash: None }` becomes `Advance::Advanced`. Delete `a_refused_fast_forward_reports_occupied_rollback_paths`, `refused_with_a_stash_and_a_blocked_rollback`, `a_popped_stash_is_not_reported_as_still_stashed`, `a_stash_whose_pop_failed_is_reported_as_stashed`. Add `a_refused_fast_forward_puts_displaced_files_back`: `stage_branch` adding `a.txt`, write `a.txt` with the blob's bytes, `displace_files(root, "s1", &rev(root, "loom/s1^{tree}"), &["a.txt".into()])`, inject the `fast_forward` refusal failpoint, call `fast_forward`; result is `Blocked(FastForwardRefused { .. })` and `a.txt` is back with its bytes.
- `tree/tests.rs`: `Advance::Advanced { stash: None }` becomes `Advance::Advanced` (two tests); in `the_new_blocks_serialize_and_explain_themselves` drop `not_restored` and its assertions and build `incomplete` with `quarantine: PathBuf::from("/q/s1-1")`, asserting the text names `path.txt` and `/q/s1-1`; delete `stash_reapply_notice_says_whether_the_merge_landed_without_the_changes`.
- `checkout_state/tests.rs`: each expected `Classification::Reapply { tracked, .. }` becomes `Classification::Blocked { paths: tracked }` in `tracked_overlap_asks_for_a_reapply` (rename it `tracked_overlap_blocks`), `staged_only_change_counts_as_tracked`, `rename_in_the_checkout_counts_both_paths`, `rename_in_the_merge_is_a_delete_plus_add`, `paths_with_spaces_and_newlines_survive`, `tracked_change_below_or_above_a_touched_path_is_a_tracked_overlap`.
- `stage_tests.rs`: `Success { stash: None, .. }` becomes `Success { .. }`; replace `clean_overlap_is_stashed_and_reapplied` with `a_clean_overlap_blocks_and_changes_nothing` (same setup; `Blocked(UncommittedOverlap)` naming `f.txt`, bytes intact, `main` unchanged).
- `blocked_retry_displaced_tests.rs` (new, `use super::*;`, fixtures `blocked_stage`, `on_disk` from `blocked_retry_tests.rs`): `a_block_with_files_in_quarantine_is_not_retried_until_the_directory_is_gone`: clear the cause (`git stash push -q --include-untracked`), create a directory, record `CheckoutRestoreIncomplete { quarantine: <it>, failures: vec![] }` through `update_stage(.. block_merge ..)`; one retry leaves `MergeBlocked`; remove the directory; the next retry gives `Completed`.
- Contracts to turn green: `tracked-overlap-is-refused-with-bytes-intact`, `non-utf8-merge-path-is-refused`, `branch-switch-before-fast-forward-records-no-merge`; with w2, `replaced-untracked-file-survives-the-window`.

## Proof
From `loom/`: `cargo build --all-targets`, `cargo test --lib git::merge::`, `cargo test --lib orchestrator::core::merge_handler::`. Until w2 and w6 finish, errors in their files are expected; report any that persist.

## Traps
- Leaving `Classification::Reapply` and mapping it to a block at the call site keeps dead `remove_untracked` plumbing; delete the variant.
- A post-check that runs only when `displaced` is non-empty, or a `?` on `rev_parse` after the fast-forward, leaves the misdirected-merge or the moved-files path open.
- Checking `rev_parse(HEAD) == old` alone is the bug: a new branch created at `old` passes it. The symbolic-ref check comes first.
- `git/mod.rs` still re-exporting `clean_worktrees` breaks the build once w3 deletes it.
