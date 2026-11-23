# Validation: git preservation, merge, worktree, cleanup

Revision `62be2a3d2`. Paths are relative to `loom/src/` unless they start with `doc/` or `loom/`.

## G1 — A failed retry spawn force-removes the worktree and force-deletes the stage branch

**Verdict:** CONFIRMED
**Evidence:**
- `orchestrator/core/stage_executor.rs:23-44`: every fresh attempt (first run, `loom stage retry`, crash retry) goes `resolve_worktree` -> gates -> `spawn_stage_agent`.
- `orchestrator/core/spawn_setup.rs:90-95` -> `git/worktree/operations.rs:262-273`: a registered worktree is reused as is, with its commits, index and untracked files.
- `orchestrator/core/stage_preflight.rs:103-115`: the before-stage gate is skipped when prior work exists, so the spawn goes ahead on a dirty worktree.
- `orchestrator/core/stage_spawn.rs:238-257`: on backend error, `block_failed_spawn` runs `git::remove_worktree(.., true)` (`--force`) and `git::delete_branch(.., true)` (`branch -D`). Nothing checks whether this launch created the worktree or whether it holds work.
- The only guard is `spawn_cleanup_allowed` (`stage_spawn.rs:305-316`): it covers `handover` and a native-worker disposition, nothing else. `stage_spawn.rs:249` and `:253` are the only `remove_worktree` and `delete_branch` calls in `orchestrator/` and `commands/`.

**Fix seam:** `block_failed_spawn` (`orchestrator/core/stage_spawn.rs:238`). Recommended: delete the cleanup block (lines 246-254), plus `spawn_cleanup_allowed` (305-316) and its test `native_gate_tests::failed_spawn_keeps_worktree_while_worker_runs` (366-379), both of which this orphans. Rewrite the doc comment at 232-237 to say a failed spawn keeps its worktree and branch. The retry already copes with what is left behind: `get_or_create_worktree` reuses a registered worktree, and `recreate_or_reuse_branch` (`git/worktree/operations.rs:130`) only recreates a branch that has zero commits ahead. The cleanup exists only to recut the branch from a newer base, and a stale base on a pristine branch is harmless because the merge is three-way. Rejected alternative: keep the cleanup when the launch created the worktree and `verify::before_after::find_prior_stage_work` (`verify/before_after.rs:73`) finds nothing. That helper returns `None` when git errors (lines 86-93, 108-114), so it fails open, and the alternative would also need a `created` flag threaded out of `get_or_create_worktree`.
**Regression test:** add a test module in `orchestrator/core/stage_spawn.rs` (private methods are visible there). Build an orchestrator with `target_hold_tests::orchestrator(root, graph(&[ID]))` and a repo with `target_hold_tests::{repo, commit, stage_branch}` (`orchestrator/core/target_hold_tests.rs:31-95`, all `pub(super)`). Create the worktree with `git::get_or_create_worktree`, then commit on the branch, stage a partial edit to a tracked file and add an untracked file. Call `orchestrator.block_failed_spawn(ID, "sess", &anyhow!("boom"), false)`. Assert the branch tip, `git diff --cached` and the untracked file's bytes are unchanged. Today the worktree directory and `loom/<id>` are gone.
**Files a fix touches:** `loom/src/orchestrator/core/stage_spawn.rs`.
**Size / tier:** S; judgment (data loss).
**Overlap:** G7 (the retry reuse path), G6 (the same `get_or_create_worktree`).

## G2 — Cleanup checks the stage branch and skips the registered worktree HEAD; the daemon also skips the base branch

**Verdict:** CONFIRMED
**Evidence:**
- `git/cleanup/removal.rs:159-198` (`require_merged_history`): the worktree-HEAD ancestry check is an `else if` under `if resources.branch_exists` (lines 173-188). With the branch present, a detached HEAD at a unique commit is never checked.
- `require_clean_worktree` (122-147) only rejects uncommitted changes, and `cleanup_after_merge` runs `git worktree remove` without `--force` (`removal.rs:42-48`). Git removes a clean worktree whose HEAD holds unique commits.
- `orchestrator/merge_lifecycle/containment.rs:57-76`: the daemon has the same shape. `Ok(true)` checks only the branch; `uncontained_worktree_head` runs only when the branch is gone (line 71). It never looks at `loom/_base/<id>`, yet `git/cleanup/batch.rs:46-47` (`cleanup_base_branch`, `git/cleanup/base.rs:24-36`) force-deletes that ref.
- The module doc (`containment.rs:8-12`) admits the duplication.
- The existing test `git/cleanup/removal/tests.rs:59-79` deletes `loom/detached` before calling, so it only reaches the `else if`.

