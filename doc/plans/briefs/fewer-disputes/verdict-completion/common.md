# verdict-completion: shared context for every worker

Plan: `doc/plans/PLAN-fewer-disputes.md` (renamed `IN_PROGRESS-...` once running), Decision 11. Read
this file in full, then your own brief. Paths below are relative to `loom/` (the stage's
`working_dir`) unless they start with `../` or `doc/`. Line numbers are advisory: locate every seam
with `loom map --outline <file>` or `loom map --find-all <symbol>` and the quoted snippet. Two
earlier stages of this plan (criteria-authoring, gate-noise-and-review) merged before this one and
edited some of the same files; treat their edits as ordinary code.

## What the stage builds

Today every verdict re-queues the stage and a fresh stage session starts, even when the verdict
changed nothing the agent must act on. After this stage:

1. The verdict apply path still retires the disputing agent first (`Orchestrator::apply_one_verdict`
   -> `retire_disputing_agents`, unchanged).
2. Three verdicts no longer re-queue when they are the last open dispute of the stage: an accepted
   integrity dispute, an accepted impact-tests dispute, and a findings verdict whose rulings are
   non-empty and all close (dismiss or defer). The stage stays `NeedsAdjudication` with no
   session: it is HELD. Every other verdict behaves exactly as today.
3. At the end of every verdict pass, the scheduler sweeps held stages. For each one it submits a
   host job of the new kind `JobKind::VerdictComplete`, bound to the stage (record `session_id`
   is the literal `daemon`), when the stage is a plan v2 standard or integration-verify stage,
   its worktree is committed and clean, and its HEAD equals the head of the stage's latest
   `JobKind::Complete` job record, whose `state` must be `JobState::Passed` (its run passed acceptance
   and every completion check; a failed, cancelled, interrupted or missing record re-queues at once).
   Otherwise it re-queues the stage (`NeedsAdjudication -> Queued`), as today.
4. The host child runs the job exactly as a `Complete` job (`child_complete::run`).
5. Applying the finished job: a pass, after the daemon's completion gates
   (`daemon::check_completion_gates`) and the content binding hold, moves the stage
   `NeedsAdjudication -> Completed` with `completed_commit` set; the monitor then merges it as any
   completed stage. A failure, a gate or binding refusal, a cancel, an interrupt (daemon restart)
   or an application that gives up re-queues the held stage; a failure or refusal also appends its
   failing checks to the stage's adjudicator feedback, which the fresh session's signal shows.

## Decided values (do not reopen)

- Kind: `JobKind::VerdictComplete`, serialized and `name()` `verdict_complete`.
- Record `session_id`: `pub(crate) const STAGE_BOUND_SESSION: &str = "daemon";` in
  `host_verify/verdict_complete.rs`. Nothing in the apply path loads that session.
- Relay request id the job records: `pub(crate) const VERDICT_REQUEST_ID: &str = "verdict-completion";`
  (same file).
- The stage-bound receipt is `stage.completed_commit` plus the job's `applied` marker in
  `verification/<stage>/<job>/`. No completion checkpoint, attempt or receipt is written for any
  session, and the retired session's record and signal are not touched (its retirement already
  happened before the verdict applied).
- Knowledge and knowledge-distill stages are never eligible: `verify::review::gate::covers(stage)`
  (plan v2, standard or integration-verify) is the eligibility test. Reason: the eligible dispute
  kinds only exist where the integrity, impact and review gates run, and a knowledge completion run
  needs the stage's session (`child_complete::verify` calls `knowledge::check_session`).
- A findings verdict with an empty `rulings` list re-queues (it upholds nothing only vacuously).
- Cancelled and interrupted jobs re-queue; they are never resubmitted.
- "Latest completion request" = the `JobKind::Complete` record with the highest
  `(submission_order, queued_at)` among the stage's retained job directories. The store keeps three
  job directories (`store::KEPT_JOB_DIRS`); a pruned request means "no request on record", which
  re-queues.

## Pinned interfaces (W1 writes the first block, W2 the second; both code against all of it)

