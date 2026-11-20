# D1: the daemon holds merges while the target is held

Stage `guard-integration`, tier opus, in parallel with D2 and D3 (disjoint files). Read
`../common.md` first. Stage `guard-core` is merged: `loom::git::target_guard`,
`MergeBlock::TargetHeld` and `loom::git::hooks::install_reference_transaction_hook` exist; read
`loom/src/git/target_guard/mod.rs` before you start.

You own `loom/src/orchestrator/core/target_hold.rs` (new),
`loom/src/orchestrator/core/target_hold_tests.rs` (new), `loom/src/orchestrator/core/mod.rs`,
`loom/src/orchestrator/core/orchestrator.rs`, `loom/src/orchestrator/core/run.rs`,
`loom/src/orchestrator/core/recovery.rs`, `loom/src/orchestrator/core/stage_executor.rs`,
`loom/src/orchestrator/core/spawn_setup.rs`,
`loom/src/orchestrator/core/merge_handler/auto_merge_outcome.rs`,
`loom/src/orchestrator/core/merge_handler/resolver_spawn.rs`,
`loom/src/orchestrator/core/merge_handler/blocked_retry.rs`,
`loom/src/orchestrator/core/merge_handler.rs`, `loom/src/orchestrator/scheduling_report.rs`,
`loom/src/git/worktree/operations.rs`, `loom/tests/integration/helpers.rs` (one change: the
`create_worktree` call in `create_worktree_isolated`, about line 276, passes `None` as the new
start point; every integration test binary that `#[path]`-includes `helpers.rs` compiles that
line, the frozen `tests/target_cli_contracts.rs` among them), `loom/maintainability-baseline.txt`,
and the existing test files under `loom/src/orchestrator/` that your change breaks (see
"Existing tests").

Frozen, read-only: `loom/src/orchestrator/core/merge_lifecycle_e2e_tests_guard_contracts.rs`
and its harness `loom/src/orchestrator/core/merge_lifecycle_e2e_tests.rs` (the module
declarations). Read the contracts first; they call `check_target_guard`, `try_auto_merge`,
`spawn_merge_resolution_sessions`, `Recovery::sync_graph_with_stage_files` and
`scheduling_report::alerts`.

Your one check, run once at the end, from `loom/`:
`cargo test --lib orchestrator::`.

## Why

`git::target_guard` decides whether a move of the target is acceptable; the daemon must act on
it. Today three daemon paths settle a stage without `merge_stage`, so an agent that
fast-forwards `main` to its own branch gets the stage marked merged and cleaned up with no gate:
`sync_graph_with_stage_files` marks `merged = true` when `verify_merge_succeeded(completed_commit,
target)` holds (`recovery.rs`, about lines 448-455); the auto-merge precheck routes a branch
with zero commits beyond the moved target to human review with a misleading reason
(`auto_merge_outcome.rs::auto_merge_precheck_blocks`, about lines 23-56); and new stage
worktrees start at the live target tip (`spawn_setup.rs::resolve_worktree`).

## 1. `target_hold.rs` (new, declared `mod target_hold;` in `core/mod.rs`)

State: add `pub(super) target_hold: Option<crate::git::target_guard::Hold>` to `Orchestrator`
(`orchestrator.rs`, struct about line 84, initialised `None` in `Orchestrator::new` about line
166; keep `orchestrator.rs` under 400 lines), plus `target_guard_error: Option<String>` for the
log-once rule below.

    impl Orchestrator {
        /// Run the target guard for the configured target and remember the result.
        pub(in crate::orchestrator::core) fn check_target_guard(&mut self) -> Option<Hold>;
        /// Whether the last check left the target held.
        pub(in crate::orchestrator::core) fn target_held(&self) -> bool;
        /// Ancestry of `commit` against the accepted target tip.
        pub(in crate::orchestrator::core) fn merged_into_accepted(&self, commit: &str,
            target: &str) -> anyhow::Result<bool>;
        /// Why `stage` must not spawn now: `Held` for a held stage, `TargetHeld` for a
        /// knowledge stage while the target is held.
        pub(in crate::orchestrator::core) fn spawn_hold_reason(&self, stage: &Stage)
            -> Option<BlockReason>;
        /// Install the reference-transaction hook and run the first check.
        pub(in crate::orchestrator::core) fn start_target_guard(&mut self);
    }