**Fix seam:** one shared predicate in `git/cleanup/removal.rs`, for example `pub(crate) fn require_stage_contained(stage_id: &str, completed_commit: Option<&str>, target_branch: &str, repo_root: &Path) -> Result<()>`. It builds `inspect_resources` (`removal.rs:91`) and checks each ref on its own, without `else`:
- the stage branch, if present;
- the worktree HEAD, whenever the directory has a `.git`, through `WorktreeGit::pinned` (as at `removal.rs:180-186`);
- the base branch, if present;
- the recorded commit, when given.

Any git error is a refusal. `cleanup_verified_stage` keeps its own "no completed commit" bail (`removal.rs:164-166`) and passes `Some(commit)`. `containment_refusal` (`orchestrator/merge_lifecycle/containment.rs:41`) calls the predicate with `recorded_commit(..)`, which keeps its current "unreadable record drops requirement 2" semantics, and maps `Err` to `Some(reason)`. Delete `uncontained_refs` and `uncontained_worktree_head` from `containment.rs`, and the duplication note in its module doc. Using the pinned helper also fixes a second defect: `containment.rs:93` runs `is_ancestor_of("HEAD", .., &worktree)` with git's default discovery inside the agent-controlled worktree, unpinned.
**Regression test:** in `git/cleanup/removal/tests.rs` (helpers `init_repo`, `create_stage_worktree`, `merge_stage`, lines 7-33), add two cases:
- merge the stage, then `git checkout --detach` in the worktree and commit there while `loom/<id>` stays merged. Assert `cleanup_verified_stage` errs and the worktree still exists. Today it is removed.
- commit to `loom/_base/<id>` only.

For the daemon, add the same two cases to `orchestrator/merge_lifecycle/containment/tag_tests.rs` (fixture `merge_lifecycle::tests::repo_with_stage_commit`) and assert `containment_refusal` returns `Some`.
**Files a fix touches:** `loom/src/git/cleanup/removal.rs`, `loom/src/git/cleanup/mod.rs` (export), `loom/src/orchestrator/merge_lifecycle/containment.rs`, `loom/src/git/cleanup/removal/tests.rs`, `loom/src/orchestrator/merge_lifecycle/containment/tag_tests.rs`.
**Size / tier:** M; judgment (data loss).
**Overlap:** CR-42 (orphan cleanup goes through `MergeLifecycle::cleanup` -> `containment_refusal`).

## G5 — A concurrent save can be erased while advancing the operator checkout

**Verdict:** CONFIRMED
**Evidence:**
- Tracked path: `git/merge/checkout_stash.rs:214-233` (`store_unchanged`) compares the trees, then `checkout_apply.rs:173` runs `reset_checkout` (`checkout_stash.rs:236-241`, `reset --hard`). That resets every tracked path, overlapping or not. The window is documented at `checkout_stash.rs:208-213` and in `doc/loom/knowledge/concerns/merge-and-recovery-edge-cases.md` ("Editor Saves Between the Stash and the Pop", recorded as an accepted residual).
- Untracked path: `checkout_state.rs:165-176` compares bytes through `DiskProbe::equals_blob` (`checkout_files.rs:75-85`). Later `remove_files` (`checkout_files.rs:145-162`) unlinks by pathname with `std::fs::remove_file(repo.join(path))`, with no re-check.
- Several git spawns separate the compare from the unlink: `pending.commit()` and, on the reapply path, the dry run and the backup ref (`checkout_apply.rs:80-81`, `120-128`). `restore_files` rebuilds the old blob, never the replaced bytes.

**Fix seam:** two parts.
- Untracked (decision settled): replace the unlink in `remove_files` (`checkout_files.rs:145`) with a move-aside. Open the parent with the existing `open_parent(repo, rel)` (`checkout_files.rs:221`, a no-follow walk via `fs::safe_fs::open_safely`). `renameat` the entry into a private quarantine directory (`<git-dir>/loom-displaced/<stage>-<secs>/`, created `0700`), then re-read the quarantined file and compare it with the blob.
  - On mismatch, rename it back with `renameat2(RENAME_NOREPLACE)`, or keep it in quarantine and block naming that path.
  - On fast-forward success, delete the quarantine; on refusal, rename the files back. This replaces `restore_files` for these paths.
