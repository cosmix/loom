# verdict-completion W2: the stage-bound completion job, its application, and the sweep

Tier: opus (`loom-senior-software-engineer`), effort xhigh (daemon lifecycle, crash recovery, lock
ordering). Wave 2: start after W1 reported; its interfaces are in the tree. Read
`doc/plans/briefs/fewer-disputes/verdict-completion/common.md` first, then W1's brief
(`w1-hold-and-handback.md`) for what you call. Frozen contracts you must make pass:
`src/host_verify/verdict_complete_contract_tests.rs` (`loom stage contracts show verdict-completion`).

Knowledge to read first (sections only): `doc/loom/knowledge/mistakes/completion-runner-blocks.md`
(the daemon completes only from its own run: this job is that run);
`doc/loom/knowledge/architecture/adjudication-lifecycle.md` "Retirement and Fresh-Session Boundary".

## Files you own

- `src/host_verify/mod.rs`
- `src/host_verify/child.rs`
- `src/host_verify/child_complete.rs` (module comment only)
- `src/host_verify/runner.rs` (two lines edited in place; the file is at 400 lines)
- `src/host_verify/apply.rs`
- `src/host_verify/apply_rejection.rs`
- `src/host_verify/apply_retry.rs`
- `src/host_verify/verdict_complete.rs` (new)
- `src/host_verify/verdict_complete_tests.rs` (new)
- `src/orchestrator/core/mod.rs` (one `mod verdict_completion;` line after `mod verdict_apply;`)
- `src/orchestrator/core/verdict_apply.rs`
- `src/orchestrator/core/verdict_apply_tests.rs`
- `src/orchestrator/core/verdict_completion.rs` (new)

Read-only: `submit.rs` (no change needed: a `VerdictComplete` request joins only a
`VerdictComplete` job through `covers`' last arm, a `Check` behind it is refused by the existing
`CHECK_BEHIND_RUN` rule, and it supersedes a running check), `wait.rs` (no waiter follows this
kind; `awaits_application` stays `Complete`-only), `store.rs`, everything W1 owns.

## Blast radius of the widened `JobKind` (settled; apply all of it)

Compiler-caught, add the arm: `mod.rs::JobKind::name`, `child.rs::run_job`, `apply.rs::apply_finished`,
`runner.rs::budget_for_job`. SILENT, edit by hand:

- `runner.rs::budget_for_job`: the arm `JobKind::Complete | JobKind::Check =>` becomes
  `JobKind::Complete | JobKind::VerdictComplete | JobKind::Check =>`, and its
  `record.kind == JobKind::Complete,` (the `include_after` flag) becomes
  `record.kind != JobKind::Check,`. Without the second edit the job gets a check's deadline and
  times out on `after_stage` and wiring tests. Both edits keep the line count at 400.
- `apply_retry.rs::give_up`: a `VerdictComplete` job whose application gave up must re-queue the
  held stage, or the stage waits forever (step 4).
- No other consumer compares kinds: `rg -n 'JobKind::' src` lists `wait.rs`, `submit.rs`,
  `status/data/contract_testing.rs`, `stage_service/contracts.rs` and test fixtures, none of which
  needs this kind. The web client never sees job kinds.

## Step 1. The kind and the child

