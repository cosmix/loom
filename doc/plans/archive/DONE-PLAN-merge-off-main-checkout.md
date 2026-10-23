# Plan: Merge Without Agents Touching the Main Checkout

A direct implementation plan, not a loom plan: run it in an ordinary interactive Claude Code session
at the repository root. Every `path:line` below was checked against the tree on 2026-10-02; anchor
edits by symbol, since lines drift.

## For the implementing session

- Load the `loom-orchestration` skill before spawning. You orchestrate: subagents implement, you
  verify. Units, owners and order are in "Work units"; spawn each unit as one subagent with its
  section quoted in full as the brief.
- Precondition: the in-tree `allow_write` fix is in the tree.
  `rg -n 'fn is_inside_cwd' loom/src/sandbox/grant_paths.rs` must print one line.
- Another change may have touched `loom/src/git/cleanup/*` or `orchestrator/merge_lifecycle.rs`
  (a worktree-cleanup fix ran alongside this plan). Re-read those by symbol before W3b edits callers.
- Gate after every unit, run by you: `cd loom && cargo fmt --check && cargo clippy --all-targets
  -- -D warnings && cargo test`. `tests/maintainability.rs` ratchets file and function sizes
  against `loom/maintainability-baseline.txt`: a listed file may not grow; lower its entry when it
  shrinks; a new file stays under 400 lines and every function under 50.
- Commit only if the operator asked for commits. Groups: one per work unit, Conventional Commits,
  no attribution trailers.

## Problem

Two merge resolvers in a project using loom failed on 2026-10-01 with
`unable to unlink old 'vitest.config.ts': Device or resource busy`, and one aborted merge left 11
untracked branch files in the main checkout. The bind mount behind that error is fixed separately
(in-tree `allow_write` entries are no longer emitted). This plan removes the class: no agent runs
git against the main checkout, and loom never leaves it mid-merge. Merges also blocked whenever the
main checkout held uncommitted files, even on paths the merge never touches.

## Measured facts (git 2.53, scratch repository)

| Operation, `main` checked out in the checkout | Result |
| --- | --- |
| `git merge --no-ff feat` with an unrelated file staged | Refused ("Your local changes ... would be overwritten") |
| `git merge-tree --write-tree main feat`, `git commit-tree`, then `git merge --ff-only <commit>`, with unrelated staged, unstaged and untracked files | Succeeds; all three survive |
| Same, unstaged edit to a file the merge changes | Refused before any write; no branch file appears |
| Same, untracked file the branch adds, byte-identical | Refused (git does not compare content) |
| Same, that untracked file different | Refused |
| `git merge --ff-only --autostash`, overlapping edit on an adjacent line | Conflict markers written into the working file; changes left in the stash |

## Decisions (operator, 2026-10-01; settled)

1. Conflicts are still resolved by a loom-spawned resolver session, now in the stage's worktree.
2. Overlapping uncommitted edits whose reapply is predicted clean are reapplied automatically, with a
   backup ref. Accepted residual: an editor holding an affected file can save stale content over it
   between stash and pop.
3. `MAX_MERGE_RESOLVER_ATTEMPTS` becomes 6 and counts EVERY resolver session, including the first.
   Today the first resolver (spawned in `attempt_auto_merge`) is uncounted, so the real maximum is
   1 + 3.
4. No backward compatibility or migration code (unreleased project).

## Target design

```text
merge_stage(stage, target)                      MergeLock held; run_git on the main checkout R
  refuse if R has an operator operation in progress (MERGE_HEAD, CHERRY_PICK_HEAD,
    REVERT_HEAD, rebase-merge/, rebase-apply/)            → Blocked(OperatorOperation)
  T0 = rev-parse <target>;  B = rev-parse loom/<id>
  merge-tree --write-tree --name-only --no-messages -z T0 B
    exit 1 → Conflict { paths }                            R untouched
    exit 0 → M = commit-tree <tree> -p T0 -p B -m "Merge loom/<id> into <target>"
             advance(<target>, T0 → M)

advance(<target>, T0 → M)
  where is <target> checked out?  (worktree list --porcelain, "branch refs/heads/<target>")
    in R          → classify R's uncommitted state against T0..M (table below), then
                    merge --ff-only M  (refused if <target> moved: recompute next tick)
    nowhere       → update-ref -m "loom: merge loom/<id>" refs/heads/<target> M T0
    elsewhere     → Blocked(TargetCheckedOutElsewhere { path })

Conflict → stage MergeConflict → resolver spawned (counted) IN THE STAGE WORKTREE:
  git merge <target> into loom/<id>, resolve, rerun the stage's acceptance, commit,
  loom stage merge <id> --resolved
--resolved → daemon checks the worktree (pinned git): no unmerged paths, no MERGE_HEAD, clean,
  HEAD contains the current <target> tip → runs merge_stage again (now clean unless <target>
  moved) → on success finalize + cleanup as today; on Conflict another counted round
Blocked(*) → MergeBlocked with a typed reason; NO resolver; merge retried every tick
```