- Tracked: a real design choice.
  - (a) Refuse automatic landing whenever `Classification::Reapply` applies: return `MergeBlock::UncommittedOverlap { paths: tracked }` from `advance_in_checkout` (`checkout_apply.rs:60-63`). Then delete `reapply`, `stash_and_land`, `stash_changes`, `reset_changed_nothing`, `dry_run_reapplies`, the backup-ref helpers, `checkout_stash.rs`, the stash half of `fast_forward.rs`, `StashReapply`, the `stash` field of `MergeResult::Success`, `note_merge_stash` consumers and the `StashNotRestored`/`StashResetRefused` retry exclusions.
  - (b) Keep the feature and accept the residual, as the knowledge file records today.

  Recommend (a): any check followed by an unconditional `reset --hard` leaves the window open, while `merge --ff-only` without the stash already refuses to overwrite local edits itself. It is a behavior removal across modules, so the orchestrator or user should confirm it before a worker starts.
**Regression test:** `git/merge/checkout_apply/tests.rs` already has the hook `before_stash_reset` (lines 30-40) and `overlapping_checkout`/`merge` (49-59). Under (a), assert that `merge(root)` returns `Blocked(UncommittedOverlap)` and `f.txt` is byte-identical. For the untracked part, add a thread-local hook before the move in `checkout_files.rs` (pattern: `tests::before_restore`, line 153) that overwrites the file with different bytes. Assert those bytes survive somewhere recoverable and the merge is blocked. Today the file is unlinked.
**Files a fix touches:** `loom/src/git/merge/checkout_files.rs`, `checkout_apply.rs`, and their `tests/`. Under (a) also `checkout_stash.rs` (+ `checkout_stash/tests*`), `fast_forward.rs`, `stash_report.rs`, `tree.rs`, `mod.rs`, `orchestrator/core/merge_handler/*` (stash notes, blocked retry), `commands/stage/merge.rs`, and `doc/loom/knowledge/architecture/merge-checkout-state.md`.
**Size / tier:** L; judgment (data loss).
**Overlap:** G3 (vanishes under (a)), G4 and CR-39 (same `remove_files` and `DiskProbe`), CR-10 (same `advance_in_checkout`).

## G4 — Byte-valued Git names defeat the ignored-file overlap check

**Verdict:** CONFIRMED
**Evidence:**
- `git/merge/checkout_apply.rs:52-56` passes `String::from_utf8_lossy` of the NUL-delimited status and diff into `classify`. A non-UTF-8 byte becomes U+FFFD.
- `checkout_state.rs:180-197` (`classify_unlisted_additions`) asks `probe.disk(path)` for the U+FFFD spelling. `checkout_files.rs:56-73` then gets `NotFound`, giving `DiskEntry::Absent`, and `blocking_parent` finds nothing, so the ignored file never blocks.
- `fast_forward.rs:127` runs `merge --ff-only --quiet <new>` with git's default `--overwrite-ignore`, which replaces the ignored file. The recorded mistake "A Fast-Forward Silently Overwrote an Ignored Local File" (`doc/loom/knowledge/mistakes/merge-in-operator-checkout.md`) is the same failure, reached through a different gap.

**Fix seam:**
- In `advance_in_checkout` (`checkout_apply.rs:40-56`), decode `status` and `diff` with `String::from_utf8`. When either fails, return `Advance::Blocked(MergeBlock::FastForwardRefused { detail: "a path the merge touches is not UTF-8; merge it by hand".into() })`, which refuses conservatively. Carrying byte paths through `checkout_state` would mean changing every `String` path type there, and the refusal is enough.
- Also add `--no-overwrite-ignore` to `try_fast_forward` (`fast_forward.rs:127`) as the final safeguard. Git then reports "The following untracked working tree files would be overwritten by merge:", which `refused_block` (`fast_forward.rs:188`) already parses into `UncommittedOverlap`.

