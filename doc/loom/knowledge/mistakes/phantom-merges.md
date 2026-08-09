# Phantom Merges

> Six lessons on writing merged=true without verifying git ancestry — the costliest recurring failure class in loom.

## Phantom Merges: merged=true Without Verification

**Mistake:** `try_auto_merge()` set `merged=true` without verifying the commit was in target branch history. Merge verification errors fell through to `merged=true` fallback. Agents also edited `.work/` files directly.
**Fix:** Use `is_ancestor_of()` to verify merge before setting `merged=true`. Treat verification errors as `MergeBlocked`. Never edit `.work/` files directly.

## Phantom Merges from Defensive "Assume Merged" Branches (2026-04-15)

**What happened:** Seven daemon-side code paths wrote `merged: true` to escape an earlier respawn-loop bug without verifying git ancestry. A user lost real work: stage `oauth-hardening` was marked merged but its commits stayed stranded on `loom/oauth-hardening`; a downstream stage then worktreed off main and produced overlapping, incomplete code. Smoking gun log: `Completed stage has no completed_commit, assuming merged stage_id=integration-verify`.

**Misleading signal:** The original respawn-loop bug (commit `1af9827`, see `doc/merge-resolve-bug-notes.md`) was patched by force-writing `merged=true` when stage was already Completed — the rationale "stage's work is done, don't revert to MergeBlocked" looked defensible. Similarly, seven separate sites used "assume already merged" / "legacy stage" / "avoid stuck-in-MergeBlocked loops" as justification for lying about merge state.

**Why it broke:** `merged=true` is a contract with the dependency scheduler. Dependents satisfy their deps by reading `dep.merged`. Lying about it silently propagates broken state across the DAG: dependents spawn with a wrong base branch, their commits overlap partially with the unmerged dep, progressive merge fails downstream.

**Prevention — INVARIANT:** **Daemon-side automated paths MUST NEVER write `merged: true` without git ancestry verification (`is_ancestor_of` returning `Ok(true)`).** The only exemptions are explicit user intent: `loom stage complete --force-unsafe --assume-merged`, `loom stage merge --resolved`, knowledge stages (no branch by design), and `loom worktree remove` cleanup.

**Detection rules for future work:**

- Any `stage.merged = true` write outside the exemption list is a phantom-merge candidate. Must be preceded by a git-verified `is_ancestor_of(completed_commit, target_branch)` returning `Ok(true)`.
- "Stage is Completed (terminal), can't go back" is NOT a license to write merged=true. `Completed + !merged` is a valid resting place — `spawn_merge_resolution_sessions` only acts on `MergeConflict`/`MergeBlocked`, so no respawn loop results.
- Dependency scheduling must cross-check ancestry (`are_all_dependencies_satisfied` in `verify/transitions/state.rs`), not trust the `merged` flag alone. Knowledge stages are the only exemption.
- `loom repair` catches stages with `merged: true` whose commit is not in the target branch — run on suspected phantom merges.

**Fix (implemented in this change):** Seven writer sites (recovery.rs, merge_handler.rs × 5, progressive_complete.rs) now leave `Completed + !merged` as the resting state instead of lying. `check_merge_state` returns `Unknown` for non-knowledge stages whose merged flag can't be ancestry-verified. `are_all_dependencies_satisfied` cross-checks ancestry per dep. `start_stage` adds a spawn-time defense-in-depth check. A one-shot retry on daemon start handles the `--no-verify`-then-restart case. `loom repair` detects and reverts phantom merges. Status UI renders `Completed + !merged` as yellow "unmerged" with a hint to run `loom stage merge <id>`.

## Phantom Merges from `--force-unsafe` Shortcuts (2026-04-27)

**What happened:** `loom stage complete --no-verify --force-unsafe --assume-merged` (and a related `--force-unsafe` alone path) wrote `merged: true` without ever verifying git ancestry. Three concrete failure modes:

1. **Phantom merge via `--assume-merged`.** `complete.rs::handle_force_unsafe_completion` set `merged = true` regardless of git reality, re-introducing the phantom-merge class via a user shortcut.
2. **Stuck `Completed + !merged` with active merge.** With `--force-unsafe` alone after a previous resolver session died mid-merge (`.git/MERGE_HEAD` set), the daemon retry called `merge_stage`, which failed; the next resolver ran `get_conflicting_files_from_status`, which destructively `git merge --abort`ed the existing active merge.
3. **`loom stage complete` on a `MergeConflict` stage.** Ran the full acceptance + goal-backward + progressive-merge pipeline, none of which is the resolver's job.

**Misleading signal:** Both `--force-unsafe` shortcuts looked defensible because they were "explicit user intent". But `--force-unsafe --assume-merged` made `merged: true` a contract violation: the dependency scheduler reads `dep.merged` and queues dependents as if the work landed. Cross-references the existing 2026-04-15 `Phantom Merges from Defensive "Assume Merged" Branches` entry — this is the user-shortcut variant of the same class.

**Why it broke:** Three preconditions all had to be wrong simultaneously: (a) no attribution check tied `MERGE_HEAD` to a specific stage, (b) `--assume-merged` skipped ancestry verification, (c) helpers that mutate git state (`merge_stage`, `get_conflicting_files_from_status`) had no guard against running over an in-progress merge. Together they made the active merge invisible to recovery.