Consequences: loom stops checking out the target in the operator's checkout
(`git/merge/mod.rs:266` `checkout_branch` leaves `merge_stage`); loom never creates `MERGE_HEAD`
in R, so a `MERGE_HEAD` there is always the operator's; the resolver's capsule is worktree-rooted
and gets no write access to R.

### Uncommitted state in R (only when `<target>` is checked out in R)

Inputs: `git status --porcelain=v2 -z --untracked-files=all` in R and
`git diff --name-status -z --no-renames T0 M`. "Touched" = a path `T0..M` modifies, adds or deletes.

| Case | Action |
| --- | --- |
| No tracked change and no untracked file is on a touched path | `merge --ff-only M`; git carries local changes |
| Only overlap: untracked files `M` adds whose bytes equal `git cat-file blob M:<path>` (a symlink never counts as equal) | Remove those files, then fast-forward |
| Tracked overlap; dry run clean | Reapply (below) |
| Tracked overlap with a predicted conflict, or an untracked file that differs | `Blocked(UncommittedOverlap { paths })`; R untouched |

Dry run: `S = git stash create` (no working-tree change); empty output means nothing tracked is
dirty. Then `git merge-tree --write-tree --merge-base=T0 M S`; exit 0 = clean.
Reapply: `git update-ref refs/loom/autostash/<stage-id>-<unix-secs> S`, `git stash push --quiet`,
`git merge --ff-only M`, `git stash pop --index --quiet`; if that fails, `git stash pop --quiet`.
If both fail, leave the stash entry, never `reset --hard`, and return
`Blocked(ReapplyFailed { backup_ref })`. Report the backup ref in the stage's merge note either way.
Never use `--autostash` (last measured row).

## Current code map (verified)

