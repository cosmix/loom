# Merge Flow

> Finished stage to target branch

## Merge Flow

How a completed worktree stage reaches the target branch, in the order the daemon runs it, and
what every outcome leaves on disk. Knowledge stages are outside this flow: they commit to the
base directly and `complete_knowledge_stage` sets `merged: true` with no branch.

## The stage is `Completed` before any merge runs

`loom stage complete` inside a worktree verifies only. The PostToolUse broker forwards
`CompleteStage` to the daemon, whose `handle_complete_stage` (`daemon/server/control_complete.rs`)
calls `stage.try_complete(None)` and writes the stage file: `status: completed`, `merged: false`,
`completed_commit: null`. No merge has happened yet, so every merge attempt that follows sees a
stage that is already `Completed`, a terminal state the transition table refuses to leave.

## Where the first merge attempt actually happens

The main loop (`orchestrator/core/orchestrator.rs`) runs `sync_graph_with_stage_files`
(`orchestrator/core/recovery.rs`) before `monitor.poll()` on every tick. The sync finds the fresh
stage, derives `completed_commit` from the `loom/<id>` branch head, runs the ancestry check (false,
nothing is merged yet), marks the graph node Completed, and queues the stage for the "Fix 11"
one-shot retry at the end of the same function. That retry calls `try_auto_merge`
(`orchestrator/core/merge_handler.rs`), which is the first and normal merge attempt. The
`StageCompleted` monitor event, handled next in the same tick by `handle_stage_completed`
(`orchestrator/core/completion_handler.rs`), calls `try_auto_merge` a second time; by then
`merged` is true and the call only runs `cleanup_already_merged`.

`try_auto_merge` in order: auto-merge enabled check (stage, then plan, then daemon default),
`auto_merge_precheck_blocks` (when the branch exists: the merge gate, then the phantom guard,
`commits_ahead_of` must be > 0), record `completed_commit` from the branch head,
`MergeLifecycle::reconcile_overlay`, then `attempt_auto_merge` (`orchestrator/auto_merge.rs`, which
spawns no session) and `apply_auto_merge_outcome` (`orchestrator/core/merge_handler/auto_merge_outcome.rs`).
`attempt_auto_merge` calls `merge_stage` (`git/merge/mod.rs`), described next.

## `merge_stage` merges off the operator's checkout

`merge_stage(stage_id, target, repo_root, work_dir, gate)` runs under `MergeLock`. It never checks out a
branch, runs a three-way merge, or creates `MERGE_HEAD` in the operator's main checkout R. Only when
the target is checked out in R does it change R's files: `git merge --ff-only`, plus the stash and
untracked-file removal of the reapply path:

1. An operator operation in progress in R (`MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`,
   `rebase-merge/`, `rebase-apply/`) returns `Blocked(OperatorOperation { marker })`.
2. The target guard (`target_guard::check_locked`, see [Target Guard](target-guard.md)) returns
   `Blocked(TargetHeld { target, accepted, observed })` while the target moved outside loom and the
   operator has not accepted it. It runs before the `AlreadyUpToDate` check and for every gate mode: the
   control-path bypass covers a stage's own paths, never a moved target. `accepted` is empty when the
   record cannot be read, `observed` when the live tip cannot be read.
