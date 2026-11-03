# stall-recovery W2: a resolved merge is verified in a host job, never on the scheduler thread

Tier: opus, effort xhigh (daemon lifecycle, merge landing, restart recovery). Wave 1, in
parallel with C1. Read `doc/plans/briefs/autonomous-recovery/common.md` first, then knowledge
`architecture/orchestrator-loop.md` and `patterns/orchestrator-daemon-loop.md`. Frozen
contracts: `src/host_verify/land_contract_tests.rs`.

## Why (the stall's root cause)

Plan 05's scheduler showed no tick for 187 s "stuck in sync, during merge resolution". Two
orchestrator-thread call sites run a stage's WHOLE completion verification inline:
`Orchestrator::resolve_merge_from_inbox` (`orchestrator/core/inbox_drain/merge_resolved.rs`,
in the sync phase's `drain_session_inboxes`) and `land_resolved_worktree`
(`orchestrator/core/merge_handler/resolver_exit.rs`, in the events phase). Both call
`commands::stage::complete::verify_resolved_completion` (`complete_resolved.rs`): acceptance
plus every completion check, for integration-verify the full test suite, on the scheduler
thread. Every other completion already runs in a host job (`host_verify`, `JobKind::Complete`,
polled by `Orchestrator::tick_host_jobs`).

## Interface (pinned)

- `crate::host_verify::JobKind::Land`, serialized `land` (`name()` returns `"land"`), after the
  live-test-lane plan's `JobKind::Live`.
- `pub(crate) fn submit_land(work_dir: &Path, stage_id: &str, session_id: &str, head: &str,
  tree_digest: &str, now: DateTime<Utc>) -> Result<String /* job id */>` in a new
  `src/host_verify/land.rs`: submits a `Land` job through the existing `submit` (one job per
  stage: a queued or running `Land` job for the stage is joined and its id returned).
- The child (`host_verify/child.rs` dispatch) runs `land::run_child(work_dir, &record)`: the
  body of `verify_resolved_completion` without its final `update_stage`: resolved-worktree
  check, snapshot capture, acceptance, `run_checks` with `daemon_gates: false, fail_closed:
  false`, each outcome recorded under its label as `child_complete.rs` does.
- Applying (`host_verify/apply.rs`): a passing `Land` job, for a stage still `MergeConflict` or
  `MergeBlocked`, validates the snapshot under the lock (`snapshot.validate_locked`) and sets
  `completed_commit = head`, returning a new `Applied::Landable { stage_id }`; a failing one
  returns `Applied::LandFailed { stage_id, summary }` naming the failing labels; a stage that
  left merge resolution returns `Applied::Ignored`.

## Edits (you own these files)

1. `src/host_verify/mod.rs`, `child.rs`, `apply.rs`, `submit.rs`, `wait.rs`, `store.rs`, new
   `land.rs`: the interface above. `submit.rs`'s session rule: a `Land` job's `session_id` is
   the merge resolver's session when one asked, else the literal `daemon`; the rule that a
   `Complete` job belongs to the stage's live session does not apply to `Land`. `wait.rs`:
   sessions never wait on a `Land` job (`site.kind == JobKind::Complete` keeps excluding it).
   Every exhaustive match on `JobKind` gets the arm.

## Beside the live plan's `JobKind::Live` (live-checks R1, merged before this plan)

R1 added `Live` to `JobKind` and `name()` and declared `mod child_live;` in `mod.rs`; added
`JobKind::Live => child_live::run_live_job(work_dir, &record),` to `child.rs::run_job`;
`JobKind::Live => Applied::Ignored(..)` to `apply.rs`; and in `submit.rs` refused a `Live`
request behind a running `Complete` or `Freeze` job, joined a `Live` request only to a `Live`
job, and never lets a `Complete` cover a `Live` request. `wait.rs` and `store.rs` got nothing.
Place every `Land` arm right after the `Live` arm:

- `mod.rs`: `Land` after `Live`, `Self::Land => "land"`, `pub(crate) mod land;` beside
  `mod child_live;` (the orchestrator calls `land::submit_land`),
  and the `JobKind` and `name()` doc comments name all five kinds.
- `child.rs` is at most 400 lines after R1 (`wc -l`). Its only new line is
  `JobKind::Land => land::run_child(work_dir, &record),` (with `land` in the existing
  `use super::{..}` list); take that line back by condensing a doc comment in `child.rs`, as R1
  did. Everything else of the child lives in `land.rs`.
- `apply.rs`: `JobKind::Land => land::apply_land(work_dir, record)?,`; the `Landable` and
  `LandFailed` bodies live in `land.rs`.
- `submit.rs`: a `Check` or `Live` request behind a non-terminal `Land` job is refused with the
  message a check gets (a `Land` job is never superseded by a check or a live run); a `Land`
  request supersedes a running `Check` or `Live` job; `covers` joins a `Land` request only to a
  `Land` job.
2. `src/orchestrator/core/inbox_drain/merge_resolved.rs`: after `check_resolved_worktree`,
   call `submit_land` and settle `Applied("verification job <id> started; the merge lands when
   it passes")`; no inline verification, no `land_stage_merge` here.
3. `src/orchestrator/core/merge_handler/resolver_exit.rs` (`land_resolved_worktree`): a
   `Land` job queued or running for the stage: return (its application lands the merge). The
   stage's `completed_commit` already equals the resolved head (a `Land` job passed while the
   resolver ran): land now (`land_stage_merge`, then `cleanup_resolved_merge`). Otherwise
   submit a `Land` job and return.