**Regression test:** `git/merge/checkout_apply/tests.rs`, `#[cfg(not(target_os = "macos"))]` (pattern at `git/worktree/operations_recovery_tests.rs:56-60`), using `test_support::{init_repo, stage_branch, commit_file}`. Commit a `.gitignore` with `x*`. On the stage branch, `git add -f` a file named `b"x\xff"`. In the checkout, write the same raw name with different bytes. Assert `merge(root)` is `Blocked`, the local bytes are intact and `refs/heads/main` is unchanged. Today it is `Success` and the file is overwritten.
**Files a fix touches:** `loom/src/git/merge/checkout_apply.rs`, `loom/src/git/merge/fast_forward.rs`, `loom/src/git/merge/checkout_apply/tests.rs`.
**Size / tier:** S; judgment (data loss).
**Overlap:** CR-39 (same `DiskProbe`), G5, CR-10 (same `try_fast_forward` line).

## G3 — Dropping the stash by position can remove another worktree's entry

**Verdict:** CONFIRMED
**Evidence:**
- `git/merge/fast_forward.rs:168-184`: `drop_stash_entry` re-lists the stash (`stash list --format=%H`), takes a position, then runs `stash drop stash@{n}` in a separate spawn.
- `refs/stash` is shared by all worktrees (module doc, lines 5-6), and the race is documented at 139-142.
- The backup ref (`checkout_apply.rs:125-126`) protects only loom's own snapshot. Any entry pushed between the lookup and the drop loses its reflog slot.

**Fix seam:** if G5 option (a) is taken, this code is deleted and G3 closes with it. Otherwise:
- Stop putting the snapshot in the shared stash list. In `store_unchanged` (`git/merge/checkout_stash.rs:214-233`), keep the `write-tree` lock probe and the tree comparison, but drop the `stash store` call.
- `pop_stash` (`fast_forward.rs:143-166`) applies by commit id. `git stash apply <commit>` accepts any stash-shaped commit. Remove the `stash_entry` presence gate and `drop_stash_entry`.
- The backup ref under `refs/loom/autostash/` stays the operator's recovery handle, as `MergeBlock::StashNotRestored { backup_ref }` already reports.
- Update `doc/loom/knowledge/architecture/merge-checkout-state.md` where it says the merge's entry sits in the stash list.

**Regression test:** `git/merge/checkout_apply/tests/stash_entry.rs`. Use the `before_stash_reset` hook (`checkout_apply/tests.rs:34-40`) to run `git stash push` of an unrelated edit after the store. After `merge(root)`, assert that the unrelated stash commit is still in `git stash list --format=%H`. Today it can be dropped. Under the fix, also assert no loom entry is in the list.
**Files a fix touches:** `loom/src/git/merge/fast_forward.rs`, `loom/src/git/merge/checkout_stash.rs`, `loom/src/git/merge/checkout_apply/tests/stash_entry.rs`, `loom/src/git/merge/fast_forward/tests.rs`.
**Size / tier:** M; mechanical.
**Overlap:** G5.

## G6 — Repairing one empty remnant runs repository-wide `git worktree prune`

**Verdict:** CONFIRMED
**Evidence:**
- `git/worktree/operations.rs:262-277`: a stage directory without a registered `.git` goes to `remove_empty_remnant` (283-292), which calls `clean_worktrees` (235-238), that is `git worktree prune` with no path scope.
- `git/cleanup/batch.rs:48-51` documents why global prune is unsafe and gates it on forced removal.
- The prune also drops this stage's own registration when the directory is empty but the admin entry (`.git/worktrees/<id>`, with its detached HEAD and index) still exists. That is a second loss beside the unrelated one.