- The target is `crate::git::branch::resolve_target_branch(&self.config.base_branch,
  &self.config.repo_root)`; paths are `self.config.repo_root` and `self.config.work_dir`.
- `check_target_guard`: `target_guard::check(..)`: `Ok(Some(Clear))` sets `target_hold = None`;
  `Ok(Some(Held(h)))` sets it, and when `h.observed` differs from the previous hold's (or there
  was none) prints once to stderr and `tracing::warn!`: `target_guard::hold_alert(target, &h)`;
  `Ok(None)` (lock contended) keeps the last state; `Err(e)` keeps the last state and logs
  `target guard: {e:#}` once per distinct message (`target_guard_error`). Returns
  `self.target_hold.clone()`.
- `merged_into_accepted` delegates to `target_guard::merged_into_accepted`.
- `start_target_guard`: `install_reference_transaction_hook(repo_root)`: `Installed` and
  `UpToDate` log at info; `ForeignHookPresent` and `Err` warn once. Then `check_target_guard()`.
  Then, when `target_guard::attestation_mode(..)` is `Off { reason }`, print one line. When
  `target_guard::attestation_latched(work_dir, target)` is `Ok(false)`: `target guard:
  attestation off ({reason}); only control-path changes, rewrites of the target and unmerged
  stage work are held`. When it is `Ok(true)` (the run started with attestation on): `target
  guard: attestation off ({reason}), but this run recorded it on; every move without a ledger
  line holds until loom target accept`.

## 2. Call sites

- `run.rs` `initialize_run` (about lines 45-71): first statement after
  `validate_work_dir_state`: `self.start_target_guard();`. It must run before
  `reconcile_and_update_graph`, `sync_graph_with_stage_files` and
  `spawn_merge_resolution_sessions`, which already merge at startup.
- `run.rs` `run_tick` (about lines 75-106): `self.check_target_guard();` right after the first
  `tick::record(..)` and before `reconcile_and_update_graph`.
- `recovery.rs` `sync_graph_with_stage_files` (ledger: file 1462, function 508): replace the
  `crate::git::merge::verify_merge_succeeded(&completed_commit, &target_branch,
  &self.config.repo_root)` call of the Completed-stage ancestry check (about lines 450-455) with
  `self.merged_into_accepted(&completed_commit, &target_branch)`. In the same file,
  `verify_merged_true_or_revert` (about line 210, ledger 56) calls
  `verify_merge_succeeded(commit, target, ..)` on the live target: route it through
  `self.merged_into_accepted(commit, target)` too (otherwise a daemon restart during a
  `NotFastForward` hold reverts a merged stage, logs "Phantom merge detected" at error level, and
  the Completed arm re-marks it merged in the same pass). Change nothing else there. The
  functions and file shrink: lower the ledger lines to the new exact counts.
- `merge_handler.rs` `verify_and_finalize_merge` (the `NoWorktree` finalize, about line 79):
  replace its `verify_merge_succeeded` call with `self.merged_into_accepted(..)`; the precheck
  covers it only when its fresh check is not contended (the frozen contract
  `contended_no_worktree_finalize_does_not_settle` holds a `MergeLock` across `try_auto_merge`
  with the branch and worktree gone, and requires `merged == false`). File ledger 616: it must
  not grow. Leave `resolver_exit.rs::merged_flag_is_proven` and `landing.rs::provable_commit` on
  the live target: the first only confirms a `merged` flag a host path wrote, the second runs
  only after `merge_stage` (which checks the guard first) returned `Success` or
  `AlreadyUpToDate`.
- `auto_merge_outcome.rs` `auto_merge_precheck_blocks`: first thing, before the branch-exists
  check: when `self.check_target_guard()` returns `Some(h)`, call
  `self.record_merge_block(stage_id, MergeBlock::TargetHeld { target: target.into(),
  accepted: h.accepted, observed: h.observed })` and return `true`. This runs a fresh check
  (one `rev-parse` when nothing moved), so merges started from monitor events after
  `start_ready_stages` are covered, and it precedes the `NoWorktree` ancestry finalize and the
  zero-commits route.