3. `T0 = rev-parse refs/heads/<target>`, `B = rev-parse refs/heads/loom/<id>`; names are qualified because a tag named like a branch would shadow it (see [Branch resolution](#branch-resolution)). `B` already in `T0` is `AlreadyUpToDate`.
4. `merge_tree` (`git/merge/tree.rs`) runs `git merge-tree --write-tree --name-only --no-messages -z T0 B`.
   Exit 0 is clean (tree id first), exit 1 is a conflict (tree id, then the conflicted paths). Exit 1
   with empty stdout (bad revision) or any other code is an error. A conflict returns
   `Conflict { conflicting_files }` and R is untouched. After a clean tree, the control-path gate
   checks the exact landing diff (see [The merge gate](#the-merge-gate)).
5. Clean: `commit_merge` (lazily, through `PendingMerge`) calls `signing::commit_tree`, which runs `git commit-tree <tree> -p T0 -p B -m "Merge loom/<id> into <target>"`
   and adds `-S` when `commit.gpgsign` is true (under `SIGN_TIMEOUT`, with the signing environment the
   daemon captured at startup; see [Daemon-Owned Commits](daemon-owned-commits.md)). A signing failure
   is a downcastable `CommitTreeError { signing: true }`. The commit always has two parents, even when
   `B` already contains `T0` after a resolver merged the target in. Stats come from `git diff --shortstat T0 M`.
6. `advance_target` (`tree.rs`) moves the target to the merge commit `M`, by where the target is
   checked out (`git worktree list --porcelain`):
   - nowhere (R on another branch or detached included): `git update-ref -m "loom: merge loom/<id>"
     refs/heads/<target> M T0`. A refusal because the target moved is `Blocked(TargetMoved)`.
   - in another worktree: `Blocked(TargetCheckedOutElsewhere { path })`.
   - in R: `advance_in_checkout` (`git/merge/checkout_apply.rs`), a guarded fast-forward that keeps the
     operator's uncommitted work. See [merge-checkout-state](merge-checkout-state.md).

   After the advance `record_merge` calls `record_advance`, which moves the guard's accepted tip to `M`;
   a failed `record_advance` only warns, and the next guard check evaluates the move.

`MergeResult` is `Success { files_changed, insertions, deletions, backup_ref, stash }`,
`Conflict { conflicting_files }`, `AlreadyUpToDate`, `Held { reason }` or `Blocked(MergeBlock)`. `MergeBlock`
(`git/merge/tree.rs`) is `OperatorOperation { marker }`, `TargetCheckedOutElsewhere { path }`, `TargetMoved`,
`UncommittedOverlap { paths }`, `FastForwardRefused { detail }`, `StashNotRestored { backup_ref }` or
`TargetHeld { target, accepted, observed }`. `MergeBlock` is stored on
the stage (`Stage.merge.block`, `#[serde(tag = "kind")]`) and has an operator-facing `Display`.
`loom run` refuses git older than 2.40 (`commands/run/git_preflight.rs`, called from
`run_startup_preflights`) because `merge-tree --write-tree` needs it. `git/runner.rs` gives
`merge-tree`, `commit-tree`, `update-ref` and `stash` the 120 s mutation timeout. Every runner command also sets
`GIT_NO_REPLACE_OBJECTS=1` and `GIT_GRAFT_FILE=/dev/null/loom-no-grafts` and passes `-c core.commitGraph=false`, so a
session-planted `refs/replace/*`, `info/grafts` or commit-graph cannot change the merge gate's diff or an ancestry
test ([Sandbox gaps](../concerns/sandbox-and-confinement-gaps.md)).

## Outcomes

`apply_auto_merge_outcome` maps each result:

| Outcome | Stage file afterwards | Daemon |
| --- | --- | --- |
| Success or `AlreadyUpToDate`, and `verify_merge_succeeded` proves ancestry | `Completed`, `merged: true`; `finalize_auto_merge` runs `finish_verified_merge`, which reconciles the base and removes worktree and branch; a backup ref is printed and logged | continues |
| Conflict | `record_merge_conflict` forces `MergeConflict`; nothing is spawned here | stays alive; the spawn loop starts a resolver in the stage worktree |
| Held (control path) | `NeedsHumanReview` with the gate's reason | exits; NEEDS REVIEW |
| Blocked (`MergeBlock`, including `TargetHeld`) | `record_merge_block` forces `MergeBlocked` with `Stage.merge.block` and `failure_info` carrying the block sentence as evidence, so `loom status` shows it | stays alive; the loop retries the merge (a `TargetHeld` block is never memoised, so the first retry after the hold clears lands it) |
| Git error (lock timeout, missing branch) | forced to `MergeBlocked`, `failure_info` type `InfrastructureError` with the git error as evidence | last stage: exits; `loom status` shows MERGE ERROR |
| Merge ran but ancestry cannot be verified | forced to `MergeBlocked`, reason in `failure_info` | same |
| Branch has zero commits beyond target | forced to `NeedsHumanReview`, `review_reason` says the agent never committed | exits; NEEDS REVIEW |
| `commit_merge` fails to sign (`CommitTreeError { signing: true }` anywhere in the chain) | `route_to_human_review` with the signer text and the remedy: fix signing, then `loom stage human-review <id> --approve`; reached from `land_stage_merge` (it returns `Landing::Held`) and from the first auto-merge (`apply_auto_merge_outcome`'s `Err` arm); no resolver spawns and nothing retries each tick | exits; NEEDS REVIEW |
| Auto-merge disabled for the stage or plan | stays `Completed + !merged` by design | exits; `loom status` shows unmerged |

`persist_merge_blocked` is the writer for the infrastructure outcomes and `route_to_human_review`
for the review outcome; both use `force_status_with_reason` because `Completed` has no legal exits.
Nothing on the daemon path writes `merged: true` without `is_ancestor_of` returning true. The
daemon's ancestry settles of a stage (the sweep and phantom-merge revert in `sync_graph_with_stage_files`, the
`NoWorktree` finalize in `merge_handler.rs`) tests against the guard's accepted tip through
`merged_into_accepted`, never the live ref: a ref any session could move would mark a stage merged without its work
landing; other consumers still read the live ref ([open list](target-guard.md#unimplemented-review-suggestions)). The auto-merge precheck records `TargetHeld` before the gate, the zero-commits route and the finalize.

## Recovery from every non-merged outcome

The spawn loop `spawn_merge_resolution_sessions` runs every tick. A `MergeBlocked` stage that has a
`merge.block` goes to `retry_blocked_merge` (`merge_handler/blocked_retry.rs`) and never gets a
resolver; every other `MergeConflict` or `MergeBlocked` stage goes to `spawn_resolver_if_due`, which first
tries `clean_merge_settled`. See
[merge-and-recovery](../patterns/merge-and-recovery.md#merge-resolver-spawn-loop).

The resolver works in the stage worktree `.worktrees/<id>`, never in R: it merges the target into
`loom/<id>`, resolves, reruns the stage's acceptance, commits, and runs `loom stage merge <id>
--resolved`. The relayed request reaches `resolve_merge_from_inbox`
(`orchestrator/core/inbox_drain/merge_resolved.rs`): status check, `check_resolved_worktree`
(`git/merge/resolved.rs`: worktree on `loom/<id>`, no `MERGE_HEAD`, no unmerged path, no tracked
change, the recorded `completed_commit` an ancestor of HEAD; untracked files are allowed because sandboxes leave stubs; pinned git's `config.worktree` check applies),
then `land_stage_merge` (`merge_handler/landing.rs`): `merge_stage` with `MergeGate::Enforce`, record. The reply mapping is the pure
`settle_for_landing` in `merge_resolved.rs`. A merged
result is applied with no cleanup, because the resolver still runs in the worktree; if the target
moved and the merge conflicts again, the request is refused with instructions and the stage stays
`MergeConflict`; a `Blocked` result is applied and the block recorded. When the resolver exits,
`handle_merge_session_completed` (`merge_handler/resolver_exit.rs`) cleans up a stage that is
`merged` and ancestry-proven, leaves a `MergeBlocked` stage with a block to the retry, and for a
`MergeConflict` stage runs `check_resolved_worktree`, `land_stage_merge`, and cleanup on success.
Otherwise the loop spawns the next counted resolver. `finalize_merge_resolution` keeps the
phantom-merge invariant (ancestry proof before `merged = true`).

`loom stage merge <id>` (`merge_retry`, run from the stage worktree) accepts `MergeConflict`,
`MergeBlocked`, and `Completed + !merged` (`commands/stage/merge/preflight.rs::require_merge_state`).
It re-runs `merge_stage` with `MergeGate::Enforce`, applies the daemon's zero-commit guard (`Stage::zero_commit_reason`), and requires `verify_or_derive_completed_commit` before `merged = true`. Success or `AlreadyUpToDate` complete the stage and clear the block; a `Held` result routes the stage to review;
Conflict moves the stage to `MergeConflict` and prints the manual route (in `.worktrees/<id>`: merge
the target, resolve, commit, `loom stage merge <id> --resolved`); Blocked records the block.
`loom stage merge --resolved` without a daemon relay (`merge_resolved`, `commands/stage/merge.rs` and
`merge/landing.rs`) takes the repo root from `WorkDir::main_project_root()`, never the cwd (the
worktree), runs `check_resolved_worktree` and `merge_stage` (gate enforced), and completes only after
`verify_or_derive_completed_commit`. The CLI's `record_block` refuses a merged stage, and `spawn_merge_resolver` refuses a typed-blocked stage. The progressive merge in `loom stage complete` and
`loom stage human-review --approve` turns `ProgressiveMergeResult::Blocked(MergeBlock)` into
`MergeBlocked` with the block. `commands/stage/merge/finish.rs` prints why cleanup did not finish:
refused, failed, or deferred.

Worktree cleanup (`merge_lifecycle::finish_verified_merge`) returns `CleanupOutcome::Deferred { reason }` when the
caller runs inside the worktree or any live session runs for the stage (`session_registry::live_sessions_for_stage`; a
failing scan defers too). The daemon sweep `sweep_merged_leftovers` (`merge_handler/leftover_sweep.rs`) runs each tick
after `drain_session_inboxes` and at startup, and finishes the cleanup of every `Completed` + `merged` non-knowledge
stage. It memoizes settled stages per daemon session and retries deferrals; a deferral lasting over 10 minutes records
a `cleanup_warning` once. The resolver's exit folds its permission approvals back from the worktree before cleanup; the
inbox sweep skips the fold-back quietly when the worktree is gone.

Worktree removal: the daemon's cleanup removes loom's known scaffold files and the empty stubs a
sandbox leaves at the worktree root (`verify/tool_artifacts.rs::NAMES`,
`git/cleanup/worktree.rs::remove_sandbox_stubs`), then runs a non-forced `git worktree remove`,
which refuses on any remaining modified or untracked file. `loom worktree remove <id>` checks
`git status` first (`git/cleanup/removal.rs::require_clean_worktree`) and refuses on modified
tracked files and on untracked files other than that scaffold and those stubs. Both delete ignored
files with the worktree, `target/` and a generated `REVIEW-PLAN-*.md` included, so move a review
into the main `doc/plans/` first. From a sandboxed session git cannot delete `.git/worktrees/<id>`
(`Device or resource busy`); run `git worktree prune` from an operator shell.

## The merge gate

`MergeGate::{Enforce, Bypass}` (`git/merge/control_paths.rs`) is a `merge_stage` argument. Under `MergeLock`, `Enforce` checks the exact `T0`/`B` merge-base diff and, after a clean `merge_tree`, the exact landing diff `T0..<merged tree>`. A control-path hit in either is `MergeResult::Held { reason }`; a diff error fails closed (`Err`). Paths come from `git diff -z` (no C-quoting) and match ASCII-case-insensitively: `.claude` and `.loom` exactly or as a directory prefix, `.mcp.json` exactly, and the tracked hooks directory exactly or below.

The daemon's `land_stage_merge` and auto-merge route `Held` to `NeedsHumanReview`. The early filters `auto_merge_precheck_blocks` and `gate_holds_merge_stage` (`merge_handler/merge_gate.rs`) stay fail-open pre-filters. Every CLI path (`loom stage merge`, local `--resolved`, the progressive merge in `loom stage complete`, the CLI resolver spawn) enforces the gate and routes a hold to review. Only `loom stage human-review --force-complete` passes `Bypass`, the operator's override; it is operator-only by the threat model, because a sandboxed stage agent cannot write the stage file that command changes first.

## Branch resolution

`branch_ref(name)` (`git/branch`) returns `refs/heads/<name>` unless the name already starts with `refs/`. `get_branch_head`, `commits_ahead_of` (raw revisions: `commits_between`) and `verify_merge_succeeded` resolve branch names through it, and `merge_stage` resolves `refs/heads/<target>` and `refs/heads/loom/<id>`. The worktree-removal guard (`git/cleanup/removal.rs`) resolves the stage and base branches through `branch_ref` too. A tag named like a branch (tags are shared by every worktree) would otherwise shadow the branch in proofs and cleanup guards. `current_branch` (`git/branch/operations.rs`) reads `git symbolic-ref -q HEAD` and strips `refs/heads/`, so such a tag does not turn the name into `heads/main`; a detached HEAD reads as `HEAD`. `loom init` refuses a detached HEAD and a branch with no commit (`checked_out_branch`, `commands/init/base_branch.rs`).

## The stage merge record

`Stage.merge: MergeRecord { block, stash, unrestored }` is `#[serde(flatten)]`, so the stage-file keys are `merge_block`, `merge_stash` and `merge_unrestored_stashes`. `merge.stash` is the stage's merge note and keeps the latest outcome: every success path that stashed writes it, and so does `block_merge(StashNotRestored)`. It is never cleared. `merge.unrestored: Vec<String>` gets every unrestored backup ref appended once and never loses one. Status's `stash_warning` lists every listed backup ref that still exists (each checked with `git rev-parse --verify --quiet`, refs only under `refs/loom/autostash/`), and the `STASH NOT RESTORED` note names all of them. An untyped merge block from the CLI's progressive merge (`mark_blocked` / `block_untyped`, `commands/stage/progressive_complete.rs`) records its reason as `failure_info` (InfrastructureError). Orphan recovery (`orchestrator/core/recovery.rs`) counts a failed commits-ahead probe as work and its `close_reason` says the count failed, with the error, instead of a number. `merge.block`, and the `failure_info` that `block_merge` wrote, is cleared whenever the stage leaves `MergeBlocked` (`try_transition`, `force_status_with_reason`) and by every untyped entry into `MergeBlocked`. `completed_commit` is recorded from the branch head whenever a stage enters `MergeConflict` without one.

## Merge Lock (git/merge/lock.rs)

`MergeLock` serializes loom-driven merges (the daemon's auto-merge, resolved landing and blocked retry, and the CLI merge paths) with an exclusive OS advisory lock (`fs2::try_lock_exclusive`) on the stable `.loom/work/merge.lock` inode. The file is created once and never unlinked; the holder's pid and timestamp are written into it for diagnosis only. `acquire` polls every 100 ms up to the caller's timeout (30 s from `merge_stage`). Release is by `Drop` or process exit, so there is no stale-lock reclamation and a pid left in the file after a merge is not a held lock.
