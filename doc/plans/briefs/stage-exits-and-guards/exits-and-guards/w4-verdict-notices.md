# exits-and-guards / w4-verdict-notices: every verdict of a round reaches the next session

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Findings

- A session filed a contract dispute and, eight seconds later, an integrity dispute. Both were
  judged, one after the other. `.loom/work/disputes/<stage>/feedback.md` then held only the
  second verdict's notice, and the next session's signal section "## Adjudicator Feedback (from
  your prior dispute)" showed only that. The first notice was the one that said the daemon had
  written an accepted replacement into the worktree, uncommitted. The next session found a
  modified frozen file it could not explain.
  Cause: `append_rejection`, `append_questions` and `write_notice` in
  `loom/src/orchestrator/adjudication/feedback.rs` each call `fs::write`, which truncates.
  `clear_feedback` is documented as "called once the agent's follow-up dispute is filed" and has
  one caller, the amendment-cap escalation; nothing calls it at a filing.

(The stage file also archived the first dispute's reason as superseded; worker
w4b-adjudication-reason fixes that in `models/stage/issue_history.rs`.)

## Decisions

- A round is every dispute filed between one entry into `NeedsAdjudication` and the requeue that
  follows the last verdict. The feedback file holds every notice of the current round, in the
  order the verdicts were applied.
- The file is cleared when a filing takes the stage INTO `NeedsAdjudication` (the first filing
  of a round) and never by a later filing of the same round. A verdict is applied only to a
  stage already in `NeedsAdjudication`, so no notice of the round can precede that clear.

## Files

Own (write):
- `loom/src/orchestrator/adjudication/feedback.rs`
- `loom/src/models/dispute.rs`
- `loom/src/stage_service/dispute.rs`
- `loom/src/stage_service/dispute_kinds.rs`

Read only: `loom/src/orchestrator/adjudication/apply.rs`, `apply_kinds.rs`,
`apply_contract_rulings.rs`, `requeue.rs` (the callers of the writers; do not edit them),
`loom/src/orchestrator/signals/generate.rs` (`append_stage_feedback`, the reader),
`loom/src/orchestrator/signals/contract.rs` (the contract-writer signal's reader),
`loom/src/models/stage/methods.rs` (`try_request_adjudication`; do not edit: the file has an
exact ledger row of 492 lines).
Frozen (never edit): `loom/src/orchestrator/adjudication/notice_contract_tests.rs`.

## Changes

### 1. One path helper below both layers (`models/dispute.rs`)

`loom/src/stage_service/` imports nothing from `crate::orchestrator`, and must not start. Add
beside `dispute_dir`, `request_file`, `verdict_file` and `applied_marker`:

```rust
/// `<disputes_root>/<stage_id>/feedback.md`: the verdict notices of the stage's current round.
pub fn feedback_file(disputes_root: &std::path::Path, stage_id: &str) -> std::path::PathBuf
```

`feedback.rs` builds its path through it (drop `FEEDBACK_FILENAME`'s other uses or keep the
constant private to `models/dispute.rs`). `models/dispute.rs` is 380 lines: keep the addition to
the function and its doc comment.

### 2. Notices accumulate (`feedback.rs`)

- Add one private writer, `append_section(work_dir, stage_id, body) -> Result<()>`, that opens
  the file the way `append_completion_failures` does (`safe_open_append` on the parent
  directory) and writes `body` preceded by a separator line `\n---\n\n` when the file already
  has content. `append_rejection`, `append_questions` and `write_notice` build the same bodies
  as today and call it. Their signatures and names stay, so no caller changes.
- Bound the file: when the existing file plus the new section would pass 65,536 bytes, the file
  is replaced by the single line `Earlier notices of this round were dropped: the file passed
  64 KiB.` followed by the new section.
- `read_feedback` and `clear_feedback` keep their behaviour.
- Rewrite the module comment and each function's doc comment: they say "overwritten on every
  adjudicator pass", which is the defect.

### 3. The first filing of a round clears (`stage_service/dispute.rs`, `dispute_kinds.rs`)

Both filing paths call `try_request_adjudication` under the stage lock (`dispute.rs` for a
criterion dispute, `file_on_locked_stage` in `dispute_kinds.rs` for the other kinds, the
loom-filed freeze refusal included). In each, before that call: when the stage's status is not
`StageStatus::NeedsAdjudication`, remove `feedback_file(..)`, treating a missing file as
success and any other error as the filing's error. A filing made while the stage is already
`NeedsAdjudication` removes nothing. Put the removal in one function both paths call.

## Tests (write them; the orchestrator runs them)

- `feedback.rs` test module: two `write_notice` calls leave both bodies, first then second,
  with one separator; a rejection followed by a notice keeps both; a file at the bound is
  replaced as specified; `append_completion_failures` after two notices keeps all three parts.
  Any existing test that asserts a second write replaces the first now asserts the opposite:
  name each in your report (expected test-integrity events).
- `stage_service` tests (the module's existing dispute tests; follow their fixture): a filing
  from `Executing` removes an existing feedback file; a second filing while `NeedsAdjudication`
  leaves it.
- Contract to turn green: `notice_contract_tests.rs`.

## Proof

From `loom/`: `cargo check --all-targets`.

## Traps

- Do not clear the file when a verdict is applied or when a session starts: the fresh session
  reads it in its signal, and a handoff successor of that session needs it again.
- `requeue.rs` holds the stage in `NeedsAdjudication` until the last sibling dispute is judged
  (`requeue_or_hold_for_remaining_disputes`); the notices of the earlier siblings are written
  while it is held. That is the window in which the old code lost them.
- The reader in `signals/generate.rs` shows the file only when `has_disputes(stage)`. It needs no
  change.
- `models/dispute.rs` is planned to be split by a later plan; add only the one function.
