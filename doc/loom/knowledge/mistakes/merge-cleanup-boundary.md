# Merge Cleanup Boundary

> Topic notes for the mistakes knowledge area.

## What Happened

`auto_merge::attempt_auto_merge` ran worktree-and-branch cleanup inside each of its
three success arms. So "merge" destroyed the evidence of the merge before its caller
could verify it — and the caller derives a missing `completed_commit` from exactly
the branch that cleanup had just deleted.

That is the phantom-merge failure class (`mistakes/phantom-merges.md`) reached from a
new direction: not by writing `merged: true` without checking git, but by removing
the ability to check at all. A caller that wanted to verify ancestry had nothing left
to verify against.

## Why It Survived So Long

A function named `attempt_auto_merge` doing cleanup is invisible from its call sites:
every caller reads as "merge the stage", and nothing in the name or signature
suggests the worktree is gone afterwards. Three separate success arms each did it, so
no single arm looked like the odd one out, and the tests passed because cleanup
genuinely succeeded — the defect was in the ORDER, not in any step.

**Detection rule:** when a function's name describes one action, list the
irreversible side effects it performs. If any of them destroys an input another
caller needs, the order is a latent defect regardless of whether tests pass. Ask
specifically: *after this returns, what can no longer be verified?*

## The Fix, and the Shape Worth Copying

`orchestrator/merge_lifecycle.rs` now owns the ordering structurally, and
`attempt_auto_merge` returns an **UNCLEANED** outcome:

```text
overlay reconcile -> merge attempt -> verify merged ancestry
  -> base reconcile & publish -> mark state and release dependents
  -> cleanup worktree/overlay
```

Two design choices carry the lesson:

- **One door.** `MergeLifecycle::cleanup` is the single path by which any merge
  caller may reach `cleanup_after_merge`. Every caller routes through it — the daemon
  auto-merge path, orphan worktree cleanup, manual `--resolved`, retry, and the
  already-merged short circuit — so **no caller can bury cleanup inside "merge"
  again**. Removing a side effect from a function is only durable if the side effect
  gains a single owner; otherwise the next caller re-adds it locally.
- **Cleanup refuses, rather than trusting.** It will not run unless the stage branch
  is provably contained in the target. A destructive step that cannot verify its own
  precondition must decline, not proceed optimistically.

Note what stayed with the callers and why: the merge attempt, the ancestry
verification and the state marking, because those need the orchestrator handle and
the stage-file lock. The lifecycle module owns only what sits AROUND them. Extracting
a lifecycle does not mean absorbing the lock-holding steps into it.

## Two Subsidiary Rules It Encodes

- **Cleanup still defers when the cwd is inside the worktree it would remove.**
  Deleting a live session's working directory breaks every hook that session fires
  afterwards — a class of failure that presents as unrelated hook errors, never as
  "your directory vanished".
- **A derived-state failure must not fail a good merge.** A reconcile failure marks
  derived state stale and leaves the merge intact, matching the existing rule that a
  cleanup failure must not turn a successful merge into an error. Generalisation: the
  durable result and the derived artifact have different failure budgets; never let
  the cheaper one veto the expensive one.

## Related

- `mistakes/phantom-merges.md` — the parent failure class, including the original
  "wrote merged=true without verifying ancestry" incidents.
- `architecture/context-retrieval.md` — the overlay/base reconcile steps this
  ordering interleaves, and why base publication requires a clean tree.
