# dispute-kinds / W2 — verdict validation, apply per kind, carried findings, contract amendment

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D8 (freeze store), D12 (rulings and
carried shapes), D13 (integrity shape), D15. Knowledge: `architecture/adjudication-lifecycle.md`;
`architecture/plan-lifecycle-and-fields.md` "Plan Versioning / Runtime Amendment" (the write set
is two files). Code: `orchestrator/adjudication/verdict.rs` (279; `classify_and_validate`),
`apply.rs` (397 ledgered; `apply_verdict_inner` L107-125 is the verdict → effect map;
`persist_verdict_result` L152-188; `requeue_or_hold_for_remaining_disputes` L344-379),
`plan_patch.rs` (normalises `plan_patch`), `plan/amendment.rs` (1084 ledgered; `AmendmentField`
L57-67), `plan/amendment_fields.rs`.

Pinned from W1: `DisputeKind`, `FindingSnapshot`, `IntegritySnapshot`.

## Files you own

`loom/src/orchestrator/adjudication/{verdict,verdict_kinds,apply,apply_kinds,apply_kinds_tests,mod}.rs`,
`loom/src/plan/amendment.rs`, `loom/src/plan/amendment_fields.rs`,
`loom/src/verify/contracts/store.rs`, `loom/src/verify/review/mod.rs` (one line),
`loom/src/verify/review/verdict_records.rs` (new), `loom/tests/adjudication_e2e.rs`,
`loom/tests/adjudication_e2e/kinds.rs` (new).

## Tasks

1. `DisputeVerdict` gains `Rulings { rulings: Vec<FindingRuling> }` with
   `FindingRuling { finding, ruling: Ruling, target_stage: Option<String>, reasoning, citations }`
   and `Ruling { Uphold, Dismiss, Defer }`. `verdict_kinds.rs` validates by kind:
   - findings disputes accept `rulings` (one ruling per disputed id, no unknown ids, `defer`
     needs a `target_stage`) or `needs-more-evidence`;
   - contract and integrity disputes accept `accept`, `reject`, `needs-more-evidence`;
   - a shape defect coerces to `NeedsMoreEvidence` with a question naming it, the same
     self-correcting rule `verdict.rs` applies today.

   `verdict.rs` dispatches by the dispute's kind; its criterion path is unchanged.
2. `verdict_records.rs` writes the D12/D13 files atomically: `append_rulings(work_dir, stage, &[..])`,
   `append_carried(work_dir, target_stage, &[..])`, `append_integrity_acceptance(work_dir, stage, &[..])`.
3. `apply_kinds.rs`, called from `apply_verdict_inner` by kind. `apply.rs` is ledgered, so pay
   for the dispatch lines by extraction.
   - rulings: check each `defer` at apply time. The target must exist, transitively depend on
     the disputing stage, and not be `Completed`, and the disputing stage must not be
     `integration-verify`. An invalid defer turns the whole verdict into `NeedsMoreEvidence`
     with a question naming why (the D15 rule: integration-verify never defers). Valid rulings
     are appended to `rulings.json`; each `defer` also goes to the target's `carried.json`.
     Feedback (`feedback.md`) lists upheld findings. Requeue.
   - contract `accept`: re-freeze that contract's files at their current worktree content
     (hash, copy, rewrite `freeze.json` through `verify/contracts/store.rs`), and apply an
     optional `plan_patch` through a new `AmendmentField::Contracts`
     (`amendment_fields.rs::apply_patch_to_stage_def` / `apply_patch_to_runtime_stage`). W1 adds
     the CLI's `AmendField::Contracts` mirror in `types_stage.rs`. Requeue.
   - contract `reject`: feedback "restore the frozen contract with
     `loom stage contracts restore <stage> --contract <id>`, then implement against it". Requeue;
     never `NeedsHumanReview`.
   - integrity `accept`: append the events with their current counts and hashes to
     `integrity.json`. Requeue. `reject`: feedback (revert the listed changes). Requeue.
   - Requeue always goes through `requeue_or_hold_for_remaining_disputes`. Retirement of the
     disputing agent (`verdict_apply.rs`) is unchanged.
4. `tests/adjudication_e2e/kinds.rs` (declared in `adjudication_e2e.rs`, which is ledgered at
   586: pay for the `mod` line), using the existing helpers (`write_stage`, `write_dispute`,
   `session_records_verdict`, `drive_dispute`).

## Named tests (binding)

In `apply_kinds_tests.rs`:

- `dismiss_ruling_closes_finding`: after apply, `verify::review::store::open_findings` no longer
  lists the finding and the stage is `Queued`.
- `defer_ruling_carries_finding_to_target`: the target stage's `carried.json` holds
  `<origin>/F-1-1`, and its `open_findings` lists it.
- `defer_from_integration_verify_is_coerced`: a `defer` on an IV stage's finding → no rulings
  written, `feedback.md` contains a question naming that integration-verify never defers.
- `contract_accept_refreezes_current_content`: `freeze.json` holds the edited file's new hash.
- `contract_reject_requeues_with_restore_feedback`: stage `Queued`, feedback names
  `loom stage contracts restore`.
- `integrity_accept_records_counts`: `integrity.json` holds the event with its current count.

In `tests/adjudication_e2e/kinds.rs`:

- `findings_dispute_round_trip`: file a findings dispute, record a `rulings` verdict through
  `session_records_verdict`, `apply_pending_verdicts` → rulings written and the stage requeued.

## Proof (one command, once, after W1 reports)

`cargo test --manifest-path loom/Cargo.toml --lib orchestrator::adjudication::`

## Report

Files changed; exact new counts of `apply.rs`, `amendment.rs`, `adjudication_e2e.rs`; the proof
result.