```rust
// src/models/stage/verdict_completion.rs (W1), `mod verdict_completion;` in models/stage/mod.rs
impl Stage {
    /// `Completed` for every completer except the verdict completion: a stage in adjudication waits for its verdict.
    pub(super) fn try_transition_to_completed(&mut self) -> anyhow::Result<()>;
    /// NeedsAdjudication -> Completed for the daemon's verdict completion; refuses any other status or a stage that names a session.
    pub fn complete_from_verdict(&mut self) -> anyhow::Result<()>;
}

// src/orchestrator/adjudication/requeue.rs (W1), re-exported from adjudication/mod.rs:
//   pub(crate) use requeue::{hand_back_held_stage, is_held_for_completion};
pub(crate) fn is_held_for_completion(work_dir: &Path, stage: &Stage) -> bool;
pub(crate) fn hand_back_held_stage(work_dir: &Path, stage_id: &str, failures: &[String]) -> anyhow::Result<bool>;

// src/orchestrator/adjudication/feedback.rs (W1)
pub fn append_completion_failures(work_dir: &Path, stage_id: &str, failures: &[String]) -> anyhow::Result<()>;
```

```rust
// src/host_verify/mod.rs (W2)
pub enum JobKind { Check, Complete, Freeze, VerdictComplete }   // name(): "verdict_complete"
pub(crate) mod verdict_complete;

// src/host_verify/verdict_complete.rs (W2)
pub(crate) const STAGE_BOUND_SESSION: &str = "daemon";
pub(crate) const VERDICT_REQUEST_ID: &str = "verdict-completion";
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VerdictStart { Waiting(String), Submitted(String), Ineligible(String) }
pub(crate) fn submit_verdict_completion(work_dir: &Path, stage: &Stage, now: DateTime<Utc>) -> anyhow::Result<VerdictStart>;
pub(crate) fn latest_completion_head(work_dir: &Path, stage_id: &str) -> anyhow::Result<Option<String>>;
pub(super) fn apply_verdict_completion(work_dir: &Path, record: &JobRecord) -> anyhow::Result<Applied>;

// src/host_verify/apply.rs (W2): one new variant
Applied::HandedBack(String)   // describe(): "handed the stage to a fresh session: <reason>"

// src/orchestrator/core/verdict_completion.rs (W2), `mod verdict_completion;` in core/mod.rs
impl Orchestrator { pub(super) fn start_verdict_completions(&mut self); }
```

Semantics both workers rely on:

- `is_held_for_completion`: `stage.status == NeedsAdjudication && stage.session.is_none()` and the
  stage has no unapplied open dispute, counted exactly as `requeue_or_hold_for_remaining_disputes`
  counts them today (request published, not closed, no `applied.marker`, no verdict receipt in
  `stage.tally.request_receipts`, not orphaned). Infallible: a dispute directory that cannot be
  listed counts as no disputes, as `dispute_ids` already does.
- `hand_back_held_stage`: takes the stage's dispute lock with
  `fs::dispute_lock::lock_stage_dispute_dir_for_daemon(&work_dir.join("disputes").join(stage_id))`,
  reloads the stage, returns `Ok(false)` changing nothing unless it is still held, appends
  `failures` to the feedback when non-empty, then under `update_stage` moves the stage
  `NeedsAdjudication -> Queued` (re-checking status and session inside the closure) and returns
  `Ok(true)`.

## Lock order and nesting (the bug class this stage can introduce)

- Order is always dispute lock, then stage file lock (`update_stage`). Filing, verdict apply and
  `persist_verdict_result` already follow it.
- `flock` locks taken through two descriptors conflict inside one process. Never call
  `hand_back_held_stage` (which locks) while holding the stage's dispute lock, and never run the
  sweep from inside `apply_one_verdict` while its claim is held: the sweep runs after the loop in
  `Orchestrator::apply_pending_verdicts`, when every claim has dropped.
- The scheduler thread uses the bounded `lock_stage_dispute_dir_for_daemon`, never the blocking
  `lock_stage_dispute_dir`. A lock timeout is an `Err` the next tick retries.

## Concurrency answers (settled during planning; the code must keep them true)

