# Merge Flow

>

## Merge Flow

How a completed worktree stage reaches the target branch, in the order the daemon runs it, and
what every outcome leaves on disk. Knowledge stages are outside this flow: they commit to the
base directly and `complete_knowledge_stage` sets `merged: true` with no branch.

## Where the first merge attempt actually happens

Graph synchronization and the StageCompleted monitor handler both reach `try_auto_merge` (`orchestrator/core/merge_handler/auto_merge_dispatch.rs`). An already merged stage proceeds to cleanup. Eligible pending merges check automatic-merge policy, target/control-path/commit-ahead preconditions and persisted completion proof before overlay reconciliation and `attempt_auto_merge`.

`completed_commit` is the verified input revision. Missing proof routes to review with a request to rerun completion. `attempt_auto_merge` calls `git/merge/mod.rs::merge_stage`, whose lock protects a final branch-tip comparison against that proof. The auto-merge outcome handler persists success, conflicts or blocks.

## `merge_stage` merges off the operator's checkout

`merge_stage(stage_id, target, repo_root, work_dir, gate)` runs under `MergeLock`. It never checks out a
branch, runs a three-way merge, or creates `MERGE_HEAD` in the operator's main checkout R. Only when
the target is checked out in R does it change R's files: `git merge --ff-only`, plus the stash, the
`reset --hard` and the untracked-file removal of the reapply path:

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
`UncommittedOverlap { paths }`, `FastForwardRefused { detail }`, `StashNotRestored { backup_ref }`,
`StashResetRefused { backup_ref }`, `CheckoutRestoreIncomplete { source_rev, failures, changes }` (`changes` is a
`RollbackChanges`, `git/merge/stash_report.rs`) or `TargetHeld { target, accepted, observed }`. `MergeBlock` is stored on
the stage (`Stage.merge.block`, `#[serde(tag = "kind")]`) and has an operator-facing `Display`.
`loom run` refuses git older than 2.40 (`commands/run/git_preflight.rs`, called from
`run_startup_preflights`) because `merge-tree --write-tree` needs it. `git/runner.rs` gives
`merge-tree`, `commit-tree`, `update-ref` and `stash` the 120 s mutation timeout. Every runner command also sets
`GIT_NO_REPLACE_OBJECTS=1` and `GIT_GRAFT_FILE=/dev/null/loom-no-grafts` and passes `-c core.commitGraph=false`, so a
session-planted `refs/replace/*`, `info/grafts` or commit-graph cannot change the merge gate's diff or an ancestry
test ([Sandbox gaps](../concerns/sandbox-and-confinement-gaps.md)).

## Outcomes

`stage_service::merge_outcome` classifies Git, daemon and progressive results into landed, already landed, conflict, typed block, review hold, and the distinct missing-branch or missing-worktree cases. Signing failures in a `CommitTreeError` chain route to `NeedsHumanReview` with the signing remedy; other execution errors record `InfrastructureError` in `MergeBlocked`. Manual retry returns an error after persisting an execution failure. These adapters guard outcome writes against changes to the captured status, session, attempt start and completion proof; reviewed landing keeps its own authority and entry step.

A landed or already-landed result requires original completion-commit ancestry before `merged: true`. Finalization rechecks the proof and current stage before applying its delta, then uses `finish_verified_merge` for deferred-safe cleanup and base reconciliation. A missing branch cannot prove a merge; a missing worktree is finalized only with ancestry proof. Manual retry preserves a completed, unmerged stage when auto-merge was disabled and a conflict occurs.

The daemon spawn loop handles conflicts in the stage worktree and retries typed blocks; `TargetHeld` is rechecked each tick. The auto-merge precheck records `TargetHeld` before the gate, zero-commit route and finalization. A zero-commit branch routes to review. Auto-merge disabled leaves the stage `Completed + !merged`.

