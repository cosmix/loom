# Merge Checkout State

> Guarded fast-forward

## Merge Checkout State

When the target branch is checked out in the operator's main checkout R, `advance_in_checkout` (`git/merge/checkout_apply.rs`) advances it to the merge commit `M` without a merge in R. R's `HEAD` must equal `T0` (the target tip the merge was computed against); otherwise the result is `Blocked(TargetMoved)`. The merge commit is written lazily through `PendingMerge` (`git/merge/tree.rs`): a target checked out elsewhere, or any blocking classification, writes no commit object. Only the `Reapply` path commits early, for the dry run.

`classify` (`git/merge/checkout_state.rs`, pure) compares `git status --porcelain=v2 -z --untracked-files=all` with `git diff --name-status -z --no-renames T0 M`. A path is touched when T0..M modifies, adds or deletes it. Directory/file prefix overlaps count as overlaps; only an exact untracked path may be byte-compared and removed. Every `git status` in R runs with `GIT_OPTIONAL_LOCKS=0`.

| Case | Action |
| --- | --- |
| No tracked change and no untracked file on a touched path | `git merge --ff-only M`; git carries the local changes |
| Only overlap: untracked files M adds, byte-identical to the blob, regular files (a symlink never counts) | remove them, then fast-forward; a rollback restores them with their exec bit |
| Tracked overlap, dry run clean | reapply (below) |
| Tracked overlap with a predicted conflict, an untracked file that differs, or any unmerged index entry | `Blocked(UncommittedOverlap { paths })`; R untouched |

Status does not list ignored files, so `CheckoutProbe` (`checkout_files.rs`) looks on disk for local files at every path M adds and at their parent prefixes: an equal regular file is removed, anything else is `UncommittedOverlap`. This keeps `git merge --ff-only` from overwriting an ignored file the branch adds with `git add -f`.

The dry run creates `S = git stash create`, then runs `git merge-tree --write-tree --merge-base=T0 M S`; exit 0 is clean. A failing `stash create` or `stash push` is `UncommittedOverlap`.

Reapply: `update-ref refs/loom/autostash/<stage>-<unix-secs>-<stash sha12> S ""` records a backup ref (the empty old value means never overwrite), `git stash push` sets the work aside, `merge --ff-only M` advances, then `stash pop --index`, falling back to `stash pop`. Backup refs are kept after a successful reapply as a safety copy and are not pruned. `--autostash` is never used because it writes conflict markers into the checkout, and loom never runs `reset --hard`.

A refused fast-forward (or an error from it) is always a typed block: git's named paths give `UncommittedOverlap`, otherwise `FastForwardRefused { detail }`. `after_errored_fast_forward` (`git/merge/fast_forward.rs`) splits two cases. A clean refusal (git exited non-zero having changed nothing) pops the stash pushed for the reapply first, then classifies; when the pop fails, the result is `StashNotRestored { backup_ref }` and the target did not move. An errored fast-forward (spawn failure, or the runner's timeout killing git) does not pop: removed untracked files are written back only where absent, the stash entry stays, and the result is `StashNotRestored { backup_ref }` (`FastForwardRefused` when no stash was pushed). When the merge landed and both pops fail, the result is `Success { stash: Some(StashReapply { restored: false, .. }) }`, not a block; the stage's merge note records it (see [merge-flow](merge-flow.md#the-stage-merge-record)).

Operator operations include `sequencer/` as well as `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`, `rebase-merge/` and `rebase-apply/`; an unreadable git dir is an error. A worktree whose directory is gone does not count as checking out the target.

Blocked results are retried by the daemon (`retry_blocked_merge`, `merge_handler/blocked_retry.rs`). The retry skips an attempt when `git::merge::blocked_merge_inputs` (`git/merge/inputs.rs`) equals the inputs of the last attempt. The fingerprint hashes the target, branch and R `HEAD` ids, R's porcelain-v2 status, size and mtime of the paths it names, the block's watched paths, whether `index.lock` exists, `git worktree list`, and the operator marker. Only `UncommittedOverlap`, `TargetCheckedOutElsewhere` and `OperatorOperation` are memoized, and not when a watched path is a directory. A memo entry expires after 10 minutes. `FastForwardRefused` is retried at most once a minute. The memo lives in `Orchestrator.blocked_merge_inputs`. Retry memos (`blocked_merge_inputs`, `refused_merge_attempts`) are pruned at the start of each spawn-loop pass for stages no longer `MergeBlocked` with a typed block (`prune_retry_memos`, `merge_handler/blocked_retry.rs`). A blocked attempt writes a commit object only on the tracked-overlap reapply path, and the memo bounds repeats. A stage whose block is `StashNotRestored`, or whose `merge.unrestored` lists a backup ref under `refs/loom/autostash/` that still exists (`git rev-parse --verify --quiet`), is not retried at all: its operator changes sit only in that stash and ref, and the working tree may be half updated, so a retry would stash again under a new ref. Retries resume once the operator restores the changes and deletes the ref. `prune_retry_memos` keeps a memo when the stage file cannot be read.