- A new dispute filed while the job runs (an operator can file one: the stage has no session):
  the apply guard sees an open dispute and returns `Applied::Ignored`; that dispute's verdict
  retires agents (`take_down_agents` cancels the stage's host job) and is the one that decides.
- `loom stage retry` / `loom stage reset` while the job runs: both cancel the stage's host job;
  the cancelled job applies against a stage that is no longer held and is `Ignored`.
- Daemon restart mid-job: `recover_after_restart` marks the running job `Interrupted`; startup
  applies it (`report_host_job_recovery`), which re-queues the held stage. A queued job survives
  the restart and runs.
- A crash between persisting the verdict and submitting the job leaves a held stage with no job:
  the sweep runs every tick (and at startup through `initialize_run`), so it submits on the next
  pass. There is no persisted "held" flag to go stale; held is derived from status, session and
  disputes.
- Scheduler view: nothing schedules a `NeedsAdjudication` stage, `recover_orphaned_sessions` never
  resets one, and the run stays alive (`NeedsAdjudication` is not terminal). `loom status` keeps
  showing it as ADJUDICATING with no judge while the job runs (accepted limit, recorded in the
  plan's residual risks).

## Files and size rules

- 400 lines per file, 50 per function, enforced by `cargo test --test maintainability` against the
  exact ledger `loom/maintainability-baseline.txt`. `src/models/stage/methods.rs` is ledgered at
  492 lines: its line count must not change (W1 replaces one line in place).
  `src/host_verify/runner.rs` is at 400: edit its lines in place, add none. Never edit the ledger.
- Every new `.rs` file starts with a `//!` module comment stating what it owns and why.
- Errors: `anyhow` with `.context(...)`, as the touched modules do. No `unwrap` outside tests.
- Logging: `tracing::{info, warn}` with `stage = %id` fields, as `requeue.rs` and
  `verdict_apply.rs` do. Operator lines go through `clear_status_line(); eprintln!(...)`.

## Frozen contract files (written by the contract session; never edit them)

- `src/host_verify/verdict_complete_contract_tests.rs` (scaffold line in `src/host_verify/mod.rs`).
- `src/orchestrator/adjudication/verdict_hold_contract_tests.rs` (scaffold line in
  `src/orchestrator/adjudication/mod.rs`).

Keep the `#[cfg(test)] mod ...;` scaffold lines the contract session added. `loom stage contracts
show verdict-completion` lists the frozen tests; make them pass, never the other way round.

## Expected test-integrity events

These existing assertions change because the behaviour changes, and each raises a `TI-edit` event
the main agent disputes in ONE `loom stage dispute-integrity` (reason: "plan Decision 11: the last
accepted integrity or impact-tests verdict, or a findings verdict that upholds nothing, now holds
the stage in NeedsAdjudication for the daemon's completion run"):

- `src/orchestrator/adjudication/apply_kinds_tests.rs`: `dismiss_ruling_closes_finding`,
  `defer_ruling_carries_finding_to_target`, `integrity_accept_records_counts` assert
  `StageStatus::Queued`; they become `StageStatus::NeedsAdjudication` (W1).
- `src/orchestrator/adjudication/apply_impact_tests.rs`:
  `an_accepted_impact_dispute_records_the_failure_as_judged` likewise (W1).
- `tests/adjudication_e2e/kinds.rs`: `findings_dispute_round_trip` likewise (W1).
- `src/models/stage/transitions.rs`: the `outgoing` bitmask row for `NeedsAdjudication` becomes
  2058 (a `let` array, not an assertion statement; no event expected) (W1).

No other existing assertion changes. A worker that finds one must stop and report it.

## Memory

Record mistakes, decisions and surprises with `loom memory note` / `loom memory decision`
immediately, with `--evidence file:line`. Stale knowledge you notice: `loom memory note
"stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line). Correction: <text>"`.
Known stale after this stage: `doc/loom/knowledge/architecture/adjudication-lifecycle.md`
("Retirement and Fresh-Session Boundary") and
`doc/loom/knowledge/concerns/agent-rule-bending-hardening.md` ("Stage Agents Stop and Report
Instead of Disputing or Blocking") say every verdict starts a fresh session. Never write knowledge
files; never use Claude Code auto-memory.

## Style

No LLMisms (the CLAUDE.md Rule 19 table) in code, comments, test names, messages or doctrine.
Comments explain a non-obvious constraint and name its revisit condition; they never narrate the
change or cite this plan.
