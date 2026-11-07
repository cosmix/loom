# verdict-completion W1: the held stage, its hand-back, and the new transition edge

Tier: opus (`loom-senior-software-engineer`), effort high. Wave 1, beside W3. W2 starts after you
report, coding against the interfaces you write, so write them exactly as `common.md` pins them.
Read `doc/plans/briefs/fewer-disputes/verdict-completion/common.md` first. Frozen contracts you
must make pass: `src/orchestrator/adjudication/verdict_hold_contract_tests.rs` (`loom stage
contracts show verdict-completion`).

Knowledge to read first (sections only): `doc/loom/knowledge/architecture/adjudication-lifecycle.md`
"Retirement and Fresh-Session Boundary"; `doc/loom/knowledge/mistakes/adjudication-autonomy-deadlock.md`
"The Disputing Agent Was Never Retired" and "Escalation Left the Disputing Agent Alive, and Approve
Re-attached It". The rule they record: every exit from `NeedsAdjudication` happens after the agents
that no longer own the stage are gone. Your hold keeps that true because the stage never leaves
`NeedsAdjudication` until W2's job decides, and the agent was retired before the verdict applied.

## Files you own

- `src/models/stage/mod.rs` (one `mod verdict_completion;` line)
- `src/models/stage/transitions.rs`
- `src/models/stage/methods.rs` (ONE line replaced in place; its line count stays at its ledger row)
- `src/models/stage/merge_block.rs` (one line replaced in place)
- `src/models/stage/verdict_completion.rs` (new, with its unit tests in a `#[cfg(test)] mod tests`)
- `src/orchestrator/adjudication/mod.rs`
- `src/orchestrator/adjudication/apply_kinds.rs`
- `src/orchestrator/adjudication/requeue.rs`
- `src/orchestrator/adjudication/feedback.rs`
- `src/orchestrator/adjudication/verdict_hold_tests.rs` (new)
- `src/orchestrator/adjudication/apply_kinds_tests.rs` (three assertions)
- `src/orchestrator/adjudication/apply_impact_tests.rs` (one assertion)
- `tests/adjudication_e2e/kinds.rs` (one assertion)

Read-only for you: everything else, including `src/orchestrator/adjudication/apply.rs`
(`persist_verdict_result`), `apply_receipt.rs`, `closed_disputes.rs`, `src/stage_service/receipts.rs`.

## Step 1. The transition edge and its guards (`models/stage`)

1. `transitions.rs::outgoing_edges`: add `Completed` to the slice the `NeedsAdjudication` row
   holds, keeping every target already there (`&[Queued, NeedsHumanReview]` at `e6680a9d`, so
   `&[Queued, Completed, NeedsHumanReview]` there; a sibling plan may have added `Blocked`). Never
   copy a literal slice. In `exhaustive_policy_tests`, the `outgoing` entry for
   `NeedsAdjudication` (the last one) becomes its current value + 8 (bit 3 is `Completed`; 2050
   becomes 2058 at `e6680a9d`). Change nothing else in that test.
2. New `src/models/stage/verdict_completion.rs` with the two methods `common.md` pins:
   - `try_transition_to_completed`: `bail!` when `self.status == StageStatus::NeedsAdjudication`
     with `stage '<id>' is awaiting adjudication; only the completion run after its verdict
     completes it`; otherwise `self.try_transition(StageStatus::Completed)`.
   - `complete_from_verdict`: `ensure!` status is `NeedsAdjudication` (message names the status)
     and `self.session.is_none()` (message names the session); then
     `self.try_transition(StageStatus::Completed)?`, `self.close_reason = None`,
     `self.stamp_completed()` (it is `pub(super)` in `methods.rs`, reachable from this sibling).
3. `methods.rs::try_complete`: replace exactly the line `self.try_transition(StageStatus::Completed)?;`
   with `self.try_transition_to_completed()?;`. Run `wc -l src/models/stage/methods.rs`: it must
   still print the count of its `loom/maintainability-baseline.txt` row (492 at `e6680a9d`).
4. `merge_block.rs::try_complete_merge`: replace its `self.try_transition(StageStatus::Completed)?;`
   with `self.try_transition_to_completed()?;`.