4. `src/orchestrator/core/host_jobs.rs` (`tick_host_jobs`): on `Applied::Landable`, call
   `self.land_stage_merge(stage_id, &target)` and, when no merge-resolver session for the stage
   is live, `self.cleanup_resolved_merge`; on `Applied::LandFailed`, call
   `self.route_merge_stage_to_review(stage_id, "resolved content failed completion
   verification: <summary>; fix the failure and run `loom stage complete <id>` before landing",
   None)` exactly as `land_resolved_worktree` reports a failed verification today. Keep the
   file under 400 lines (it is 249).
5. `src/orchestrator/core/merge_handler/resolver_spawn.rs` (or wherever
   `spawn_merge_resolution_sessions` decides a stage needs a resolver; find it with
   `loom map --find-all spawn_merge_resolution_sessions`): a `MergeConflict` or `MergeBlocked`
   stage with a queued or running `Land` job gets no resolver; one whose worktree already
   passes `check_resolved_worktree` and has no live resolver and no `Land` job gets a `Land`
   job instead of a counted resolver (the recovery after a daemon restart dropped a job).
6. `src/orchestrator/signals/merge.rs`: the resolver's instructions say the merge lands after
   loom verifies the resolved content in a host job, and that `loom stage merge --resolved`
   returns once that job starts. The file is 397 lines after the live plan: reword step 5 in
   place, keep it at or under 400, and keep the live plan's
   `merge_live::format_live_section(stage)` call after the task section, whose `## Live
   Entries` text has the resolver run `loom check --live <stage>` before `--resolved`.
7. `src/commands/stage/complete_resolved.rs` stays for the operator's `loom stage merge`
   (`commands/stage/merge.rs`), which runs on the operator's host, not the scheduler thread.
   After your edits, `rg -n 'verify_resolved_completion' src/orchestrator` finds nothing.

## Corner cases to handle and test

- A second `merge --resolved` while the job runs joins the same job.
- The resolver commits again after the job started: the job's result names the old head; apply
  compares `head` with the branch tip and treats a moved tip as `LandFailed` ("the stage
  branch moved during verification; it is verified again").
- The daemon restarts mid-job: `host_verify::runner::recover_after_restart` settles the job;
  step 5 resubmits on the next tick when the worktree is resolved and nobody else will.
- A passing job whose landing is `Held` / `Conflict` / `Blocked`: the existing landing
  reporting applies; nothing is retried in a loop (the merge gate records why it held).
- `run_checks` holds the live plan's `live` receipt check, so a Land job runs it as
  `verify_resolved_completion` does. The live plan's merge signal has the resolver of a stage
  with required live entries record a receipt for the resolved content with
  `loom check --live <stage>` (a `Live` job) before `--resolved`, so that check passes. A
  receipt bound to the pre-merge content, or none (the resolver skipped the run, or its
  `--resolved` superseded the `Live` job still running), fails as `live`, and the job is
  `LandFailed` like any failing label. Keep that behaviour; no special case.

## Done

Your one check after the stage compiles: `cargo test --lib host_verify::`.
