# exits-and-guards / w4b-adjudication-reason: a second dispute of a round does not archive the first as superseded

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Finding

A session filed a contract dispute and, eight seconds later, an integrity dispute. Both were
accepted. The stage file's `issue_history` recorded the first dispute's reason with
`kind: review`, `ended_by: replaced by a newer review reason`, `disposition: superseded`.

Cause: both filings end in `Stage::try_request_adjudication`
(`loom/src/models/stage/methods.rs`), which calls `replace_review_reason`
(`loom/src/models/stage/issue_history.rs`). A second filing while the stage is already
`NeedsAdjudication` is an accepted no-op transition; `replace_review_reason` then sees a
different reason, archives the first with `IssueDisposition::Superseded` and stores the second.

## Decision

While a stage is `NeedsAdjudication`, a further reason joins the review reason; nothing is
retired. When the stage leaves `NeedsAdjudication`, `retire_review_reason` already splits the
reason into lines, so both reasons reach `issue_history` with the round's real ending.

## Files

Own (write):
- `loom/src/models/stage/issue_history.rs`

Read only: `loom/src/models/stage/methods.rs` (`try_request_adjudication`; do not edit: the
file has an exact ledger row of 492 lines), `loom/src/models/stage/issue_history/disposition.rs`,
`loom/src/models/stage/transitions.rs`.
Frozen (never edit): `loom/src/models/stage/adjudication_reason_contract_tests.rs`.

## Change

`replace_review_reason(&mut self, reason: String)`:

- when `self.status == StageStatus::NeedsAdjudication` and a review reason is present:
  - the new reason equal to the old one, or already present as whole lines of it, changes
    nothing;
  - otherwise the review reason becomes the old text, a newline, then the new text, and nothing
    is pushed to `issue_history`;
- every other case behaves as today (a different reason retires the old one as superseded).

At the first filing of a round the review reason is absent, because entering `Executing`
retires it (`supersedes_reasons`), so the first filing takes the existing path. Update the doc
comment to state the rule.

## Tests (write them; the orchestrator runs them)

In the test module that already covers `issue_history.rs` (find it with
`rg -n 'replace_review_reason' loom/src --glob '*test*'`; if none exists, add
`loom/src/models/stage/issue_history_tests.rs` declared the way this module's neighbours declare
theirs and report the new file):

- two different reasons while `NeedsAdjudication` give a two-line review reason and no
  `Superseded` entry;
- the same reason twice leaves one line;
- after both, `retire_review_reason("NeedsAdjudication -> Queued", ..)` pushes one issue whose
  evidence holds both lines;
- a different reason while `NeedsHumanReview` still retires the old one as superseded.

An existing test that asserts the superseded entry for two filings in `NeedsAdjudication` now
asserts the opposite: name it in your report (an expected test-integrity event).

Contract to turn green: `adjudication_reason_contract_tests.rs`.

## Proof

From `loom/`: `cargo check --all-targets`.

## Traps

- Do not touch `try_request_adjudication`: `methods.rs` must keep its exact length.
- `replace_review_reason` has other callers (human review, escalations). The join applies in
  `NeedsAdjudication` only.
