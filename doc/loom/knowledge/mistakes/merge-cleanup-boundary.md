# Merge Cleanup Boundary

>

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
specifically: _after this returns, what can no longer be verified?_

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

## Cleanup Refused Over Scaffold It Never Planted (2026-08-26)

**What happened:** Every non-forced worktree removal in this repo failed from
2026-08-09 (`bb15c919`) on. `remove_worktree_scaffold` (`git/cleanup/worktree.rs`)
runs ahead of `git worktree remove` and demanded that the worktree's root `CLAUDE.md`
be a symlink, bailing otherwise. This repo tracks `CLAUDE.md`, so every worktree checks
it out as a regular file; `setup_root_claude_md` (`git/worktree/settings.rs`) already
knew that and skipped planting the symlink — the removal side did not. Git was never
asked, and the `?` chain in `cleanup_after_merge` then skipped the branch and
base-branch deletion too: `.worktrees/<id>`, `loom/<id>` and `loom/_base/<id>` all
survived every merge. The daemon's deferred-cleanup branch
(`orchestrator/core/merge_handler.rs`, the `stage.merged` short circuit) discarded the
`CleanupOutcome`, so the only trace was a `tracing::warn!` on daemon stderr.

**Misleading signal:** the worktree WAS clean — empty `git status`, stage merged and
Completed. Nothing pointed at loom's own pre-check; the natural suspects were untracked
files (a worktree-local runtime dir, say), which cannot be it: the non-forced
`git worktree remove` runs `git status --porcelain` without `--ignored`, so ignored paths
never block it.