**Prevention — Routing-and-Attribution INVARIANT:** _An active merge on disk may block or guide recovery, but it must not mutate a stage unless loom can attribute that merge to that stage._

- `MERGE_HEAD` in the main repo is global. Every state-machine mutation triggered by detection must come with proof of attribution: orphaned `SessionType::Merge` metadata, `MERGE_HEAD` commit matching `loom/<stage-id>` HEAD, or `completed_commit` match. Without attribution, refuse — never mutate.
- `--force-unsafe --assume-merged` must verify ancestry via `verify_merge_succeeded` before writing `merged=true`.
- `--force-unsafe` alone must refuse if an attributed active merge exists for THIS stage (would orphan MERGE_HEAD).
- Routing must be a pure read-only function (`route_complete_for_conflicts`) — persistence happens only on the success path so refusal preserves stage state.

**Fix (this change):**

- New module `git/merge/in_progress.rs` is the single source of truth for `MERGE_HEAD` detection.
- New module `orchestrator/merge_attribution.rs` ties active merges to specific stages via session metadata, branch HEAD, or `completed_commit`.
- `route_complete_for_conflicts` (in `commands/stage/complete.rs`) is the new pure routing seam — read-only, never mutates.
- `merge_verify::verify_or_derive_completed_commit` shared helper enforces ancestry for `--assume-merged` and `loom stage merge --resolved`.
- Daemon recovery runs `reconcile_main_repo_active_merge` BEFORE `sync_graph_with_stage_files` and BEFORE `recover_orphaned_sessions` so attribution sees session metadata before recovery deletes it.
- `sync_graph_with_stage_files` re-verifies `Completed + merged=true` non-knowledge stages, deriving from branch HEAD when missing and reverting `merged=false` when unverifiable.

## Helpers That Abort Active Merges (2026-04-27)

**What happened:** `merge_stage` and `get_conflicting_files_from_status` both ran `git merge --abort` on the repo as part of their normal flow (cleanup after success, abort the test merge). When invoked while a real merge was already in progress, they destroyed the user's resolution work.

**Misleading signal:** Both helpers acquire `MergeLock` at entry, so concurrent loom-driven merges are serialized. The bug is not concurrency — it's that the helpers don't distinguish "no merge in progress" from "a merge IS in progress that I didn't start".

**Prevention:** Helpers that mutate git merge state MUST refuse with `require_no_active_merge` when `MERGE_HEAD` is set on the repo path they're running in. Never silently `git merge --abort`. Defense in depth: even if attribution misses an active merge upstream, the guard surfaces an error instead of corrupting state.

**Fix:** Added `require_no_active_merge(repo_root)` helper in `git/merge/mod.rs`; called from `merge_stage` and `get_conflicting_files_from_status` after acquiring the merge lock. Both bail with a distinct error pointing at the path where the merge is in progress.

## Merge Handler: Inline Branch Names

**Mistake:** 8 instances of `format!("loom/{}")` instead of `branch_name_for_stage()`.
**Fix:** Always use `branch_name_for_stage()` for branch name construction.

## Merge Conflict Session Lifecycle: Original Session Continued Running (2026-04-16)

**What happened:** When `loom stage complete` detected a merge conflict during progressive merge, the original execution session continued running instead of exiting. Three coordinated issues prevented clean handoff to the resolution session:

1. `complete_with_merge()` returned `Ok(false)` on merge conflict, which propagated back to `complete.rs:623` without error — the session stayed alive
2. `commit-guard.sh` (Stop hook) set `stage_incomplete=1` for `MergeConflict` status, blocking the session from exiting even if it tried
3. `spawn_merge_resolution_sessions()` didn't kill the stale original session, leaving a zombie process that blocked merge resolver spawning

**Why:** The `Ok(false)` return was designed for "merge didn't succeed but keep running" — wrong mental model. Merge conflict means "your work is done, hand off to resolver." The commit-guard didn't distinguish between "stage still executing" and "stage waiting for merge resolution." And session cleanup assumed sessions would exit on their own.

**Prevention:**

- When adding new terminal/handoff stage statuses, always update: (1) `complete_with_merge` return behavior, (2) `commit-guard.sh` case statement, (3) `detection.rs` normal-exit matches, (4) `spawn_merge_resolution_sessions` cleanup logic
- Use `bail\!()` not `Ok(false)` when the session MUST exit — `Ok(false)` leaves the caller alive
- Test the full lifecycle: stage completes → merge conflicts → original session exits → resolver spawns → resolver resolves

**Fix:** Four-part coordinated change:

- `progressive_complete.rs`: Changed `Ok(false)` to `bail\!()` for Conflict and Blocked arms, forcing session exit with clear message
- `commit-guard.sh`: Changed MergeConflict case to allow session exit (no longer sets stage_incomplete)
- `merge_handler.rs`: Added `kill_session()` call for stale Stage sessions before spawning merge resolver
- `merge.rs`: Added "Inherited Responsibilities" section to merge signal explaining resolver owns the stage