Why the guards (Blast Radius result): `commands/stage/progressive_complete/finish.rs::record_completion`
documents that it relies on the table refusing `NeedsAdjudication -> Completed` ("if a dispute moved
the stage to NeedsAdjudication meanwhile, it refuses"); `complete_pipeline.rs::complete_without_verification`,
`knowledge_complete.rs` and `commands/stage/merge/landing.rs::prove_and_mark_merged` reach the same
completers. Routing both generic completers through the guard keeps every one of them refusing, so
the new edge is taken only by `complete_from_verdict`. `orchestrator/core/merge_handler/landing.rs::apply_finalized`
calls `try_transition(Completed)` directly but only from merge states, and `host_verify/apply.rs::bind_and_complete`
requires `Executing`; leave both.

Acceptance of step 1: unit tests in `verdict_completion.rs` show `try_complete` and
`try_complete_merge` refuse a `NeedsAdjudication` stage and still complete an `Executing` one;
`complete_from_verdict` refuses `Executing`, refuses a `NeedsAdjudication` stage with
`session: Some(..)`, and completes a sessionless `NeedsAdjudication` stage with `completed_at` set
and `close_reason` cleared.

## Step 2. The held stage (`adjudication/requeue.rs`, `apply_kinds.rs`, `mod.rs`)

1. `requeue.rs`: extract the dispute count inside `requeue_or_hold_for_remaining_disputes` into a
   private `fn unapplied_open_disputes(work_dir: &Path, stage: &Stage) -> usize` (the same
   iterator and filter, moved, not copied). Extract the "hold while siblings remain" tail (the
   `tracing::info!` plus `force_status_with_reason` when not already `NeedsAdjudication`) into a
   private helper so both callers below share it.
2. Add `pub(super) fn hold_for_completion_or_remaining_disputes(work_dir: &Path, stage: &mut Stage) -> Result<()>`:
   identical to `requeue_or_hold_for_remaining_disputes` (the `NeedsHumanReview` early return and
   its warning, the `ensure!(all_unapplied > 0, ...)`, the sibling hold) except that when no
   sibling remains it leaves the stage in `NeedsAdjudication` and logs
   `tracing::info!(target: "loom::adjudication", stage = %stage.id, "holding stage for the daemon's completion run")`.
   It changes no field: `persist_verdict_result` (`apply.rs`) then records only the verdict
   receipt, so the stage on disk is `NeedsAdjudication`, sessionless, with every dispute applied.
3. Add `is_held_for_completion` and `hand_back_held_stage` as `common.md` pins them, on a
   private `fn verdict_held(work_dir: &Path, stage: &Stage) -> bool` (`NeedsAdjudication`, no
   session, `unapplied_open_disputes == 0`). `is_held_for_completion` is
   `verdict_held(..) && !stage.held`: `Stage.held` is the operator's `loom stage hold`, and a
   stage the operator holds is never completed by the daemon. `hand_back_held_stage` guards on
   `verdict_held` alone, so the sweep can hand back a stage the operator holds; it never touches
   `held`. In `hand_back_held_stage`, the closure passed to
   `crate::verify::transitions::update_stage` re-checks `s.status == NeedsAdjudication &&
   s.session.is_none()` and calls `s.try_transition(StageStatus::Queued)` only then; the function
   returns whether the saved stage is `Queued`. Write the feedback (step 3) before the stage
   update, inside the lock. Log `tracing::info!` with the stage id and the failure count when it
   moves the stage.
4. `adjudication/mod.rs`: `mod requeue;` stays private; add
   `pub(crate) use requeue::{hand_back_held_stage, is_held_for_completion};` beside the other
   re-exports, and `#[cfg(test)] mod verdict_hold_tests;` beside the other test modules. Keep the
   contract scaffold line `#[cfg(test)] mod verdict_hold_contract_tests;`.
5. `apply_kinds.rs`: add `fn notify_and_hold(work_dir, stage, body) -> Result<()>`
   (`feedback::write_notice`, then `hold_for_completion_or_remaining_disputes`) beside
   `notify_and_requeue`, and call it at exactly three sites:
   - `apply_integrity_accept`: its final `notify_and_requeue(work_dir, stage, &body)`.
   - `apply_by_kind`, the `DisputeKind::ImpactTests` arm of `DisputeVerdict::Accept`.
   - `apply_rulings`: `notify_and_hold` when `!rulings.is_empty() && rulings.iter().all(|r| r.ruling.closes())`,
     else `notify_and_requeue` (the `refused_defer` early return stays first and untouched).
   Every other arm (contract accept and reject, integrity reject, impact reject, freeze-refusal,
   criterion verdicts through `apply.rs` and `apply_accept.rs`) keeps `notify_and_requeue` or its
   current path. Update the module comment's bullet list to say which verdicts hold.

Do NOT copy from `requeue_or_hold_for_remaining_disputes` the call to `transition_to_queued` into
the hold path, and do not add a stage field or marker file for "held": held is derived (common.md,
"Concurrency answers").

Acceptance of step 2: once W2 has landed, `cargo test --lib orchestrator::adjudication::` passes,
including the frozen `verdict_hold_contract_tests` and your `verdict_hold_tests`. Until then the
library's test target cannot compile at all: the frozen `host_verify` contract file names W2's
items. The library itself (`cargo check --lib`, no test modules) must compile with your code alone.

## Step 3. The failure section (`feedback.rs`)

`append_completion_failures(work_dir, stage_id, failures)` appends to the stage's `feedback.md`
(create it when missing; never overwrite the verdict's notice) exactly:

```text

### Completion after the verdict

The verdict changed nothing in the plan, so loom ran the stage's completion on the committed tree without a session. It failed:

- <failure 1>
- <failure 2>

Action: fix these failures, commit, then run `loom stage complete`.
```

Each failure line is `crate::verify::review::report::single_line(failure)` cut with
`crate::models::dispute::truncate_to_byte_limit(.., 300)`; at most 20 lines, then one line
`- and <n> more; \`loom stage complete\` lists every failure`. The text comes from command output
(job summaries); the bullet prefix and `single_line` keep a line such as `## heading` or an escape
sequence from forming a heading or reaching the terminal raw. Add the unit tests beside the
existing ones at the bottom of `feedback.rs`: the notice survives the append; a failure holding
`"## x\u{1b}[2J"` renders as one bullet with no control character and no line starting `## `;
25 failures render 20 bullets and the "and 5 more" line.

## Step 4. Existing tests whose expectation changes

Change only the status expectation named in `common.md` ("Expected test-integrity events"):
`apply_kinds_tests.rs` (`dismiss_ruling_closes_finding`, `defer_ruling_carries_finding_to_target`,
`integrity_accept_records_counts`), `apply_impact_tests.rs`
(`an_accepted_impact_dispute_records_the_failure_as_judged`), `tests/adjudication_e2e/kinds.rs`
(`findings_dispute_round_trip`): `StageStatus::Queued` becomes `StageStatus::NeedsAdjudication`.
Leave every other assertion, including the reject tests, which still expect `Queued`.

## Step 5. `verdict_hold_tests.rs` (your unit tests; the contracts cover the headline cases)

Use `super::tests::{make_stage, write_stage, write_request, write_verdict, reject_verdict}` as
`apply_kinds_tests.rs` does.

- An impact-tests accept as the only dispute holds the stage (`NeedsAdjudication`,
  `is_held_for_completion` true).
- An integrity accept on dispute 1 while dispute 2's request is open: stage `NeedsAdjudication`,
  `is_held_for_completion` false; then an integrity accept on dispute 2: held.
- An integrity accept on dispute 1, then a contract reject on dispute 2: the stage ends `Queued`
  (the last verdict decides).
- A findings verdict with `rulings: vec![]` re-queues.
- `hand_back_held_stage` on a held stage with two failures: returns true, stage `Queued`, the
  feedback holds the verdict notice followed by the two bullets.
- `hand_back_held_stage` returns false and changes neither stage nor feedback for: a stage with an
  open dispute, a stage naming a session, a `Queued` stage.
- A verdict-held stage with `held: true` (the operator's hold): `is_held_for_completion` is
  false; `hand_back_held_stage` with no failures returns true and leaves it `Queued` with `held`
  still true.

## Traps

- `flock` through a second descriptor in the same process blocks: `hand_back_held_stage` takes the
  dispute lock itself, so nothing that already holds it may call it (common.md, "Lock order").
- `persist_verdict_result` re-applies only verdict-owned fields onto the fresh stage; the hold
  must therefore be expressed purely as "status unchanged". Do not set `review_reason` or any other
  field in the hold path.
- `dispute_is_orphaned` excludes requests no filing published; keep using it through the shared
  count so a crashed filing never holds a stage forever.
- `methods.rs` line count: exactly its ledger row (492 at `e6680a9d`), or
  `cargo test --test maintainability` fails.

## Done

Report the files changed, the final signatures, and anything left unresolved. Your one check:
`cargo check --lib`, once (the test target waits for W2; the main agent runs the tests). Do not
run the full suite, clippy or fmt.
