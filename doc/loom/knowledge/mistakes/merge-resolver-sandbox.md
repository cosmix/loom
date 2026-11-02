# Merge Resolver Sandbox

>

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
