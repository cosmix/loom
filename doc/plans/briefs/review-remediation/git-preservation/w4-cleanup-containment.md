# git-preservation / w4-cleanup-containment: one containment predicate for CLI and daemon; orphan cleanup moves out of `git/`

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/git.md`, sections G2, CR-42.

## Findings covered
- G2: `require_merged_history` checks the worktree HEAD only when the stage branch is gone, so a merged branch plus a detached worktree HEAD holding unique commits lets cleanup remove the worktree. The daemon's `containment_refusal` has the same gap, never checks `loom/_base/<id>` (which cleanup force-deletes), and runs `is_ancestor_of("HEAD", ..)` with git's own discovery inside the agent-controlled worktree.
- CR-42: `git/cleanup/orphans.rs` imports `crate::orchestrator`, the only such import under `git/`; orphan classification tests ancestry against the live target ref instead of the guard's accepted tip.

## Files
Own (write), relative to `loom/`:
- `src/git/cleanup/removal.rs`; `src/git/cleanup/removal/tests.rs`; `src/git/cleanup/mod.rs`
- `src/git/cleanup/orphans.rs` (delete, moved); `src/orchestrator/merge_lifecycle/orphans.rs` (new); `src/orchestrator/merge_lifecycle/orphans/tests.rs` (new)
- `src/orchestrator/merge_lifecycle.rs`
- `src/orchestrator/merge_lifecycle/containment.rs`; `src/orchestrator/merge_lifecycle/containment/base_tests.rs` (new)
- `src/commands/worktree_cmd.rs`; `src/git/cleanup/worktree_failure_tests.rs`

Read only: `src/git/branch/ancestry.rs` (`is_ancestor_of`, `commits_ahead_of`), `src/git/worktree/pinned.rs` (`WorktreeGit::pinned`), `src/git/target_guard/mod.rs` (`merged_into_accepted`), `src/orchestrator/merge_lifecycle/tests.rs` (fixtures), `src/orchestrator/merge_lifecycle/containment/tag_tests.rs`.
Frozen (contract tests, never edit): `src/git/cleanup/removal/detached_contract_tests.rs`, `src/orchestrator/merge_lifecycle/containment/detached_contract_tests.rs`. Loom adds their `mod` lines to `removal.rs` and `containment.rs`; keep them.

## Changes

### 1. G2: the shared predicate (`removal.rs`)
Add, and export from `src/git/cleanup/mod.rs` as `pub(crate) use removal::require_stage_contained;`:

`pub(crate) fn require_stage_contained(stage_id: &str, completed_commit: Option<&str>, target_branch: &str, repo_root: &Path) -> Result<()>`

It calls `inspect_resources(stage_id, repo_root)?` and hands off to a private `fn require_contained(resources: &StageResources, completed_commit: Option<&str>, target_branch: &str, repo_root: &Path) -> Result<()>`, which replaces `require_merged_history`. Checks, each on its own (no `else`), in this order, failing at the first; any git error is an `Err` (a refusal):
1. `completed_commit`, when `Some`: `require_ancestor(commit, ..)`. Message: `the recorded commit {commit} is not retained by target branch '{target}'`.
2. The stage branch, when `resources.branch_exists`: `commits_ahead_of(&resources.branch_name, target_branch, repo_root)`; non-zero bails `stage branch head '{branch}' still holds {n} commit(s) not retained by target branch '{target}'`.
3. The worktree HEAD, whenever `resources.worktree_exists` and `resources.worktree_path.join(".git").try_exists()?`: `WorktreeGit::pinned(repo_root, &resources.worktree_path)` (context `Failed to pin worktree for history check`), then `git.run(&["merge-base", "--is-ancestor", "HEAD", &branch_ref(target_branch)])`; exit 1 bails `the worktree HEAD of {path} holds commits not retained by target branch '{target}'`; any other non-zero exit bails with git's stderr.
4. The base branch, when `resources.base_branch_present`: `require_ancestor(&branch_ref(&resources.base_branch_name), ..)` with label `stage base branch head`.

`require_ancestor`'s message stays `The {label} '{commit}' is not retained by target branch '{target}'`. These texts keep the substrings existing tests assert: `not retained`, `stage branch head`, `stage base branch head`, `still holds 1 commit`, `worktree HEAD`.

`cleanup_verified_stage` keeps its own empty-commit bail (`Stage has no retained completed commit; refusing normal removal`), then calls `require_contained(&resources, Some(completed_commit), ..)`.

### 2. G2: the daemon uses it (`containment.rs`)
- `containment_refusal` becomes: `let commit = recorded_commit(stage_id, work_dir);` then `require_stage_contained(stage_id, commit.as_deref(), target_branch, repo_root).err().map(|error| format!("stage '{stage_id}': {error:#}"))`. `recorded_commit` keeps its semantics: an unreadable or commit-less record drops the recorded-commit check only.
- Delete `uncontained_refs`, `uncontained_worktree_head`, `uncontained_record` and the now unused imports (`commits_ahead_of`, `is_ancestor_of`, `branch_exists_strict`, `verify_merge_succeeded`, `branch_ref`, `branch_name_for_stage` as applicable).
- Rewrite the module doc: delete the paragraph about `require_merged_history` being a duplicated sibling; say the predicate is `git::cleanup::require_stage_contained`, shared with `cleanup_verified_stage`. Rewrite `containment_refusal`'s doc to the four checks; keep the paragraph on why the recorded commit is additional, never a short circuit.
- Add `#[cfg(test)] mod base_tests;` beside `mod tag_tests;`.