**Fix seam:** `remove_empty_remnant` (`operations.rs:283`). Replace the `clean_worktrees` call with `if is_valid_git_worktree(path, repo_root)? { bail!(...) }` (`git/worktree/checks.rs:51`, matching on canonical paths). The message should say the path is still registered, the operator must run `git worktree remove --force <path>` or `git worktree prune` after checking its admin entry, and the directory was preserved. Otherwise `remove_dir` as today. Then delete `clean_worktrees` (`operations.rs:234-238`), which has no other caller, and its re-exports in `git/worktree/mod.rs:46` and `git/mod.rs:33`.
**Regression test:** `git/worktree/operations_recovery_tests.rs`.
- Extend `recreates_empty_remnant_after_prune` (31-40): add an unrelated `git worktree add --detach` elsewhere, commit in it, then delete its directory. Assert `.git/worktrees/<other>/HEAD` still exists after `get_or_create_worktree("s", ..)`. Today it is pruned.
- `prune_and_listing_errors_preserve_directory` (42-54) asserts the "Failed to prune" message and must be rewritten: in a non-repo the listing error now surfaces first.
**Files a fix touches:** `loom/src/git/worktree/operations.rs`, `loom/src/git/worktree/mod.rs`, `loom/src/git/mod.rs`, `loom/src/git/worktree/operations_recovery_tests.rs`.
**Size / tier:** S; judgment (data loss).
**Overlap:** G7, G1 (same `get_or_create_worktree`).

## G7 — Registration survives a failed scaffold and the retry reuses it unscaffolded

**Verdict:** CONFIRMED
**Evidence:**
- `git/worktree/operations.rs:75-85`: `git worktree add` registers the worktree, then `scaffold_worktree` (96-117) can fail. `ensure_work_symlink`, `setup_claude_directory` and `setup_root_claude_md` all use `?`. The error reaches `spawn_setup.rs:97-106`, which blocks the stage.
- On retry, `operations.rs:266-273` returns the registered worktree without scaffolding.
- `setup_claude_directory` (`git/worktree/settings.rs:86-117`) skips everything when `.claude/` exists. A failure after `create_dir_all` (line 92) but before `create_worktree_settings` (113) would stay partial even if scaffold reran.
- Nothing else calls the scaffold steps on the spawn path. Outside `operations.rs` they are only called in tests.

**Fix seam:** `create_worktree` (`operations.rs:85`). Make creation atomic from the caller's view: on `scaffold_worktree` error, run `remove_worktree(stage_id, repo_root, true)` (`operations.rs:193`) and return the original error with the removal result as context. The worktree is seconds old and holds only a checkout and loom scaffold, so nothing of the operator's is lost. The branch is kept, and the retry's `worktree add -b` falls into `recreate_or_reuse_branch`, which recreates a zero-ahead branch and reuses one that has commits. This is preferred over re-running scaffold on reuse: a project-tracked `.claude/` makes "idempotent on reuse" need a tracked-path check (`git/cleanup/worktree.rs:91` `tracked_scaffold_paths` is `pub(super)` in another module), and partial `.claude/` state would need per-entry guards.
**Regression test:** `git/worktree/operations_recovery_tests.rs` (`repo()`, `ScratchHome::set()`). Make `.loom/work` resolve but make `setup_root_claude_md` fail: put a root `CLAUDE.md` in the repo and pre-create `.worktrees/` read-only after the add. A simpler route is a `#[cfg(test)]` thread-local failpoint in `scaffold_worktree`, following the `failpoint` pattern in `git/merge/fast_forward.rs:211-251`. Assert `create_worktree` errs, `is_valid_git_worktree(path)` is false and the `.worktrees/<id>` directory is gone. Then a second `get_or_create_worktree` produces `.loom/work` and `CLAUDE.md` links. Today the first call leaves the registration, and the second returns `Ok` without the links.
**Files a fix touches:** `loom/src/git/worktree/operations.rs`, `loom/src/git/worktree/operations_recovery_tests.rs`.
**Size / tier:** S; needs-exploration (the failpoint fixture).
**Overlap:** G1, G6.

## CR-10 — Operator branch switch or detach between the HEAD check and `merge --ff-only`

**Verdict:** CONFIRMED (worse than described)
**Evidence:**
- `git/merge/tree.rs:264-279` (`advance_target`) decides that the target is checked out in R from `git worktree list`. Then `checkout_apply.rs:35-37` only checks that `HEAD`'s commit equals `old`, not which ref `HEAD` names.
- Several git spawns follow (status, diff, probes, commit, and on the stash path a dry run, backup ref and reset) before `fast_forward.rs:127` runs `merge --ff-only`, which advances whatever `HEAD` is. A branch the operator created or switched to at `old`, or a detached `HEAD`, receives `M`; `refs/heads/<target>` stays at `old`.
- `git/merge/mod.rs:231-235` then calls `record_merge`, and `git/target_guard/record.rs:161-172` moves the guard's accepted tip from `old` to `M`.
- Worse: the daemon's phantom-merge proof, `verify_and_finalize_merge` (`orchestrator/core/merge_handler.rs:47-87`) -> `target_guard::merged_into_accepted` (`git/target_guard/mod.rs:336-346`), tests ancestry against the accepted tip, now `M`, so it passes. The stage is written `merged = true` while the target ref lacks the commit.
- Post-merge cleanup is still refused, because `containment_refusal` checks the live ref, so the branch survives.