The daemon ancestry settles of a stage (the sweep in `sync_graph_with_stage_files`, which settles only Completed stages not yet merged, and `NoWorktree` finalization) use the guard accepted tip through `merged_into_accepted`, never the live ref; other consumers still read the live ref ([open list](target-guard.md#unimplemented-review-suggestions)).

## Recovery from every non-merged outcome

The resolver works in `.worktrees/<id>` and commits the target merge on `loom/<id>`. `loom stage merge <id> --resolved` reaches the authenticated `MergeResolved` handler in `orchestrator/core/inbox_drain/merge_resolved.rs`, or the local CLI resolved-merge path. Both call `commands/stage/complete_resolved.rs::verify_resolved_completion` before landing. Ancestry and cleanliness alone do not authorize a landing: resolved content must pass fresh completion gates.

The shared verifier checks the registered branch/worktree and original commit ancestry, captures content and effective-policy snapshots, runs acceptance and the complete verification pipeline, then validates the snapshot against current state under the stage lock before updating `completed_commit`. The merge lock compares the landing tip to that verified commit. The merge session's authorization stays separate from the completion host job. Resolver verification runs synchronously in its trusted caller.

A clean resolved worktree can also use `loom stage complete <id>` to run the ordinary operator pipeline. Resolver exit uses the shared verifier before automatic landing. Failed verification retains the worktree and original proof and routes an exited resolver to review with completion guidance; relay failure is refused. A live resolver retains its worktree after a successful relay until exit cleanup. Already landed content requires ancestry proof before cleanup.

The spawn loop sends typed `MergeBlocked` records to `retry_blocked_merge`. A retry that hits a permanent failure (the stage branch deleted, or moved after verification; `permanent_merge_failure`) routes the stage to review, and any other retry error keeps the typed block and records the failure once (`apply_retry_failure`). Other conflict/blocked records use `spawn_resolver_if_due`, which first checks whether the merge settled. Target holds, control-path violations, ownership, uncommitted changes and original-commit ancestry remain independent merge guards. Missing automatic completion proof routes to review instead of deriving authority from HEAD.

`loom stage merge <id>` retries allowed merge states through `merge_stage` with the gate enforced. Successful outcomes persist merge evidence; conflicts and typed blocks retain their recovery paths. The progressive completion and human-review paths preserve typed merge blocks. See [merge-and-recovery](../patterns/merge-and-recovery.md#merge-resolver-spawn-loop).

`merge_lifecycle::finish_verified_merge` defers worktree cleanup while the caller or any live session uses it, including an indeterminate session scan. The daemon leftover sweep retries completed merged stages at startup and each tick, recording a cleanup warning after prolonged deferral. Resolver permissions fold back before deletion. Cleanup removes only recognized scaffold files and sandbox stubs, then uses non-forced worktree removal; other modified or untracked files prevent deletion. Ignored build outputs are removed with the worktree. Operator-side pruning may be required when sandbox permissions prevent Git metadata removal.

## The merge gate

`MergeGate::{Enforce, Bypass}` (`git/merge/control_paths.rs`) is a `merge_stage` argument. Under `MergeLock`, `Enforce` checks the exact `T0`/`B` merge-base diff and, after a clean `merge_tree`, the exact landing diff `T0..<merged tree>`. A control-path hit in either is `MergeResult::Held { reason }`; a diff error fails closed (`Err`). Paths come from `git diff -z` (no C-quoting) and match ASCII-case-insensitively: `.claude` and `.loom` exactly or as a directory prefix, `.mcp.json` exactly, and the tracked hooks directory exactly or below.

The daemon's `land_stage_merge` and auto-merge route `Held` to `NeedsHumanReview`. The early filters `auto_merge_precheck_blocks` and `gate_holds_merge_stage` (`merge_handler/merge_gate.rs`) stay fail-open pre-filters. Every CLI path (`loom stage merge`, local `--resolved`, the progressive merge in `loom stage complete`, the CLI resolver spawn) enforces the gate and routes a hold to review. Only `loom stage human-review --force-complete` passes `Bypass`, the operator's override; it is operator-only by the threat model, because a sandboxed stage agent cannot write the stage file that command changes first.

## Branch resolution

`branch_ref(name)` (`git/branch`) returns `refs/heads/<name>` unless the name already starts with `refs/`. `get_branch_head`, `commits_ahead_of` (raw revisions: `commits_between`) and `verify_merge_succeeded` resolve branch names through it, and `merge_stage` resolves `refs/heads/<target>` and `refs/heads/loom/<id>`. The worktree-removal guard (`git/cleanup/removal.rs`) resolves the stage and base branches through `branch_ref` too. A tag named like a branch (tags are shared by every worktree) would otherwise shadow the branch in proofs and cleanup guards. `current_branch` (`git/branch/operations.rs`) reads `git symbolic-ref -q HEAD` and strips `refs/heads/`, so such a tag does not turn the name into `heads/main`; a detached HEAD reads as `HEAD`. `loom init` refuses a detached HEAD and a branch with no commit (`checked_out_branch`, `commands/init/base_branch.rs`).

## The stage merge record

`Stage.merge: MergeRecord { block, stash, unrestored }` is flattened into the stage-file keys `merge_block`, `merge_stash` and `merge_unrestored_stashes`. The latest stash note survives later outcomes; unrestored backup refs are retained once each and the status surface reports those that still exist.

Typed block state and its failure information clear when the stage leaves MergeBlocked. Untyped progressive-completion failures retain their infrastructure diagnosis, and recovery preserves failed commit-ahead probes as work with the actual error in its reason.

Conflict entry preserves the existing `completed_commit` and never records a missing proof from branch HEAD. Automatic merge, blocked retry, restart recovery, resolver-exit ancestry checks and locked landing finalization must consume persisted completion proof rather than create it. A disappearing or changing proof fails locked finalization. Explicit authenticated operator skip/unsafe commands remain separate intentional overrides; their commit pin is not evidence that tests ran.

## Merge Lock (git/merge/lock.rs)

`MergeLock` serializes loom-driven merges (the daemon's auto-merge, resolved landing and blocked retry, and the CLI merge paths) with an exclusive OS advisory lock (`fs2::try_lock_exclusive`) on the stable `.loom/work/merge.lock` inode. The file is created once and never unlinked; the holder's pid and timestamp are written into it for diagnosis only. `acquire` polls every 100 ms up to the caller's timeout (30 s from `merge_stage`). Release is by `Drop` or process exit, so there is no stale-lock reclamation and a pid left in the file after a merge is not a held lock.

## Completion generated-output policy

Completion validates committed tracked content and rejects untracked inputs, except loom's own state (tool-artifact mount placeholders, `.claude/settings*`, spools, `.loom/cache/`, scaffold symlinks) and untracked output that the committed `.gitignore` files ignore (`src/handoff/completion/generated.rs`). No ecosystem directory list exists: a project states its generated output (`target/`, `_build/`, `build/`, `__pycache__/`, `node_modules/`) in the ignore rules it commits. `.git/info/exclude` and `core.excludesFile` are never honoured; an untracked `.gitignore` that git reads is itself a refused input, and tracked `.gitignore` edits fail the tracked-diff check. Knowledge stages honour no ignore rules. A stage declaration that names an ignored path (the path, or a directory above it holding no tracked file, after trailing wildcard-only components are dropped) makes it a required input to commit with `git add -f`; a broad declaration such as `packages/**` over tracked sources does not. Verification snapshots and merge landing remain bound to the exact verified commit.

## Completion precedes automatic merge

`loom stage complete` in a session relays a `complete` request, and the daemon completes the stage only from a passing host job (`host_verify/apply.rs::bind_and_complete`). It records the verified `completed_commit`, and transitions the stage to Completed with `merged: false` for non-knowledge stages; the merge flow lands it afterwards. Missing proof cannot be filled from a newer branch tip to authorize landing. See [Host Verification Jobs](verification-v2-gates.md).

Operator progressive completion performs its merge before the final Completed transition (`commands/stage/progressive_complete/finish.rs`). The host-job completion does not.

## A Resolver's Committed Content That Fails Completion

A merge resolver cannot repair content its own merge commit made fail `verify_resolved_completion`: a Merge-scope commit needs the merge in progress, which the merge commit used up, and a Merge stage cannot dispute (`MergeConflict` reaches only `Completed` and `Blocked`, `models/stage/transitions.rs`). `merge_handler/resolver_handback.rs` therefore stops every resolver of the stage and requeues the stage so a fresh Stage session gets the refusal text (up to `MAX_MERGE_HANDBACKS` = 2; an approval zeroes the count; the refusal is cut at `MAX_REFUSAL_BYTES`). `MergeConflict` to `Queued` stays illegal: `Stage::try_hand_back_merge` (`models/stage/merge_block.rs`) steps through `Blocked`. The stage is held for the operator (`NeedsHumanReview`) instead when a resolver may still run, a live worker session exists, the stage used its handbacks, or the merge commit since `completed_commit` is missing. Every `Err` of `verify_resolved_completion` counts as a failure, transient ones included, on purpose: the fresh session verifies from scratch and the cap holds a repeating error. Past mistakes: [Planner UI Repair](../mistakes/planner-ui-repair.md#a-merge-resolver-could-not-land-a-merge-that-edited-an-accepted-test-file).

## Post-completion plan commits

The DONE rename is committed from an isolated index rooted at the observed target HEAD. The merge lock covers preparation, signed commit creation, compare-and-swap ref movement, index synchronization for the two owned plan paths, and exact-commit attestation. Other staged and unstaged paths remain operator-owned.

## Legacy worktree cleanup API

`fs::worktree_files` remains public for compatibility; its legacy cleanup operations and types are deprecated because they omit stage retirement duties including liveness, final relay drain, settings and memory handling. Session listing uses `fs::session_files` directly; the canonical session and stage lookup re-exports remain available. The legacy option to clean signals only applies when session cleanup is enabled; disabling stage archiving leaves the stage file in place.