- `blocked_retry.rs` `retry_blocked_merge_at` (about lines 88-124): its FIRST statement returns
  early when `self.target_held()` (the tick's check ran first). The retry resumes on the first
  tick after the operator accepts or restores: `remember_blocked_inputs` (about lines 172-198)
  memoises only `UncommittedOverlap`, `TargetCheckedOutElsewhere` and `OperatorOperation`, and
  `TargetHeld` must stay out of that memo.
- `resolver_spawn.rs` `spawn_resolver_if_due` (about line 41): its FIRST statement returns early,
  spawning nothing, when `self.target_held()`, before `gate_holds_merge_stage` and
  `clean_merge_settled`, which probe the live target and can route the stage to review: a
  resolver merges the target into the stage branch and would carry the held commits past a
  later restore.
- `stage_executor.rs` `start_stage` (ledger: function 149, file 597): replace the block

      // Skip if stage is held
      if stage.held {
          self.spawn_blocks
              .insert(stage_id.to_string(), BlockReason::Held);
          return Ok(());
      }

  with `if let Some(reason) = self.spawn_hold_reason(&stage) { self.spawn_blocks.insert(
  stage_id.to_string(), reason); return Ok(()); }` (rustfmt decides the layout; it must not
  grow). `spawn_hold_reason` returns `Some(BlockReason::Held)` when `stage.held`, else
  `Some(BlockReason::TargetHeld)` when `stage.stage_type == StageType::Knowledge` and
  `self.target_held()`, else `None`. Lower the ledger lines if they shrink.
- `scheduling_report.rs`: add `BlockReason::TargetHeld`; `describe()`: "waiting while the
  target branch is held (loom target status)"; `self_resolving()`: false. In `alerts(work_dir,
  daemon_running)`, BEFORE the `if !daemon_running { return alerts; }` gate, push one
  `Alert { severity: Severity::Warning, text: target_guard::hold_alert(&target, &hold) }` per
  entry of `target_guard::recorded_holds(work_dir)` (an `Err` adds nothing): a hold is a
  recorded fact that outlives the daemon. Keep the file under 400 lines; update the function's
  doc comment, which says everything is gated on `daemon_running`.

## 3. New worktrees start at the accepted tip

`git/worktree/operations.rs`: add a `start_point: Option<&str>` parameter to
`get_or_create_worktree` (about line 214) and `create_worktree` (about line 38; ledger 115, must
not grow: extract the `already exists` handling into a helper first). `git worktree add -b
loom/<id> <path> <start>` uses `start_point.or(base_branch)`.

The `already exists` branch deletes a leftover `loom/<id>` (no worktree) and recreates it only
when it has zero commits beyond its base. With a start point, measure against the start point:
`crate::git::branch::commits_between(&branch_ref(&branch_name), start, repo_root)` (it takes
raw revisions; `commits_ahead_of` takes branch names only and would error on an object id).
Without one, keep `commits_ahead_of(&branch_name, &base, ..)` as today. Measured against the
live target instead, a held target moved onto the branch's own commit B counts zero, and the
branch is deleted and recreated at the accepted tip, dropping the only branch ref to B (a git
reproduction confirmed it). Keep the fail-closed `unwrap_or(1)` on both. Lower the ledger line
if `create_worktree` shrinks.

`spawn_setup.rs` `resolve_worktree` (about lines 72-104): for `ResolvedBase::Main(target)`, pass
`target_guard::accepted_tip(&work_dir, target).ok().flatten()` as the start point; for
`ResolvedBase::Branch(_)`, `None`. With a clear guard the accepted tip equals the live tip, so
nothing changes; while held, new stages start from what loom accepted and spawns continue.

## 4. Existing tests

Run `rg -n 'try_auto_merge|sync_graph_with_stage_files|spawn_merge_resolution_sessions|get_or_create_worktree|create_worktree\(' loom/src loom/tests`
to find callers. `get_or_create_worktree`/`create_worktree` callers need the new argument
(`None`). A test that moves `main` by hand after the guard recorded a tip (the first
`try_auto_merge` or `check_target_guard` records it) and then expects a merge or a `merged`
flag now meets the guard: a plain fast-forward with no control path and no stage work still
passes (no hook is installed in those repositories, so attestation is off); a hand merge of a
`loom/*` branch into `main`, a control-path commit on `main`, or a rewrite of `main` holds.
Fix such a test by adding setup (accept the move with `target_guard::accept`, or record the
baseline after the move), never by changing an assertion line; if an assertion itself encodes
the old behaviour, follow common.md.

## 5. Tests (yours, beside the frozen contracts)

`target_hold_tests.rs` (declared from `target_hold.rs` with `#[cfg(test)] #[path =
"target_hold_tests.rs"] mod tests;`), real git, built with the same kind of fixture as
`merge_lifecycle_e2e_tests_support.rs` (that module is private to its parent; copy the few
helpers you need):

- `check_target_guard` logs a given hold once across repeated ticks (call it twice; assert the
  returned hold is equal and `target_guard_error` stays `None`);
- a contended `MergeLock` keeps the previous state;
- `spawn_hold_reason`: knowledge stage while held → `TargetHeld`; standard stage while held →
  `None`; held stage → `Held`;
- `resolve_worktree`-level: with the record's accepted tip behind the moved `main`,
  `get_or_create_worktree(.., Some("main"), Some(accepted))` creates `loom/<id>` at the accepted
  tip;
- `alerts(work_dir, false)` carries the hold line when the daemon is down;
- `blocked_retry`: a `TargetHeld` stage is not retried while held and lands on the first retry
  after `target_guard::accept` and a `check_target_guard()` call (`target_held()` reads the
  in-memory state, which only `check_target_guard` clears);
- `a_held_respawn_keeps_an_orphaned_stage_branch`: accepted `A`; `loom/<id>` at a commit `B`
  (a child of `A`) with no worktree; `main` moved to `B` with hooks disabled and the hold
  recorded; `get_or_create_worktree(<id>, .., Some("main"), Some(A))` reuses the branch, and
  `loom/<id>` is still at `B` afterwards (mutation M9 measures against `main` and turns it red);
- `resolve_worktree_starts_at_the_accepted_tip_while_held`: record a hold (accepted `A`, main
  moved to `T`), call the orchestrator's `resolve_worktree` for a new stage, and assert
  `loom/<id>` is at `A`, not `T` (drives the spawn path itself, not only
  `get_or_create_worktree`; mutation M5 of the plan turns it red);
- `a_conflicted_stage_gets_no_resolver_while_held`: a `MergeConflict` stage while held;
  `spawn_merge_resolution_sessions` returns 0 and
  `crate::orchestrator::core::merge_resolver_attempts(&work_dir, id)` stays 0 (mutation M6);
- `a_knowledge_stage_is_not_started_while_held`: `start_stage` on a Queued `stage_type:
  knowledge` stage while held leaves it Queued with `spawn_blocks` holding `TargetHeld`
  (mutation M8 makes `spawn_hold_reason` return `None` for it and turns this red);
- `start_target_guard` installs the hook (`is_reference_transaction_hook_installed`) and records
  the tip (`accepted_tip` is `Some`);
- `a_graft_does_not_mark_a_stage_merged`: graph holding `s`; a commit on `loom/s`; stage `s`
  saved Completed with `completed_commit` = the `loom/s` tip and `auto_merge: Some(false)` (so
  the sync's one-shot `try_auto_merge` returns early, `orchestrator/auto_merge.rs`
  `is_auto_merge_enabled`); write `<main tip> <loom/s tip>` to `.git/info/grafts`; run
  `sync_graph_with_stage_files` then `sweep_merged_leftovers`; assert `merged == false`, `loom/s`
  still exists and main is unchanged (guard-core's runner change makes this pass; mutation M4).

## Traps

- `check_target_guard` in the precheck takes the merge lock only when the target moved; never
  call it while holding a `MergeLock` (the precheck runs before `attempt_auto_merge`, which
  takes it).
- `record_merge_block` keeps the stage `MergeBlocked`; do not route a held target to human
  review.
- `recovery.rs` and `stage_executor.rs` are ledgered: count lines after `cargo fmt` in your
  head; the main agent's gate fails on any growth.
- Never print a hold from `check_target_guard` on every tick: log on change only.
- After an agent-side `update-ref` of the checked-out `main`, R's index still holds the old tree
  (`git status` shows the reverse diff staged); after accept, the checkout fast-forward keeps
  that staged deletion. The contracts pass regardless; do not chase it.