**Fix seam:**
- `advance_in_checkout` (`checkout_apply.rs:30`): before classifying, require `git symbolic-ref -q HEAD` == `branch_ref(target)`, else `Blocked(TargetMoved)`. This needs the target name: add a `target: &str` parameter, passed from `advance_target` (`tree.rs:272`).
- `fast_forward` (`fast_forward.rs:47-54`): after `try_fast_forward` succeeds, require `rev_parse(repo, &branch_ref(target)) == new`, else return a new `MergeBlock::FastForwardMisdirected { head }` (or `TargetMoved`) so that `merge_and_advance` never reaches `record_merge`. The pre-check narrows the window; the post-check stops the false record, which is what matters.
- Reuse `tree::rev_parse` and `git::branch::branch_ref`. `checked_out_branch_with` (`git/branch/operations.rs:45`) reads the symbolic ref through a `WorktreeGit`.
- A new block variant must also be added to `MergeBlock` display and blocked-retry handling (`orchestrator/core/merge_handler/blocked_retry.rs`). Reusing `TargetMoved` avoids that and is acceptable.

**Regression test:** `git/merge/checkout_apply/tests.rs`. Add a thread-local hook immediately before `try_fast_forward` (pattern: `before_stash_reset`) that runs `git checkout -q -b other`. With `init_repo`/`stage_branch`/`lock_dir`, run `merge(root)`. Assert the result is not `Success`, `refs/heads/main` is unchanged, and the target guard record under `lock_dir()` did not move to the merge commit. Today it returns `Success`, `other` advances and `main` does not.
**Files a fix touches:** `loom/src/git/merge/checkout_apply.rs`, `loom/src/git/merge/fast_forward.rs`, `loom/src/git/merge/tree.rs`, `loom/src/git/merge/checkout_apply/tests.rs`; optionally `loom/src/git/merge/tree.rs` (`MergeBlock`) and its consumers if a new variant is chosen.
**Size / tier:** M; judgment (concurrency, phantom merge).
**Overlap:** G4 and G5 (same functions).

## CR-23 — Partial `ScratchAdmin::publish()` and path-based removal of a linked directory

**Verdict:** NARROWED
**Evidence:**
- `git/worktree/scratch_admin_publish.rs:15-32`: publish is a sequence of per-file compare-and-swaps under git's own `<name>.lock` (`scratch_admin_files.rs:256-289`). A later CAS failure leaves earlier swaps in place: `HEAD` is published before `index` (`scratch_admin.rs:188`), so a lost index race after a commit leaves the new `HEAD` with the old index. Each step is a CAS that git itself would honor, and git's own multi-file updates are no more atomic. A rollback would need a second CAS and could race the same way, so "restore backups" is not implementable as stated.
- The path-based removal is real: `publish_linked` (99-111) calls `remove_dir(&self.admin_path.join(name))` (153-162), which uses `symlink_metadata` and `remove_dir_all` through the admin path, not the `self.admin` descriptor. That contradicts the module promise at `scratch_admin.rs:38-40` ("applies what git changed through a descriptor opened at the start"). Sessions can write the admin and common directories (`scratch_admin.rs:3-4`), so a swap of `.git/worktrees/<id>` for a link between `prepare` and `publish` redirects the recursive removal. It needs a pinned command that deletes a `LINKED_DIRS` entry (`sequencer`, `rebase-*`) plus a concurrent swap.

