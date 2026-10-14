---
---
# Merge And Recovery

> Progressive merge, conflict recovery

## Progressive Merge Pattern

Dependencies merged to main before dependent stages execute: `Stage A completes -> Merge A to main -> Stage B starts`. Base branch resolution: no deps = init_base_branch or default; all deps merged = main; single dep not merged = dependency branch (legacy fallback). MergeLock prevents concurrent merges (30s timeout, 5min stale cleanup).

## Merge Anti-Respawn Pattern

Resolver sessions are spawned only within a budget: `MAX_MERGE_RESOLVER_ATTEMPTS` (6) per stage, counting every resolver session including the first (the spawn loop starts the first one; `attempt_auto_merge` spawns none), counted in `.loom/work/merge-resolver-attempts/<stage-id>.count` (`orchestrator/core/merge_handler/resolver_attempts.rs`). `ReservedAttempt::record` writes count+1 before a spawn; `keep()` runs as soon as the backend spawn returns `Ok`, and dropping the guard without it rolls the count back, so only a spawned resolver consumes budget. A counter file that exists but cannot be read or parsed, or a counter path that is not a regular file, reads as the cap; a counter that cannot be written refuses the spawn and routes the stage to review.

After a backend `Err`, `settle_resolver_spawn` (`resolver_stop.rs`) kills the would-be session and asks `teardown_proves_gone` whether it is gone. On tmux the kill must succeed and the liveness probe must say gone, because a surviving server on the socket shows only as a kill failure. On the native lane the probe decides alone: it checks the `loom-merge-<stage>` window as well as the PID file, and a lane with no terminal launches nothing. Proven gone refunds the attempt, and the failure is retried next tick. Not proven keeps the attempt and routes the stage to review. A `save_session` failure after a successful spawn only warns: the session stays in `active_sessions`, which holds the stage while its PID identity lives.

The merge signal file is a liveness record, never a respawn guard: `find_live_merge_session_for_stage` (`orchestrator/signals/merge.rs`) deletes it once its session is dead. A readable signal whose session record is missing is stale, because failed spawns, orphan recovery and `loom sessions kill` all leave record-less signals behind. A record that exists but cannot be read is an error, which routes the stage to review. An unreadable signal is attributed through the session record its filename names: another stage's is skipped, this stage's blocks the spawn while alive and is removed when dead, and one that cannot be attributed routes the stage to review.

## Merge Recovery Flow

A conflicting merge leaves the stage in `MergeConflict` and R untouched; a merge that cannot proceed leaves it in `MergeBlocked` with `Stage.merge.block` (see [merge-flow](../architecture/merge-flow.md)). Neither state holds an agent session open: `commit-guard.sh` allows exit for them and `detection.rs` treats the exit as normal.

1. `spawn_merge_resolution_sessions` kills any stale original session, then starts a resolver in the stage worktree `.worktrees/<id>` (`SessionBackend::spawn_merge_session_in_worktree`) with a worktree-rooted capsule and `LOOM_MERGE_SESSION=1`. `LOOM_WORKTREE_PATH` is left unset because presence-based gates would read it as a stage agent.
2. Its signal (`orchestrator/signals/merge.rs`) tells it to continue a merge in progress or run `git merge --no-commit --no-ff <target>` in the worktree, resolve, rerun the stage's acceptance criteria, commit, and run `loom stage merge <id> --resolved`. It forbids rebase, reset, squash, amend and force-push, and for a non-conflict failure quotes up to 5 sanitized lines of the failure. It never touches the main checkout and never runs `loom worktree remove`. The signal includes an "Inherited Responsibilities" section explaining that the resolver owns the stage.
3. The relayed `--resolved` is checked by `check_resolved_worktree` and landed by `land_stage_merge`; the worktree is removed when the resolver exits (`resolver_exit.rs`), or by the leftover sweep once no session runs for the stage.

`check_resolved_worktree(repo_root, stage_id, completed_commit)` (`git/merge/resolved.rs`) uses pinned git, whose `config.worktree` check applies ([Execution Containment](../architecture/execution-containment.md)). It requires HEAD on `loom/<id>`, no `MERGE_HEAD`, no unmerged path, no tracked change (untracked files allowed; `--no-optional-locks`), and the recorded `completed_commit` (hex-validated) as an ancestor of HEAD, so a rebase, squash or reset by the resolver is refused. It does not require the current target tip: `merge_stage` merges newer target commits or reports a conflict.

Key invariant: the original execution session MUST exit when a merge conflict is detected. `bail!()` in `complete_with_merge()` ends it, `commit-guard.sh` does not block the exit, and the spawn loop kills a stale session before spawning the resolver.