1. `mod.rs`: add `VerdictComplete` after `Freeze`, `Self::VerdictComplete => "verdict_complete"`;
   the `JobKind` and `name()` doc comments name all four kinds ("... or the completion the daemon
   runs for a stage a verdict left held"). Declare `pub(crate) mod verdict_complete;` with the
   other modules. Keep the contract scaffold line `#[cfg(test)] mod verdict_complete_contract_tests;`.
2. `child.rs::run_job`: `JobKind::Complete => child_complete::run(work_dir, &record),` becomes
   `JobKind::Complete | JobKind::VerdictComplete => child_complete::run(work_dir, &record),`
   (keep that exact text on one line). Mention the kind in the module comment's job list, in place.
   The child is unchanged otherwise: for a non-knowledge stage `child_complete::verify` never reads
   `stage.session` or `record.session_id`.
3. `child_complete.rs`: the module comment's first line names both kinds.
4. `runner.rs`: the two in-place edits above.

## Step 2. `verdict_complete.rs`: submission

Constants and `VerdictStart` as `common.md` pins them.

`submit_verdict_completion(work_dir, stage, now)`, in this order:

1. A job already decides the stage: `store::load_current` is a `VerdictComplete` record that is
   either not terminal, or terminal with no `apply::APPLIED_MARKER` file
   (`store::load_job_file`). Return `Waiting(id)`; never queue a second job.
2. `!crate::verify::review::gate::covers(stage)` -> `Ineligible("stage '<id>' is not a plan v2
   standard or integration-verify stage")`.
3. `latest_completion(work_dir, &stage.id)?` (the newest `Complete` record, defined below) is `None`
   -> `Ineligible("no `loom stage complete` request of the stage is on record")`. Its `state` is not
   `JobState::Passed` (the record's own `state` field in `host_verify/mod.rs`, set when the child's
   run passed acceptance and every completion check) -> `Ineligible("the stage's latest `loom stage
   complete` run did not pass")`. A failed, cancelled, interrupted or missing newest record re-queues
   at once: no VerdictComplete job is submitted. The daemon-applied gates (step 3a) still refuse a
   passed run later, as before.
4. Repo root: `crate::fs::work_dir::WorkDir::new(work_dir)?.main_project_root().context(...)`.
   `crate::handoff::completion::content::committed_content(stage, &repo_root)`: an `Err` ->
   `Ineligible(format!("the stage's worktree is not committed and clean: {error:#}"))` (it also
   rejects untracked files and a checkout off the stage branch).
5. The head it returns differs from the requested head -> `Ineligible(format!("HEAD {head} is
   not {requested}, the commit its latest completion request named"))`.
6. Tree digest through pinned git, as the child computes it: `crate::git::worktree::stage_worktree_path(stage, &repo_root)?`,
   `WorktreeGit::pinned(&repo_root, &worktree)?`, `digest::tree_digest_in(&git)?`.
7. `submit::submit` with `SubmitRequest { stage_id, session_id: STAGE_BOUND_SESSION, request_id:
   VERDICT_REQUEST_ID, kind: JobKind::VerdictComplete, head, tree_digest, no_cache: false,
   criteria: None }`. `Queued(job)` / `Joined(job)` -> `Submitted(job.id)`;
   `Superseded { queued, .. }` -> `Submitted(queued.id)`; `Refused(reason)` -> `Ineligible(reason)`
   (a refusal is deterministic; re-queueing beats retrying it every tick).

`latest_completion(work_dir, stage_id) -> Result<Option<JobRecord>>`: `Ok(None)` when `store::stage_dir(work_dir,
stage_id)?` is not a directory; else load every `store::job_ids` record with
`store::load_job_record`, keep `kind == JobKind::Complete`, take the max by
`(submission_order, queued_at)`, return that record whatever its `state`. An unreadable record is
skipped with a `tracing::warn!`, as `apply_retry::unapplied_of` does. `latest_completion_head` is
`latest_completion(..)?.map(|record| record.head)` (the pinned signature); submission does not call
it, so it carries `#[cfg(test)]`. `VerdictComplete` and `Check` records never
count: the precondition is about the agent's own `loom stage complete`.

## Step 3. `verdict_complete.rs`: application

`apply.rs::apply_finished` gains `JobKind::VerdictComplete => verdict_complete::apply_verdict_completion(work_dir, record)?,`
(exactly that text; import `verdict_complete` in the existing `use super::{..}` list).
`apply_finished` already ignores a non-terminal record and writes the `applied` marker after the
arm returns; do not write it yourself.

`apply_verdict_completion(work_dir, record)`:

1. `load_stage`. A `Completed` stage: `Ignored("already completed")` when
   `completed_commit == Some(record.head)`, else `Ignored("the stage was completed from other content")`.
   Nothing else to settle: there is no session.
2. `!is_held_for_completion(work_dir, &stage)` -> `Ignored("the stage is no longer held for its
   verdict completion")`. This covers a dispute filed meanwhile, a retry, a reset, a session.
3. The stage's current job is a different, non-terminal record -> `Ignored(format!("host job {id}
   decides the stage"))`.
4. By state: `Passed` -> step 3a; `Failed` -> hand back with `failure_lines`; `Cancelled` and
   `Interrupted` -> hand back with no failures (a plain re-queue, as today).

3a. Passed:
- `store::load_result(..)?.with_context(..)?` (a passed job without a result is an `Err`, retried).
- `crate::daemon::check_completion_gates(work_dir, &record.stage_id)` (integrity and review gates,
  session-independent): an `Err` -> hand back with `format!("completion gates: {error:#}")`.
- In an inner block: take `crate::fs::dispute_lock::lock_stage_dispute_dir_for_daemon(&work_dir.join("disputes").join(&record.stage_id))?`,
  then `update_stage(&record.stage_id, work_dir, |stage| bind_and_complete_held(stage, work_dir, record, &result))`.
  The claim drops at the end of the block, BEFORE any hand-back call.
- `bind_and_complete_held`: `is_held_for_completion(work_dir, stage)` re-checked under both locks
  (else `binding_failed("the stage left its verdict hold before completion was applied")`); then
  the shared run binding (below); then `stage.completed_commit = Some(result.head.clone())` and
  `stage.complete_from_verdict()`.
- `Ok` -> `tracing::info!` with stage and commit, `Applied::Completed { commit: result.head }`.
  A `BindingFailed` error -> hand back with its reason (`binding_reason`); any other error -> `Err`.

Shared run binding: extract from `apply.rs::bind_and_complete` the checks after its session test
(`result.passed && result.head == record.head`, `VerificationSnapshot::capture` commit equals the
head, `result.policy_hash` equals the snapshot's policy) into
`pub(super) fn verify_bound_run(stage: &Stage, work_dir: &Path, record: &JobRecord, result: &JobResult) -> Result<()>`
in `apply.rs`, and call it from both `bind_and_complete` and `bind_and_complete_held`. Make
`binding_failed` and `binding_reason` `pub(super)`. Do NOT reuse `check_outside_lock`,
`validate_active_identity`, `complete_stage` or `settle_completed`: they require `Executing` and a
session that owns the stage, take the sessions-directory lock, retire the session and write the
session's checkpoint receipt. A held stage has no session, and the retired one must not gain a
receipt for a run it never asked for.

Hand back: `hand_back(work_dir, record, failures) -> Result<Applied>` calls
`crate::orchestrator::adjudication::hand_back_held_stage(work_dir, &record.stage_id, &failures)?`;
`true` -> `Applied::HandedBack(<first failure, else "the run was <state>">)`; `false` ->
`Ignored("the stage is no longer held for its verdict completion")`.

`failure_lines(work_dir, record) -> Result<Vec<String>>`: the result's outcomes, else
`store::read_progress` (as `apply_rejection::record_failure` falls back); one line per failed
outcome, `"<label>: <first non-empty summary line>"`; then the result's `error`, else
`record.detail`, when present; `"the run failed without naming a check"` when empty. Share the
"first non-empty summary line" step with `apply_rejection.rs::first_failure` by extracting it there
as `pub(super) fn first_summary_line(summary: &str) -> &str`. Sanitising and capping is W1's
`append_completion_failures`; do not repeat it.

`apply.rs`: add `Applied::HandedBack(String)` with a doc comment ("the stage went back to a fresh
session, for the reason given") and its `describe` arm; update the module comment. Keep the file
under 400 lines.

## Step 4. Giving up

`apply_retry.rs::give_up`: before writing the marker, `if record.kind == JobKind::VerdictComplete
{ verdict_complete::give_up(work_dir, record, reason)?; }`. In `verdict_complete.rs`,
`pub(super) fn give_up(work_dir, record, reason) -> Result<()>` hands the stage back with
`format!("loom could not apply its completion run: {}", run_output_line(reason))`
(`verify::contracts::refusal::run_output_line`, as `give_up` already uses) when step 3's guards 2
and 3 hold, and does nothing otherwise. An `Err` propagates, so the marker is not written and the
next tick retries the hand-back, as `record_freeze_failure` does.

## Step 5. The sweep (`orchestrator/core/verdict_completion.rs`, `verdict_apply.rs`)

- `verdict_apply.rs::apply_pending_verdicts`: after the `for` loop (every claim has dropped), call
  `self.start_verdict_completions();` (exactly that text). Not on the early return for a failed
  scan. Update the method's doc comment: it ends by starting the completion of every held stage.
- `verdict_completion.rs`, `impl Orchestrator`:
  - `pub(super) fn start_verdict_completions(&mut self)`: `crate::verify::transitions::list_all_stages(&work_dir)`
    (a failure is a `tracing::warn!` and a return); for each stage with status
    `NeedsAdjudication` and `is_held_for_completion`, call a private per-stage helper. Use the stage
    files, never `self.graph`: the graph's statuses are synced earlier in the tick, before this
    pass applied the verdicts.
  - Per stage: `submit_verdict_completion(&work_dir, &stage, Utc::now())` (import it with `use`).
    `Waiting` -> nothing. `Submitted(id)` -> `clear_status_line(); eprintln!("Stage '{id}': the
    verdict changed nothing in the plan; loom runs its completion on the committed tree (host job
    {job})")`. `Ineligible(reason)` -> `hand_back_held_stage(&work_dir, &stage.id, &[])`; on
    `Ok(true)` print `"Stage '{id}': {reason}; a fresh session starts with the verdict"`. Any
    `Err` -> `tracing::warn!` and the next tick retries.
- Because `initialize_run` calls `apply_pending_verdicts` after `report_host_job_recovery`, the
  sweep also runs at startup, after interrupted jobs were applied. Do not add a second call site.

## Step 6. Tests

`src/host_verify/verdict_complete_tests.rs`, declared from `verdict_complete.rs` with
`#[cfg(test)] #[path = "verdict_complete_tests.rs"] mod tests;` (the `apply.rs` precedent). Use
`crate::host_verify::tests_complete_support::{Fixture, STAGE}` and `store`. Hold a fixture stage
with `update_stage(STAGE, &fx.work_dir, |s| { s.status = StageStatus::NeedsAdjudication; s.session = None; Ok(()) })`.
The contracts cover the headline cases; add:

- `latest_completion_head` picks the newest `Complete` by submission order and ignores `Check`
  and `VerdictComplete` records; returns `None` with no `verification/<stage>` directory.
- A newest `Complete` record that failed acceptance requeues at once and submits no VerdictComplete
  job: on a held plan v2 stage whose newest `Complete` record has `state: JobState::Failed` and
  HEAD equal to the worktree, `submit_verdict_completion` is `Ineligible` and `store::load_current`
  finds no `VerdictComplete` record.
- Submission on a plan v1 stage and on a knowledge stage is `Ineligible` and queues nothing.
- A passed job on a held plan v2 stage with no review round: the gates refuse, the stage is
  `Queued`, the feedback names `completion gates`.
- A passed job whose HEAD moved after the run (`fx.commit_empty()` before applying): `HandedBack`,
  stage `Queued`, feedback names the moved commit.
- A passed job while another non-terminal job is the stage's current one: `Ignored`, stage still
  `NeedsAdjudication`.
- A passed job applied twice (marker removed between): the second is `Ignored("already completed")`.
- `runner::budget_for_job` gives a `VerdictComplete` record the same seconds as a `Complete` one
  for the same stage.
- `apply_retry` giving up on a `VerdictComplete` job re-queues the held stage: write the job's
  `apply-attempts` file as `b"10"` with `store::save_job_file` and call
  `apply_retry::apply_record`, as `src/orchestrator/core/host_jobs_tests.rs` does around its
  `save_job_file(.., "apply-attempts", b"10")` line.
`src/orchestrator/core/verdict_apply_tests.rs` (keep it under 400 lines; reuse its
`orchestrator_for` and `stage_needing_adjudication`): a held plan v1 stage is re-queued by
`apply_pending_verdicts` with nothing to apply; a `NeedsAdjudication` stage with an open dispute
request and no verdict is left as it is.

## Traps

- Lock nesting: `hand_back_held_stage` locks the dispute directory; call it only after the inner
  block that holds the claim has ended. A second `flock` on another descriptor in the same process
  waits for the first.
- `submit` prunes old job directories (`KEPT_JOB_DIRS` = 3). Read `latest_completion_head` before
  you submit, never after.
- `Fixture::record` saves `submission_order: 0`; order ties fall back to `queued_at`.
- `apply_finished` is re-entered after a crash between the stage write and the marker: step 3.1's
  `Completed` branch is that resume; it must not hand back or error.
- `runner.rs` must stay at 400 lines; `child.rs` and `apply.rs` under 400.

## Done

Report the files changed, the final signatures and anything unresolved. Your one check, once,
after both W1 and you have written everything: `cargo test --lib host_verify::verdict_complete`.
Do not run the full suite, clippy or fmt.
