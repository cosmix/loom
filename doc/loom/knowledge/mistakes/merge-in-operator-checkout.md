# Merge In Operator Checkout

> Operator checkout git

## Merges Ran Git in the Operator's Checkout (2026-10-02)

**What happened**: `merge_stage` checked out the target and ran `git merge --no-ff` in the operator's main checkout, the conflict probe did a checkout and `merge --no-commit` there, and the resolver worked there. Merge resolvers in another project failed on 2026-10-01 with `unable to unlink old 'vitest.config.ts': Device or resource busy`, and one aborted merge left 11 untracked branch files in the checkout. Merges also blocked on uncommitted files the merge never touched. Earlier incidents of the same cause: [an aborted sandboxed merge leaves untracked files](sandbox-tooling-and-network.md#a-sandboxed-git-merge-that-aborts-still-leaves-the-branchs-new-files-untracked-2026-09-10), [a bind mount breaks git merge and git stash](sandbox-tooling-and-network.md#a-sandbox-bind-mount-makes-git-merge-and-git-stash-fail-in-an-interactive-session-2026-09-13), [merge with merge-tree and commit-tree](phantom-merges.md#git-merge-fails-on-sandbox-bind-mounted-files-merge-with-merge-tree-and-commit-tree-2026-09-19).

**Why**: the merge used the operator's working tree as scratch space, so every sandbox artefact, dirty file or interruption there became a merge failure.

**Prevention**: no agent runs git against the main checkout. The merge is computed with `merge-tree` and `commit-tree`, and the main checkout is touched only by a guarded fast-forward ([merge-flow](../architecture/merge-flow.md), [merge-checkout-state](../architecture/merge-checkout-state.md)).

**Fix**: `merge_stage` rewritten around `merge-tree`/`commit-tree`/`advance_target`, the resolver moved to the stage worktree, blocked merges retried by the daemon (commits f42b4eb0, 0f2d65a3, 5a91759d, 1bd5804b, a731e3c2).

## Plan Errors Found While Implementing the Merge-Off-Checkout Plan (2026-10-02)

**What happened**: six defects in `PLAN-merge-off-main-checkout`.

1. The integration scenario said an overlapping uncommitted edit "merges on the tick after the edit is committed". Committing an edit the dry run predicted to conflict turns the next attempt into a `Conflict` (same lines, same three-way result). The test sets the edit aside with `git stash` instead.
2. W3a and W3b were declared parallel with disjoint files, but W3a changed `generate_merge_signal` and `spawn_merge_resolver` signatures that W3b's files call, so they ran in sequence.
3. The per-tick retry of a blocked merge wrote a new commit object (and stash commits) every tick before discovering the block. The input fingerprint (`blocked_merge_inputs`) fixed it.
4. The plan's "report the backup ref in the stage's merge note" names a field that does not exist.
5. `git merge-tree` with a bad revision exits 1 with empty stdout, not 128.
6. A web fixture (`web/src/api/fixtures/snapshot.json`) pinned the old cap text "attempt 1 of 3" and was not in the plan's file list.

**Why**: the plan was written from the intended behaviour without checking each step against the merge algebra, the callers of changed signatures, or what a per-tick retry writes.

**Prevention**: when a plan pins an integration scenario, check that its steps are consistent with the merge algebra it relies on. When units are declared parallel, check the signatures each one changes against the callers in the other's files. When a design retries every tick, ask what each attempt writes. Search fixtures and snapshots for text a change alters.

**Fix**: each item corrected in the implementation as listed.

## The Control-Path Gate Read Quoted Paths (2026-10-02)

**What happened**: the merge gate read `git diff --name-only` without `-z`, which C-quotes non-ASCII and special paths into a quoted octal-escape form. `starts_with(".claude/")` failed on them, so such a branch merged unreviewed. Exact names (`.mcp.json`) and case variants were missed too, the gate ran outside the merge lock (a race with the branch tip), and it failed open on a git error.

**Why**: the gate parsed display output meant for people and checked a commit other than the one later merged.

**Prevention**: read git path lists with `-z`; gate the exact commit being merged under the same lock; fail closed on any error.

**Fix**: `git/merge/control_paths.rs`, with the gate inside `merge_stage` (`MergeGate`).

## A Fast-Forward Silently Overwrote an Ignored Local File (2026-10-02)

**What happened**: `git merge --ff-only` replaces an operator's ignored file at a path the branch adds with `git add -f`, and `git status` does not list ignored files, so the overlap classification saw nothing.

**Why**: classification trusted status as the full picture of local files.

**Prevention**: check the disk for every path the merge adds, and for its parent prefixes.

**Fix**: `CheckoutProbe` (`git/merge/checkout_files.rs`).

## A Tag Named Like a Branch Faked Merge Proofs (2026-10-02)

**What happened**: bare branch names in `merge-base --is-ancestor`, `rev-list` and `rev-parse` resolve tags before branches. A tag named like `loom/<id>` or the target, which every worktree shares, could satisfy a merge proof or a cleanup guard.

**Why**: branch names were passed to git unqualified.

**Prevention**: qualify branch names as `refs/heads/<name>` wherever a proof or guard resolves one. When introducing a ref-resolution rule, grep every git invocation that takes a branch name.

**Fix**: `branch_ref` in `git/branch`, used by `get_branch_head`, `commits_ahead_of`, `verify_merge_succeeded`, the ancestry callers and `merge_stage`. A later review found the removal guard (`git/cleanup/removal.rs`) and `current_branch` still resolving bare names; both now go through `branch_ref` / `symbolic-ref`.

## CLI Merge Paths Wrote merged = true Without Proof (2026-10-02)

**What happened**: the progressive merge and `loom stage merge` trusted `AlreadyUpToDate` with no ancestry check and no zero-commit guard, the two protections the daemon path already had.

**Why**: the guards were written into the daemon path only, so the CLI paths stayed on an older contract.

**Prevention**: every writer of `merged = true` goes through the same proof (`verify_or_derive_completed_commit`) and guard (`Stage::zero_commit_reason`).

**Fix**: one shared guard text and the proof on every CLI merge path.