**Fix seam:** `publish_linked` (`scratch_admin_publish.rs:104-106`): replace `remove_dir(&self.admin_path.join(name))` with `crate::fs::no_follow_tree::remove_entry(self.admin.as_fd(), name, 0)` (`fs/no_follow_tree.rs:55`, `pub(crate) fn remove_entry(parent: BorrowedFd<'_>, name: &OsStr, depth: usize) -> nix::Result<()>`), mapping the errno with context. Delete `remove_dir` (153-162). For the torn publish, the smallest sound change is ordering plus reporting: publish `index` before `HEAD`, which is git's own commit order, and on a mid-publish error name the files already published in the error. No rollback.
**Regression test:** `git/worktree/scratch_admin_publish_tests.rs` (and `scratch_admin_link_race_tests.rs` for its race fixtures). Prepare a scratch whose admin has `sequencer/`, delete the private link, then replace the real admin directory with a symlink to a tempdir containing `sequencer/keep`. Call `publish()` and assert `keep` survives. Today it is removed.
**Files a fix touches:** `loom/src/git/worktree/scratch_admin_publish.rs`, `loom/src/git/worktree/scratch_admin.rs` (copy order), `loom/src/git/worktree/scratch_admin_publish_tests.rs`.
**Size / tier:** S; judgment (security).
**Overlap:** CR-40, CR-41 (same `ScratchAdmin`).

## CR-39 — Gitignored intermediate symlinked directory is followed by `DiskProbe` and `remove_files`

**Verdict:** NARROWED
**Evidence:**
- `git/merge/checkout_files.rs:56-85`: `disk` and `equals_blob` run `symlink_metadata` and `std::fs::read` on `repo.join(path)`, which follows every component but the last.
- `checkout_state.rs:180-197` probes only the merge's added paths that status does not list. Git status does not descend into a symlink, so `a/b/f` under an ignored symlink `a` reaches `compare_bytes`. When the external file equals the blob, `remove_files` (`checkout_files.rs:145-162`) unlinks it through the link, outside `repo`.
- Narrowing: the symlink must be in the operator's own checkout, and ignored; agents work in stage worktrees. Only a file byte-identical to the incoming blob is removed. `restore_files` already refuses link parents (`open_parent`, 221-239), so on rollback the file is reported, not silently lost.
- The cited `checkout_state.rs:86-88` is `classify`'s doc comment; the relevant code is `checkout_state.rs:180-212`.

**Fix seam:**
- `DiskProbe::disk` (`checkout_files.rs:56`): walk the parent components of `path` with `symlink_metadata`. Any symlink, or any non-directory parent, yields `DiskEntry::Other`, which blocks via `differing`.
- `equals_blob` then can only read regular files under real directories, but it should read through the descriptor: `open_parent(repo, rel)` + `openat(O_NOFOLLOW)`.
- `remove_files` (`checkout_files.rs:151`): open the parent with the existing `open_parent` and `unlinkat` the basename. If G5's move-aside lands, use `renameat` from that same parent fd instead. Path validation (`Component::Normal` only) already exists in `restore_one` (184-190); extract it into a shared helper `fn checkout_relative(path: &str) -> Result<&Path>` and use it in all three places.

**Regression test:** `git/merge/checkout_files/tests.rs`. Ignore `a` in the operator checkout, make `a` a symlink to an outside tempdir containing `b/f` with the blob's bytes, and stage a branch that adds `a/b/f`. Assert the merge is blocked and the outside `b/f` still exists. Today it is unlinked.
**Files a fix touches:** `loom/src/git/merge/checkout_files.rs`, `loom/src/git/merge/checkout_files/tests.rs`.
**Size / tier:** S; judgment (security, data loss).
**Overlap:** G5 (same `remove_files`), G4 (same `DiskProbe`).

## CR-40 — `copy_in` truncates a file that grows past `limit`

**Verdict:** NARROWED
**Evidence:**
- `git/worktree/scratch_admin_files.rs:154-179`: the length checks use the fstat taken at open. `io::copy(&mut source.take(limit), ..)` then silently stops at `limit` if the file grew meanwhile, and the copy is short.
- The claimed "truncated admin state written back on publish()" does not happen. `Copied.real` is the pre-copy `Stamp` (inode, len, mtime; `scratch_admin.rs:216-223`), and `swap` refuses the publish unless the real file still matches that stamp (`scratch_admin_files.rs:268-272`). A grown file fails the CAS.
- What remains is git running one command against a truncated private copy. `index` carries a checksum and fails loudly; a text file such as `MERGE_MSG` could feed a short message to that command.

