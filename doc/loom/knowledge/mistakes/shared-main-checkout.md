# Shared Main Checkout

> Agents sharing the main checkout with other writers: stash and reset hazards

## A whole-tree git stash on main swept up another writer's edits

**What happened:** To prove a regression test failed without the fix, the agent ran a bare `git stash` on `main` while a loom plan was running and another process was editing files in the same checkout. The stash took that writer's uncommitted `loom/src/verify/tool_artifacts.rs` edits, and `git stash pop` then aborted because the writer had rewritten the file in the meantime.
**Why:** `git stash` without a pathspec captures every tracked change in the checkout, not only the agent's own, and the main checkout can be written by stages, sessions and the operator at any moment.
**Prevention:** On a shared checkout, never run a bare `git stash`. To test without a change, stash only your own files with `git stash push -- <path>...` (pathspec relative to the current directory), or copy the files aside and restore them.
**Fix:** The working-tree copy of the other writer's file was a superset of the stashed one, so it was left alone. The agent's two files were restored with `git checkout stash@{0} -- <files>` and the stash was dropped.