| What | Where | Change |
| --- | --- | --- |
| `merge_stage` | `loom/src/git/merge/mod.rs:78` | Rewrite per the design; keep `MergeLock` and the operator-merge refusal (`require_no_active_merge`, :57) |
| Probe merge in R | `loom/src/git/merge/probe.rs::get_conflicting_files_from_status` (checks out the target, `merge --no-commit --no-ff`, aborts; refuses a dirty tree :128) | Delete; only caller `orchestrator/core/merge_handler.rs:902` takes paths from `merge_tree` |
| `conflict_resolution_instructions` | `git/merge` (re-export `git/mod.rs:29`), pinned in `git/merge/tests.rs:~96-107`, otherwise unused | Delete with its test |
| Git timeouts | `loom/src/git/runner.rs:~47` `git_timeout` (120 s only for checkout/commit/merge/...) | Add `merge-tree`, `commit-tree`, `update-ref`, `stash` |
| Git version | none today | New check, git >= 2.40, in `commands/run/mod.rs` `run_startup_preflights` (~:115) |
| `merge_stage` callers | `orchestrator/auto_merge.rs:94`; `orchestrator/progressive_merge/execution.rs:52` (CLI `loom stage complete` non-control path, `loom stage human-review --approve`); `commands/stage/merge.rs:192` `merge_retry` | Follow the new `MergeResult` |
| First resolver spawn | `orchestrator/auto_merge.rs:128-132` (`Session::new_merge`, `generate_merge_signal`, `spawn_merge_session(.., repo_root)`), uncounted | Remove; set `MergeConflict` and let the spawn loop spawn it (counted) |
| Spawn loop | `merge_handler.rs:820` `spawn_merge_resolution_sessions` → `resolver_spawn.rs:41` `spawn_resolver_if_due` → `merge_handler.rs:883` `spawn_merge_resolution_session` → `resolver_stop.rs:46` `launch_resolver` | Skip `Blocked(*)` stages; take conflict paths from `merge_tree`; drop the `merge_head_exists`/`detect_in_progress_merge_at(repo_root)` reads (:899, :918) |
| Worktree spawn | `orchestrator/terminal/backend.rs:~162` private `spawn_worktree_session(kind, stage, &Worktree, ..)` → `native/mod.rs:245` / `tmux/backend.rs:98`; `Worktree` at `models/worktree.rs:9` (built like `orchestrator/core/stage_spawn.rs:~250`) | Add `pub fn spawn_merge_session_in_worktree`; `launch_resolver` and `commands/stage/merge_resolver.rs:96` (CLI no-daemon path) use it. `spawn_merge_session` (:183) loses its callers: delete it and `spawn_main_repo_session`'s Merge use |
| Capsule | `native/session_settings.rs:93-99` derives `worktree_rooted` from the cwd | Nothing: a Merge session in `.worktrees/<id>` gets a worktree-rooted capsule |
| Wrapper env | `native/wrapper.rs:202-216` `kind_env` | Keep `LOOM_MERGE_SESSION=1`; keep `LOOM_WORKTREE_PATH` unset for Merge (presence-based gates would misread it) |
| Relay | `relay/matrix.rs` Merge rows allow `MergeResolved`; `relay/emit/cwd.rs:36` boundary is the project root | Nothing required |
| Daemon `--resolved` | `orchestrator/core/inbox_drain/merge_resolved.rs:27` `resolve_merge_from_inbox`; `merge_is_concluded` (:67-83) reads R | Check the stage worktree through `WorktreeGit::pinned_in_project_of` (`git/worktree/pinned.rs`; the agent controls the worktree's `.git` file); then run `merge_stage`; finalize only after it advanced |
| Local `--resolved` | `commands/stage/merge.rs:56-113` `merge_resolved`; `merge_verify.rs` `verify_or_derive_completed_commit` requires ancestry in the target | Same flow as the daemon handler; update the hint text (`merge_verify.rs:~58`) |
| Resolver exit | `merge_handler.rs:~35-200` `handle_merge_session_completed`, `:225` `finalize_merge_resolution` (both require the stage commit already in the target) | Run `merge_stage` first when the stage branch is not yet in the target; this is also where the worktree cleanup for a resolved merge moves (W3b step 6) |
| Cleanup on `--resolved` | `orchestrator/core/inbox_drain/merge_resolved.rs:~55` removes the worktree as soon as `--resolved` is relayed | Remove it there; cleanup runs at resolver exit |
| Blocked handling | `merge_handler.rs:437` `persist_merge_blocked` (`InfrastructureError`); auto-merge is one-shot (`orchestrator/core/recovery.rs:~765-795` `merge_retry_attempted`) | New typed block reason on the stage; per-tick retry of `merge_stage` for those stages only |
| Merge gate | `merge_handler/merge_gate.rs:266` `control_path_violation` diffs `merge-base(target, stage)..stage` | Unchanged: after the resolver merges the target in, the merge-base is the target tip |
| Signal | `orchestrator/signals/merge.rs:17, :156-250` | Rewrite (W3a); keep the `## Target` fields that `parse_merge_signal_content` reads |
| Unused conflict signal | `orchestrator/signals/merge_conflict.rs` `generate_merge_conflict_signal` (tests only) | Delete with its tests |
| Cap | `orchestrator/core/merge_handler/resolver_attempts.rs:15` (=3, doc :9), test :129-133 `max_merge_resolver_attempts_matches_default_retries` | 6; rename the test; drop the "mirrors max_retries" doc |

Left alone this round (harmless once loom creates no `MERGE_HEAD` in R; record as a concern):
`orchestrator/merge_attribution.rs` (`attribute_main_repo_merge` :153, `reconcile_main_repo_active_merge`
:260, called from `recovery.rs:263`) and `commands/stage/complete.rs:109-300`
`route_complete_conflict` rules 3-8. An operator `MERGE_HEAD` attributes as `GlobalUnattributed`,
which mutates nothing.

## Work units

| Unit | Agent | Files owned | Read-only |
| --- | --- | --- | --- |
| W1 git layer | `loom-senior-software-engineer` (opus) | `git/merge/mod.rs`, new `git/merge/tree.rs`, new `git/merge/checkout_state.rs`, `git/merge/probe.rs` (delete), `git/merge/tests.rs`, `git/mod.rs`, `git/runner.rs`, `commands/run/mod.rs` | `git/worktree/pinned.rs`, `git/merge/lock.rs` |
| W3a resolver location + signal | `loom-software-engineer` (sonnet) | `orchestrator/terminal/backend.rs`, `orchestrator/core/merge_handler/resolver_stop.rs`, `commands/stage/merge_resolver.rs`, `orchestrator/signals/merge.rs`, `orchestrator/signals/merge_conflict.rs`, `orchestrator/signals/tests_merge.rs`, `orchestrator/monitor/tests/merge_sessions.rs`, `orchestrator/terminal/mod.rs` (comment), `loom-hooks/commit-guard.sh` and `loom-hooks/stage-terminal-guard.sh` (comments only) | `native/wrapper.rs`, `relay/*` |
| W3b flow | `loom-senior-software-engineer` (opus) | `orchestrator/auto_merge.rs`, `orchestrator/core/merge_handler.rs`, `merge_handler/resolver_spawn.rs`, `merge_handler/spawn_failure.rs`, `merge_handler/resolver_attempts.rs`, `orchestrator/core/recovery.rs`, `orchestrator/core/inbox_drain/merge_resolved.rs`, `orchestrator/progressive_merge/execution.rs`, `commands/stage/merge.rs`, `commands/stage/merge/*` (includes `finish.rs`), `commands/stage/merge_verify.rs`, the stage model file holding the block reason, new integration test `loom/tests/merge_off_checkout.rs` | W1's API |
| W4 docs | main session or `loom-software-engineer` (sonnet) | knowledge files and docs listed under "Docs" | all code |

Order: W1, then W3a and W3b in parallel (disjoint files; W3b calls `launch_resolver`, whose
signature W3a keeps), then W4. All paths are under `loom/src/` unless they start with `loom/` or
`loom-hooks/`.

### W1 git layer

1. `git/merge/tree.rs`: `merge_tree(repo, target, branch) -> Result<TreeMerge>` with
   `TreeMerge::{Clean { tree: String }, Conflict { paths: Vec<String> }}` from
   `merge-tree --write-tree --name-only --no-messages -z`: exit 0 clean, exit 1 conflict (first
   NUL field is the tree, the rest are paths, deduplicated), anything else an error carrying stderr.
   `commit_merge(repo, tree, parents: [&str; 2], message) -> Result<String>`.
   `advance_target(repo, target, old, new, stage_id) -> Result<Advance>` with
   `Advance::{Advanced, Blocked(MergeBlock)}`; `MergeBlock` = `OperatorOperation`,
   `TargetCheckedOutElsewhere { path }`, `TargetMoved`, `UncommittedOverlap { paths }`,
   `ReapplyFailed { backup_ref }`. All through `run_git` (which already disables hooks and
   fsmonitor) on R; R is the operator's checkout, not agent-writable, so pinning is not needed.
2. `git/merge/checkout_state.rs`: pure `classify(status_v2_z, diff_name_status_z) -> Classification`
   for the four table rows (unit-test each row, plus renames reported as delete+add, paths with
   spaces and newlines, a staged-only change). Thin effectful wrappers do the dry run and reapply
   exactly as written in the design.
3. Rewrite `merge_stage` per the design; `MergeResult` gains `Blocked(MergeBlock)` and its
   `Conflict` carries the paths. Remove `checkout_branch`/`abort_merge` uses from it; delete either
   helper if it is left without callers.
4. Delete `probe.rs` (or the parts only `merge_handler.rs:902` used; that call site is W3b's and
   switches to `merge_tree`) and `conflict_resolution_instructions` with its re-export and test.
5. `runner.rs` timeouts; git >= 2.40 preflight (refuse `loom run` with the installed version named).
6. Tests in temp repositories reproducing every row of "Measured facts", plus: target not checked
   out anywhere (update-ref, R's HEAD untouched), target checked out in a second worktree
   (blocked), target moved between `rev-parse` and advance (blocked `TargetMoved`), operator
   `MERGE_HEAD` (blocked), reapply clean (local edits and the merge both present afterwards,
   backup ref exists), reapply predicted conflict (R byte-identical before and after).

### W3a resolver location + signal

1. `SessionBackend::spawn_merge_session_in_worktree(stage, &Worktree, session, signal_path)`
   delegating to `spawn_worktree_session(SessionType::Merge, ..)`. `launch_resolver` and
   `merge_resolver.rs:96` build the `Worktree` for `.worktrees/<stage-id>` and call it. If the
   worktree directory is missing, return an error the caller routes to human review.
2. Rewrite the merge signal: the agent works in its stage worktree; steps are `git merge <target>`,
   resolve, rerun the stage's acceptance criteria (reverse today's "Do NOT re-run" at :238), commit,
   `loom stage merge <id> --resolved`; never touch the main checkout. Drop the `ActiveMergeState`
   branches (:200-215). Update the pinned tests listed in the code map.
3. Delete `generate_merge_conflict_signal` and its tests; fix the stale comments.

### W3b flow

1. `attempt_auto_merge`: on `Conflict`, set `MergeConflict` and spawn nothing. On `Blocked`, persist
   `MergeBlocked` with the typed reason (a field on the stage, serialized with the stage file).
2. Spawn loop: skip stages blocked by a `MergeBlock`; take conflict paths from `merge_tree`.
3. Per tick, for each stage carrying a `MergeBlock`, run `merge_stage` again; success follows the
   normal verified-merge path, `Conflict` moves it to `MergeConflict`.
4. `--resolved` (daemon and local) and `handle_merge_session_completed`: verify the worktree
   (pinned git), then `merge_stage`; finalize and clean up only after `Advanced` and
   `verify_merge_succeeded`.
5. Cap 6, counted for every resolver; update `next_step.rs` test literals (`:133`, `:140`).
6. Worktree cleanup waits for the resolver to exit. `inbox_drain/merge_resolved.rs:~55` removes
   the stage worktree as soon as `--resolved` is relayed; with the resolver now running inside that
   worktree, that deletes a live session's working directory and breaks its hooks. Move the cleanup
   for a resolved merge to the resolver session's exit (`handle_merge_session_completed`), after
   the merge has advanced and been verified.
7. `commands/stage/merge/finish.rs:32-40` prints the cleanup outcome only on `Done`; print the
   reason when cleanup ends `Failed` or `Refused`, so a deferred cleanup is never silent.
8. Integration test `loom/tests/merge_off_checkout.rs` (`serial_test`; no process may outlive the
   test): a conflicting stage resolved in its worktree reaches `main` while R holds unrelated
   staged and unstaged files, which survive; R never shows `MERGE_HEAD` during the flow; an
   overlapping uncommitted edit leaves the stage `MergeBlocked` with the path named and merges on
   the tick after the edit is committed.

## Docs (W4)

Rewrite to the new flow: `doc/loom/knowledge/architecture/merge-flow.md` (Outcomes table, "The merge
runs in the operator's main checkout", the staged-index paragraph, Merge Lock takers),
`patterns/merge-and-recovery.md` (anti-respawn cap 6, Merge Recovery Flow, Resolver Spawn Loop),
`concerns/merge-and-recovery-edge-cases.md` (probe exemption, uncounted first resolver, target left
checked out: now resolved), and `README.md` around :135, :329, :1123 if they describe where the
resolver works. Add a concern for the attribution code left in place. Add a mistakes entry linking
the bind-mount merge failures (`mistakes/sandbox-tooling-and-network.md` 2026-09-10 and 2026-09-13,
`mistakes/phantom-merges.md` 2026-09-19, and 2026-10-01 in another project). Use
`loom knowledge replace-section` / `update`, run from the repository root; finish with
`loom knowledge check`.

## Verification (main session)

- The full gate after each unit, and once more at the end.
- `rg -n 'checkout_branch\(|get_conflicting_files_from_status|spawn_merge_session\(' loom/src`
  shows no production call that runs a checkout or merge in R or spawns a resolver there.
- Run W3b's integration test alone with `--nocapture` and read its output.