**Daemon ordering invariant:** Reconciliation runs BEFORE `sync_graph_with_stage_files` AND BEFORE `recover_orphaned_sessions`. Recovery deletes orphaned merge session files; attribution depends on their metadata. Sync reads stage files into the graph; if reconcile flips disk state AFTER sync, the graph keeps the stale view and would queue dependents based on a phantom merge.

**Daemon-off CLI parity:** `loom stage complete` on a `Completed + merged=true` stage with an active main-repo merge attributed to it triggers the same revert the daemon performs (`Completed → MergeConflict + merged=false + merge_conflict=true`) before spawning the resolver. The router's `RevertAndSpawnResolver` arm encodes this; persistence is the caller's responsibility, BEFORE spawn so `spawn_merge_resolver`'s status contract is satisfied.

## Attribution-Aware Recovery

`MERGE_HEAD` in the main repo is global state — only one merge in progress at a time across all stages. Stage-state mutation triggered by detecting it must come with proof of attribution; without proof, refuse rather than mutate. Loom creates no `MERGE_HEAD` in the main checkout, so for loom's own merges this code has nothing to attribute; it stays for an operator's own merge, which attributes as `GlobalUnattributed` and mutates nothing (see [concerns](../concerns/merge-and-recovery-edge-cases.md#attribution-code-has-nothing-to-attribute)).

Three attribution sources (first match wins):

1. **MergeSession metadata** — orphaned or live `SessionType::Merge` with matching `merge_source_branch`.
2. **Branch HEAD match** — a `MERGE_HEAD` SHA equals `loom/<stage-id>` HEAD.
3. **Completed-commit match** — a `MERGE_HEAD` SHA equals `stage.completed_commit`.

**BaseConflict carve-out:** When current HEAD is `loom/_base/*` (or any session has `SessionType::BaseConflict` matching it), return `GlobalUnattributed` even if the merge heads contain a stage branch's commit. Multi-dependency base merges check out their own branch and run a merge there; their MERGE_HEAD must NOT mutate stage state.

Single decision point: `attribute_main_repo_merge` in `orchestrator/merge_attribution.rs`. Both daemon recovery (`reconcile_main_repo_active_merge`, called from `orchestrator/core/recovery.rs`) and the CLI router consume it.

## Pure Routing Helper

`route_complete_for_conflicts` is the canonical example: read-only function that returns `CompleteConflictRoute` without writing to disk. Persistence is the caller's responsibility on the success path only. This preserves the "refusal preserves stage file state" invariant — refusal always leaves the stage file untouched, which is critical for tests and for users investigating why a completion attempt was rejected. The router's attribution rules (`route_complete_conflict` rules 3-8 in `commands/stage/complete.rs`) apply only to an operator's own `MERGE_HEAD`, since loom creates none.

Apply this pattern when adding routing/verification helpers: keep the function pure, return an enum of decisions, let the caller persist on the success branch.

## No Worker Thread for Adjudication (Retired Pattern)

**Retired 2026-08-31. Do not reintroduce this for adjudication.** This section described a
worker-thread + mpsc channel the adjudicator used to drive a blocking call off the orchestrator's
poll loop: `worker_completion_tx`/`_rx` on the `Orchestrator`, a `std::thread::spawn` per dispute,
and a drain in the main tick. All of it is deleted: the adjudication worker and
client modules were removed outright, so there is no thread to reintroduce it into.

Adjudication now spawns a real loom session through the `TerminalBackend` and observes the
resulting state change on the ordinary poll loop, the same way merge conflict resolution has always
worked. That removed the thread registry, the mpsc channel, the cooperative cancellation flag, the
`.inflight` staleness marker, and the retry/backoff loop in one go — a session is already a tracked,
restart-surviving, externally-observable unit of work, and the thread machinery was reimplementing a
worse version of it.

The general lesson, if you are reaching for a worker thread in the orchestrator: ask first whether
the work is a SESSION. Loom already knows how to spawn one, track its liveness across a daemon
restart, and notice when the state it was supposed to change has changed. A thread gets you none of
that and costs you a shutdown story.

## Dispute File Authority Split Pattern

Three-file trust boundary to prevent self-approval attacks:

| File             | Writer                             | Content                  | Rationale                                            |
| ---------------- | ----------------------------------- | ------------------------ | ---------------------------------------------------- |
| `.loom/work/disputes/<stage-id>/<n>/request.md`     | Daemon (on agent's behalf via RPC) | Agent's evidence payload | Agent can read but never write directly              |
| `.loom/work/disputes/<stage-id>/<n>/verdict.md`     | Daemon worker thread only          | Verdict + citations      | Stage agents never write here — daemon-authored only |
| `.loom/work/disputes/<stage-id>/<n>/applied.marker` | Daemon only (zero-byte)            | Idempotency guard        | Prevents re-application on restart                   |

If the agent could write both request and verdict, it could pre-fill `verdict: Accept` and self-approve. The split enforces the trust boundary at the filesystem level.

## Plan Amendment Atomic Write Pattern

For amending the IN_PROGRESS plan file safely (Stage 3):

```text
1. Acquire .loom/work/plan_versions/.lock  (file lock — serializes concurrent amendments)
2. Compute new plan content in memory
3. Atomic-write .loom/work/plan_versions/<n>.md  (full snapshot)
4. Append to .loom/work/plan_versions/audit.md  (O_APPEND — atomic for small rows)
5. Atomic temp+rename of IN_PROGRESS plan file to new content
6. Release lock
```

Recovery on crash: scan audit.md for latest amendment; verify plan file matches snapshot. If mismatch → restore from `<n>.md`. If `<n>.md` missing → discard audit row, use `<n-1>.md`.

Note: `plan/graph/loader.rs:60-86` PREFERS `.loom/work/stages/` files over the plan file. Plan-file amendment MUST also update the corresponding `.loom/work/stages/<stage_id>.md` for the change to be reflected in the running orchestrator graph.

## Merge Resolver Spawn Loop

`MergeConflict` and `MergeBlocked` never count as terminal for the watch-mode exit (`recovery.rs::stage_file_is_terminal`), so the daemon stays up until each such stage is merged or reaches `NeedsHumanReview`. Every tick `spawn_merge_resolution_sessions` routes each merge-state stage:

- A `MergeBlocked` stage WITH a `merge.block` goes to `retry_blocked_merge` (`merge_handler/blocked_retry.rs`): it repeats the merge when its inputs changed and never gets a resolver. A block is an environment state (operator operation, target checked out elsewhere, uncommitted overlap), not a content conflict.
- Every other stage goes to `spawn_resolver_if_due` (`orchestrator/core/merge_handler/resolver_spawn.rs`), in order:
  1. the early merge gate (a fail-open pre-filter): a branch touching a control path has any live resolver stopped (`resolver_stop.rs::stop_gated_resolvers`), then goes to review;
  2. stale-session cleanup, then the live-signal check;
  3. a strict existence check on the stage branch and the target branch: missing goes to review, a git failure is transient;
  4. a check that the stage worktree exists: a missing worktree goes to review, because the resolver works there;
  5. the resolver cap (6), which escalates to review;
  6. a fresh re-read of the stage, then `clean_merge_settled` (`resolver_spawn.rs`): when `merge_tree` is now clean (gate included) it lands the stage with no resolver; an unprovable landing goes to review, and a failure falls through to the resolver;
  7. the attempt reservation and the spawn.

A transient spawn failure (tmux, lock timeout) is retried every tick with no cap, and the daemon stays up meanwhile; this is an operator decision. A pass works from a stage copy that can go stale while it runs git, so every routing goes through `route_merge_stage_to_review` (`review_route.rs`), which re-checks the status inside the `update_stage` lock and writes nothing when a concurrent `loom stage merge` has already moved the stage on. `route_to_human_review` writes whatever the status and is kept for `try_auto_merge`, whose stage is `Completed`.

A resolver that cannot finish runs `loom stage block <id> "<reason>"`, as its signal tells it. The relay matrix applies Merge × block, and the drain routes it to `hold_merge_for_resolver` (`merge_handler/resolver_give_up.rs`): a give-up. `require_giveable` refuses unless the record is `Running` and is the stage's live resolver (the tracked session or `find_live_merge_session_for_stage`), so a retired resolver's late ticket cannot stop its successor, and refuses a `MergeBlocked` stage with a typed `merge.block`, which loom retries itself. The reason is flattened by `inline_safe`; every resolver of the stage is stopped (`stop_resolvers_proven` reports whether each stop was proven) and the stage goes to `NeedsHumanReview` with the reason and the manual steps.

The `--resolved` reply mapping is the pure `settle_for_landing` (`inbox_drain/merge_resolved.rs`). `loom stage merge` hitting a conflict moves a `MergeConflict`/`MergeBlocked` stage to `MergeConflict` (the `MergeBlocked -> MergeConflict` edge exists for this), clears its `failure_info`, and prints what the daemon will do next, read from the resolver counter (`commands/stage/merge/next_step.rs`). A `Completed` stage that was never merged (auto-merge disabled) is left untouched and gets only the manual steps.