**Why:** the creation side and the removal side of the scaffold encoded different
assumptions. Creation was conditional ("plant a symlink only if the checkout has
none"); removal was unconditional ("it must be our symlink"). Tests covered the
worktree-local CLAUDE.md symlink case, never a regular tracked root `CLAUDE.md`. And
bailing on "unexpected content" in `.claude/` re-implemented git's cleanliness check in
front of git, but stricter and blind — no path in the error, no ignore semantics.

**Prevention:** when a pre-removal step mirrors a conditional creation step, it must
mirror the condition: remove only what you can prove you planted (`remove_if_symlink`),
never assert the shape of what you did not. Do not re-implement git's cleanliness policy
ahead of `git worktree remove` — let git refuse, and name the blocking paths from
`git status --porcelain` in the error. A daemon path that calls cleanup must print a
`Failed`/`Refused` outcome; "logged at warn" is silence in a detached daemon. A
regression test for cleanup must build a real worktree from a repo that tracks the
scaffold path.

**Fix:** `remove_worktree_scaffold` removes the root `CLAUDE.md` only when it is a
symlink; `remove_known_claude_scaffold` skips unknown `.claude/` entries (Claude Code's
`.cc-writes/` runtime dir included) and keeps a non-empty dir; `cleanup_worktree`
appends the blocking paths when git refuses; `try_auto_merge` prints failed/refused
deferred cleanups with the `loom worktree remove <id>` hint. And because the daemon's
console is `.loom/work/orchestrator.log` (fds 1 and 2 are `dup2`'d there in
`daemon/server/lifecycle.rs`, and the `LogLine` broadcaster has no client), printing is
not enough: `MergeLifecycle::cleanup` — the one door — now records a failed or refused
outcome as `Stage.cleanup_warning`, cleared by the next cleanup that succeeds, and
`loom status` (static graph, `--verbose` attention list, live TUI tree) renders it as
`cleanup failed` with the hint. Regression test:
`test_cleanup_worktree_succeeds_when_repo_tracks_root_claude_md` in
`git/cleanup/tests.rs`.

## Cleanup Refused Over Loom's Own Memory Spool (2026-08-27)

**What happened:** every worktree whose stage recorded a memory note survived its merge.
`git worktree remove` refused with "contains modified or untracked files", and the
`Blocking paths` line named `?? .loom/memory-spool.jsonl` — loom's own file.
`fs/memory/spool.rs` writes `<worktree>/.loom/memory-spool.jsonl` as the sandbox fallback
for `loom memory note` (`.loom/work` is a symlink outside the write boundary). Teardown drained
the spool but left the file, and nothing ignored or removed it.

**Why it was invisible here:** this repository's own `.gitignore` lists `.loom/cache/` and
`.loom/memory-spool.jsonl` by hand. No other project has those lines, so loom broke worktree
cleanup everywhere EXCEPT in its own checkout — the one place it gets exercised daily. The
failure was found in a different repository.

**Second defect, found while fixing the first — the per-worktree exclude never worked.**
`add_settings_local_to_worktree_gitignore` wrote to `<repo>/.git/worktrees/<stage-id>/info/exclude`,
with a doc comment asserting that acts as a per-worktree `.gitignore`. **Git resolves `info/`
to the COMMON git dir**, so that file is never read. Demonstrated directly:

```text
rule in .git/worktrees/wt/info/exclude  →  git status: ?? ignored.txt   (NOT ignored)
rule in .git/info/exclude               →  git status: (clean)          (ignored)
```

The function had therefore never done anything since it was written. `.claude/settings.local.json`
escaped notice only because most projects gitignore `.claude/` themselves.

**Prevention:**

- **A cleanup path must know about every file its own product plants.** Creation and removal
  drifted apart again, exactly as in the scaffold incident above. When adding a runtime file
  under a worktree, update `is_worktree_scaffold_path`, `remove_worktree_scaffold`, and the
  exclude list in the same change.
- **Never let the tool's own repo be the only test bed.** A hand-written `.gitignore` line in
  this checkout masked a universal breakage. When a fix depends on ignore rules, ask what a
  fresh project without them would do.
- **Assert git's BEHAVIOUR, not the file content.** A test that only asserted the exclude file
  contained the pattern passed for as long as the bug existed. The regression test now runs
  `git check-ignore` / `git status --porcelain` against a real worktree.

**Fix:** `remove_worktree_scaffold` deletes the drained spool when it is a regular file loom
planted (never a symlink, never a tracked path), then removes `.loom/` only if that leaves it
empty. The exclude writer targets the common `.git/info/exclude` and lists exactly
`.loom/memory-spool.jsonl` and `.loom/cache/` — **not** a blanket `.loom/`, which would hide a
committable `.loom/config.toml`.

## Related

- `mistakes/phantom-merges.md` — the parent failure class, including the original
  "wrote merged=true without verifying ancestry" incidents.
- `architecture/context-retrieval-state.md` — the overlay/base reconcile steps this
  ordering interleaves, and why base publication requires a clean tree.

## Merge failure policy missed a conflict-state transition

**What happened:** A shared merge failure writer refused to record infrastructure errors on a stage in `MergeConflict`, leaving manual retry in its old state.

**Why:** The state machine has no direct `MergeConflict -> MergeBlocked` transition; merge outcome writers deliberately force this transition after guarding ownership and proof.

**Prevention:** Compare existing outcome writers and state-machine edges before unifying their mutations, including repeat `MergeBlocked` and unmerged `Completed` cases.

**Fix:** The shared writer permits guarded merge-state outcomes and records the failure on the fresh stage.

## Distinct merge holds need distinct operator messages

**What happened:** Unifying signing and control-path holds replaced the control-path failure message with generic review text, breaking its existing operator contract.

**Why:** One coarse held outcome did not carry which remedy applies.

**Prevention:** Preserve a typed hold reason through adapters and test existing control-path output alongside signing failures.

**Fix:** Keep separate signing and control-path dispositions while sharing the durable review transition.

## Half-removed cleanup accepted uncertain entries and raced new files

**What happened:** A cleanup repair retained scan-then-remove behavior and treated some scaffold-shaped entries as owned without establishing their identity.
**Why:** The review checked existing files at inspection time but did not account for replacement or creation before recursive deletion.
**Prevention:** Require positive ownership proof and use removal operations that refuse unexpected entries at deletion time; do not treat names alone as proof.
**Fix:** Reassess half-removed recovery with deterministic adversarial interleavings and preserve uncertain entries.

## Empty directory replacement can fool half-removed recovery

**What happened:** Directory-only cleanup prevented late file loss but could remove a substituted empty directory and report success while the original was renamed away.
**Why:** unlinkat by name is not conditional on the inode held by an earlier directory descriptor.
**Prevention:** Treat name-based directory removal as a separate ownership boundary and do not claim exact identity without an atomic proof.
**Fix:** Reassess concurrent replacement and preserve or explicitly report any remaining limitation.

## A Merge Resolver Inherited the Stage's Write Denies and Gave Up Silently (2026-10-14)

**What happened**: a plan set `deny_write: ["doc/specs/**"]`. A stage's merge conflicted while main had changed `doc/specs/`. `session_sandbox` built the resolver's capsule from the stage's merged sandbox, so `git merge --no-commit --no-ff main` in the worktree failed with "Read-only file system". The resolver explained this in prose and ended its turn. Status kept saying "merge resolver ... is running (attempt 1 of 6)" because its pid was alive, the web node showed no terminal because `session_for_stage` described the ended worker `stage.session` still named, and `loom stage block` from a resolver was refused at three layers (relay matrix, `require_owner`, the `MergeBlocked -> Blocked` transition). When the operator finished the merge by hand, `loom stage commit` told them to run `git merge --abort`.

**Why**: the sandbox was built per stage with no session kind, though a resolver writes whatever the target changed. Resolver liveness was read as progress. The resolver had no path to report failure, and the status collector, the web schema and the CLI messages were each written for the stage worker only.

**Prevention**: a session kind that writes outside its stage's authorship (a resolver merging the target) gets its own sandbox rule plus an equivalent check at the point its output lands. Never let a released deny go unchecked. Every agent session needs a machine-readable give-up path that its signal names; prose at the prompt is invisible to loom. Status that reports "running" must come from activity, never from pid liveness alone. A refusal message names the path for every caller who can hit it, the operator included.

**Fix**: `release_worktree_write_denies` for `Merge` sessions plus the commit-time guarded-path check ([daemon-owned-commits](../architecture/daemon-owned-commits.md#what-the-daemon-refuses)); the give-up path ([merge-and-recovery](../patterns/merge-and-recovery.md#merge-resolver-spawn-loop)); `speaking_for` and staleness-based resolver notes ([status-data-model](../architecture/status-data-model.md#attention-rendering-static-view)); `merge_in_progress` in `git/stage_commit.rs` names the operator path (`git commit --no-edit`, then `loom stage merge <id> --resolved`).

**Review-round lessons**: the first sandbox fix dropped every deny, including absolute carve-outs, with no downstream check. The guarded-path check first missed trailing-slash globs, failed open on an unreadable plan snapshot, accepted `--ours` on a cleanly merged path (reverting the target after a fast-forward), and printed `git checkout --ours`, which is a no-op on a resolved index. Test every refusal by executing the command it prints.

## A Resolver Was Told To Pass Criteria It Could Not Run

**What happened:** the merge signal told a resolver that the acceptance criteria must pass before it commits, while the relay matrix refused `loom check` for `Merge` sessions. Inside the sandbox `cargo test` cannot pass (AF_UNIX denied, no nested bwrap, read-only `/tmp`), so the instruction could not be met. Separately, a merge kept the target's `INDEX.md` while auto-resolving knowledge files, and the landing refused it as stale although `loom knowledge check` had reported 0 issues.

**Why:** the signal text, the matrix and the gate were each written alone, and nothing tied an instruction in a signal to a capability of that session kind. The completion gate compared the index and `loom knowledge check` did not.

**Prevention:** when a signal tells a session kind to do something, trace the command through that kind's relay verdict and daemon admission. A gate and its diagnostic command share one predicate (`index::index_is_stale`).

**Fix:** a live resolver of the stage may relay `check` (`inbox_drain/apply.rs::require_merge_resolver`); step 3 of the signal syncs the index; `loom knowledge check` reports `StaleIndex`.

## The Integrity Gate Read a Formatter Reflow as a Changed Assertion (2026-11-07)

**What happened:** integration-verify's merge resolver merged feat-planner-ui, whose commit 5b3ec4a3 left `web/src/plan/feature/spine-model.test.ts` failing `oxfmt --check`. The resolver formatted it to pass that criterion; oxfmt wrapped one `expect(...)` across five lines and added a trailing comma. Every acceptance criterion passed, then the test-integrity gate raised `TI-edit-web/src/plan/feature/spine-model.test.ts` and the stage was handed back to a fresh stage session, re-running the stage.

**Why:** `lost_assertions` (`loom/src/verify/integrity/edits.rs`) counted a removed assertion as kept only when an identical trimmed line was added back. A reflow never yields an identical line, so the formatter gate and the integrity gate could not both pass.

**Prevention:** a diff-based gate over source lines must compare modulo formatting, or it fights the repository's own formatter check. Compare whole hunks, so a reflow that also changes a token still counts.

**Fix:** a hunk whose removed and added text are equal after `normalize` (whitespace dropped outside string literals; a comma before `)`/`]`/`}` dropped except for Python, where `(1,)` is a tuple) contributes nothing. Tests in `loom/src/verify/integrity/tests.rs` cover Rust and TypeScript reflows and reflows hiding a value or string change.