### 3. CR-42: move orphan cleanup (`git/cleanup/orphans.rs` to `orchestrator/merge_lifecycle/orphans.rs`)
- Move the file's content. Imports become `crate::git::cleanup::{branch_exists_strict, CleanupConfig, CleanupResult}` and `super::{CleanupOutcome, MergeLifecycle}`.
- In `merge_lifecycle.rs` (398 lines; one line only) add `pub mod orphans;` directly under `mod containment;`. Do not add a `pub use`: the file must stay at or under 400 lines.
- `src/git/cleanup/mod.rs`: delete `mod orphans;` and `pub use orphans::cleanup_orphaned_worktrees;`.
- `src/commands/worktree_cmd.rs`: import `cleanup_orphaned_worktrees` from `crate::orchestrator::merge_lifecycle::orphans` and drop it from the `crate::git::cleanup` list.
- `src/git/cleanup/worktree_failure_tests.rs`: the one call becomes `crate::orchestrator::merge_lifecycle::orphans::cleanup_orphaned_worktrees(repo.path())`; the assertions do not change.
- Accepted tip (decision settled): classification measures against `crate::git::target_guard::merged_into_accepted(repo_root, work_dir, target, rev)`, which uses the guard's accepted tip when the record has an entry and the live target otherwise. In `classify_stage`, `completed_is_merged` calls it with the commit; `branch_has_no_unmerged_work` keeps the `branch_exists_strict` check and replaces `commits_ahead_of(..) == 0` with `merged_into_accepted(.., &branch_ref(&branch))`. Any `Err` means not safe (keep). Thread `work_dir: &Path` through `partition_worktrees`, `is_orphan`, `classify_stage` and `branch_has_no_unmerged_work` (`is_orphan` gets `work_dir` and derives `stages_dir` itself).
- The removal still goes through `MergeLifecycle::cleanup`, so the live-target predicate of section 2 also gates it; the accepted tip only makes classification stricter.

Verify afterwards: `rg -n 'crate::orchestrator' src/git --glob '!*tests*'` prints nothing.

## Interfaces shared with other workers
Adds `pub(crate) fn require_stage_contained(stage_id: &str, completed_commit: Option<&str>, target_branch: &str, repo_root: &Path) -> Result<()>` (exported from `crate::git::cleanup`). Moves `cleanup_orphaned_worktrees` to `crate::orchestrator::merge_lifecycle::orphans::cleanup_orphaned_worktrees(repo_root: &Path) -> Result<()>`, same signature. No other worker calls either.

## Tests
- `removal/tests.rs` (helpers `init_repo`, `create_stage_worktree`, `merge_stage` in that file):
  - `verified_cleanup_refuses_a_detached_head_with_work_beyond_a_merged_branch`: create, merge, then in the worktree `git checkout --detach` and commit a file; `cleanup_verified_stage` errs naming `worktree HEAD`; the worktree and `loom/<id>` still exist. Today it is removed.
  - `verified_cleanup_refuses_base_branch_work_with_a_merged_branch` already exists as `verified_cleanup_refuses_unmerged_stage_base_branch`; keep it.
  - `stage_containment_refuses_a_base_branch_holding_unmerged_commits`: call `require_stage_contained(id, None, "main", root)` with `loom/_base/<id>` ahead of `main` and the branch merged; it errs with `stage base branch head`.
- `containment/base_tests.rs` (new; `use super::super::tests::{repo_with_stage_commit, merge_stage_branch}; use super::*;`): `a_base_branch_ahead_of_the_target_refuses_cleanup`: `repo_with_stage_commit(id)`, `merge_stage_branch(root, id)`, then in `root`: `git checkout -q -b loom/_base/<id> main`, commit a file, `git checkout -q main` (the pattern of `verified_cleanup_refuses_unmerged_stage_base_branch`); `containment_refusal(&MergeLifecycle::new(id, root, &work_dir), "main")` is `Some` containing `stage base branch head`. Today `None`.
- `orphans/tests.rs` (new; declare `#[cfg(test)] mod tests;` in `orphans.rs`): `a_branch_contained_only_in_the_live_target_is_kept`: `repo_with_stage_commit(id)` from `merge_lifecycle::tests`; seed the guard record at the pre-merge tip with `crate::git::target_guard::check(root, &work_dir, "main")` (no entry: the tip is trusted and recorded); then merge the branch with `git merge --no-ff loom/<id>` outside loom; build `Stage { status: StageStatus::Completed, merged: true, completed_commit: Some(head), ..Stage::default() }`; `classify_stage(id, &stage, "main", root, &work_dir)` returns `false`. Today it returns `true`. Add the positive control: after `crate::git::target_guard::record_advance(&work_dir, "main", &pre_merge_tip, &rev("main"))`, it returns `true`.
- Existing tests to keep green unchanged: `merge_lifecycle::tests` (`still holds 1 commit`, `worktree HEAD`), `containment::tag_tests`, `removal::tests`, `merge_handler` tests asserting containment text.
- Contracts to turn green: `detached-head-blocks-cli-cleanup`, `detached-head-blocks-daemon-cleanup`.

## Proof
From `loom/`: `cargo build --all-targets`, `cargo test --lib git::cleanup::`, `cargo test --lib orchestrator::merge_lifecycle::`, `cargo test --lib orchestrator::core::merge_handler::`.

## Traps
- Keeping an `else if` anywhere between the branch and worktree-HEAD checks reopens G2.
- `is_ancestor_of("HEAD", .., &worktree)` resolves through the worktree's own `.git`; only `WorktreeGit::pinned` reads the registered admin directory.
- A `pub use orphans::..` line in `merge_lifecycle.rs` pushes it past 400 lines.
- `merged_into_accepted` errors when the guard record is unreadable; that must keep the worktree, never treat it as merged.