**Fix seam:** `copy_in` (`scratch_admin_files.rs:173-176`): copy with `.take(limit + 1)` and `ensure!(copied == meta.len(), "... changed while it was being copied")`. That rejects both growth and shrinkage and keeps the copy consistent with the stamp it is published against.
**Regression test:** `git/worktree/scratch_admin_tests.rs`. A writer thread appends to `MERGE_MSG` while `ScratchAdmin::prepare` runs is racy. A deterministic unit test of `copy_in` with `budget` set below the file's length plus a pre-grown file is simpler: write `n` bytes, open, then append before the copy through a test seam. Assert `Err`. Today it is `Ok` with `n` bytes copied.
**Files a fix touches:** `loom/src/git/worktree/scratch_admin_files.rs`, `loom/src/git/worktree/scratch_admin_tests.rs`.
**Size / tier:** S; mechanical.
**Overlap:** CR-23.

## CR-41 — `run_pinned` reads identity from the unvalidated real git dir and compares canonical vs raw `repo_root`

**Verdict:** REFUTED
**Evidence:**
- Identity read: `git/worktree/pinned.rs:283-290` runs `identity_args` against the real `pin.git_dir` before `ScratchAdmin::prepare`. This is deliberate and documented: `pinned_settings.rs:1-18` explains that `includeIf "gitdir:"` needs the real path, and `collect_identity` (118-141) keeps only the `system`/`global`/`local` scopes, so a session-written `config.worktree` (scope `worktree`) is read as data and dropped. `git config --list` runs no filters or hooks, and the read is bounded by `git_timeout` (`git/runner.rs:221-223`).
- `work_tree != repo_root` at `pinned.rs:292`: `WorktreeGit::pinned` (136-150) succeeds only for a worktree that `registered_admin_dir` finds under `<common>/worktrees/`, which a main checkout never is. `pinned_main` (115-128) sets `repo_root = work_tree`, both canonical. So the comparison is never between the main checkout and a raw spelling of itself, and the reflog flag is never wrongly applied to main-repo commands.

**Fix seam:** none needed. Optional hardening, not a defect: canonicalize `repo_root` in `pinned` (`pinned.rs:145`) so the field's meaning matches `pinned_main`.
**Regression test:** none.
**Files a fix touches:** none.
**Size / tier:** S; mechanical (optional hardening only).
**Overlap:** CR-23.

## CR-42 — `git/cleanup/orphans.rs` imports `orchestrator`, and orphan classification uses the live target instead of the guard's accepted tip

**Verdict:** NARROWED
**Evidence:**
- Layering is confirmed: `git/cleanup/orphans.rs:10` imports `crate::orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle}`. It is the only `use crate::orchestrator` under `git/`.
- `classify_stage` (`orphans.rs:136-174`) and `branch_has_no_unmerged_work` (176-180) test ancestry against the live `refs/heads/<target>`. The daemon's `containment_refusal` (`orchestrator/merge_lifecycle/containment.rs:57-123`) does the same, and orphan cleanup calls it again through `MergeLifecycle::cleanup` (`orphans.rs:205-208`).
- No loss follows from the live target: commits contained in the live target ref at deletion time are retained by that ref, whether or not the guard accepted the move. Checking only the accepted tip would be stricter, never safer for data. The ancestry part is a policy preference, not a correctness defect.

**Fix seam:** move `git/cleanup/orphans.rs` to `orchestrator/merge_lifecycle/orphans.rs`, re-exported for `commands/worktree_cmd.rs:10,53`. Drop the `pub use orphans::cleanup_orphaned_worktrees` at `git/cleanup/mod.rs:53` and update the test call at `git/cleanup/worktree_failure_tests.rs:178`. Keep the live-target ancestry. Once G2 lands, `classify_stage`'s ref checks could call the shared predicate from G2 instead of `branch_has_no_unmerged_work`, removing a third copy of the containment logic.
**Regression test:** none for the move (existing tests in `git/cleanup/worktree_failure_tests.rs` follow the function). The architecture check is `rg -n 'use crate::orchestrator' loom/src/git` returning nothing.
**Files a fix touches:** `loom/src/git/cleanup/orphans.rs` (moved), `loom/src/git/cleanup/mod.rs`, `loom/src/orchestrator/merge_lifecycle.rs` (module declaration), `loom/src/commands/worktree_cmd.rs`, `loom/src/git/cleanup/worktree_failure_tests.rs`.
**Size / tier:** M; mechanical.
**Overlap:** G2.
