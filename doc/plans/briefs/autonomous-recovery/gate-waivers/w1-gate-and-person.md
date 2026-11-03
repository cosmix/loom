# gate-waivers W1: gate disputes and the needs-person verdict, end to end

Tier: opus. Wave 2, after W0. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `tests/gate_waiver_contracts.rs`, `src/stage_service/recovery_contract_tests.rs`.

## Why

Two stops have no autonomous route. (1) Loom's own gates `unwired`, `duplicates` and an
`impact-tests` run that errored before any selection have no plan entry to amend
(`doc/loom/knowledge/concerns/merge-and-recovery-edge-cases.md`, "A Defect in a Loom Gate Parks
the Stage Until an Operator Acts"). (2) A check that fails because the host lacks a tool,
credential or decision is refused as a block (the guard sends it to a dispute) and the
adjudicator has no verdict that ends at a readable block.

## Filing (you own these files)

1. `src/stage_service/dispute_gate.rs` (the orchestrator created it and declared it in
   `stage_service/mod.rs`; never edit `mod.rs`; declare `dispute_freeze_text.rs` from
   `dispute_kinds.rs` with `#[path]` if you create it) with `pub(super) fn confirm_gate(work_dir: &Path,
   stage: &Stage, gates: Vec<String>) -> Result<DisputeKind>`:
   - every label is in `WAIVABLE_GATES` (else `"'<label>' is not a waivable gate"`), no label
     twice;
   - the stage's current session's latest trusted checkpoint
     (`crate::handoff::load_trusted_session_checkpoint(&stage.id, session, work_dir)`, as
     `control_block.rs` reads it) is phase `ToolFailed` and lists each label with
     `passed: false` (else `"gate '<label>' did not fail in the latest loom stage complete"`);
   - for `impact-tests`, the observed record (`crate::verify::impact_tests::load_observed`)
     has no failure (else `"impact-tests named failing tests; dispute them with --field
     impact-tests --test <id>"`).
2. `src/stage_service/dispute_kinds.rs` (398 lines: move `normalized_problems` and
   `receipt_key` to `dispute_gate.rs`'s sibling `dispute_freeze_text.rs` if the arm does not
   fit): in `confirm`, handle `DisputeKind::Gate` BEFORE the plan-version check (a gate
   dispute files for plan version 1 and 2) by calling `confirm_gate`. The budget is
   `gate_disputes` (W0 routed `dispute_budgets`).
3. `../loom-hooks/loom-relay.sh`: `stage:dispute-criteria` with `--field gate` (both `--field
   gate` and `--field=gate`) routes `file-dispute`, like `impact-tests`.
   `../loom-hooks/tests/loom-relay-kinds.sh`: `expect file-dispute` lines for both spellings
   and one `expect dispute` line where `gate` appears only in `--reason`.

## Verdicts

4. New `src/orchestrator/adjudication/verdict_person.rs`: `pub(super) fn validate(json:
   &Value) -> ValidationOutcome` for tag `needs-person`: `need` parses as `PersonNeed` (else
   needs-more-evidence naming the three values); `what` is a non-empty string, reduced to one
   line (`strip_control(.., &[])`, newlines become spaces) and cut to
   `NEEDS_PERSON_WHAT_MAX_BYTES` on a char boundary; grounding through
   `verdict::grounding(json, "Needs-person")`.
5. `src/orchestrator/adjudication/verdict.rs`: `parse_and_validate_for` and
   `parse_and_validate_in` route tag `needs-person` to `verdict_person::validate` for EVERY
   kind before the kind routing (freeze refusals included). Keep the file at or under 400
   lines.
6. `src/orchestrator/adjudication/verdict_kinds.rs`: `DisputeKind::Gate` verdicts: `accept`
   needs `plan_patch: {"waive": [<labels>]}`, non-empty, each one of the disputed gates (else
   needs-more-evidence naming them); stored as `DisputeVerdict::Accept` with that
   `plan_patch`. `reject` and `needs-more-evidence` as for other kinds.

## Apply

7. New `src/orchestrator/adjudication/apply_gate.rs`: on a gate accept, one `update_stage`
   pushes a `GateWaiver { gate, dispute_id, reason: <reasoning, control-stripped, cut to 500
   bytes>, loom_version: crate::version::VERSION, waived_at: now }` per waived label (a label
   already waived is not pushed twice), then `notify_and_requeue` with a notice naming the
   waived gates. Test: the waiver is still on the stage after the requeue and a reload. A gate reject escalates to human review (the deadlock rule in `apply.rs`).
8. New `src/orchestrator/adjudication/apply_person.rs`: `NeedsPerson` blocks the stage. One
   `update_stage` calls `crate::stage_service::apply_block(stage, &reason)` with reason
   `"<need.reason_prefix()>: <what> (dispute <n>; <first 200 bytes of reasoning>)"`, then the
   in-memory stage mirrors `status`, `close_reason`, `review_reason` and `failure_info` so
   `persist_verdict_result` writes nothing more.
9. `src/orchestrator/adjudication/apply_kinds.rs`: route `NeedsPerson` (any kind) and the
   gate kind's accept/reject; `src/orchestrator/adjudication/apply.rs`:
   `persist_verdict_result` closes the stage's other open disputes for `Blocked` as it does
   for `NeedsHumanReview`. Never edit `src/orchestrator/adjudication/mod.rs` (spawn-recovery,
   a concurrent stage, owns it): declare `verdict_person` from `verdict.rs` and `apply_gate`,
   `apply_person` from `apply_kinds.rs` as `#[path = "<file>.rs"] mod <name>;` children, and
   `needs_person`, `gate` from `prompt.rs` where it declares its other children.

## Briefings

10. New `src/orchestrator/adjudication/prompt/needs_person.rs` with the one paragraph every
    briefing ends with (appended once in `prompt::build`): when the disputed check cannot pass
    until a person installs a host tool, provides a credential or makes a decision only a
    person can make, emit `{"verdict": "needs-person", "need": "host-install"|"credential"|
    "decision", "what": "<one line: exactly what is needed>", "reasoning": "...",
    "citations": [...]}`; loom blocks the stage with that need and the operator resumes it
    with `loom stage retry`. Never for something the stage or an amendment can fix. Update
    `prompt/tests_golden.rs` expectations for the appended paragraph (an expected assertion
    change; list it).
11. New `src/orchestrator/adjudication/prompt/gate.rs`: the briefing for a gate dispute: each
    label, what the gate checks (`unwired`: `verify::wiring_detection::detect_unwired_files`
    against the base branch; `duplicates`: `verify::duplicate_detection`, failing only when
    detection could not run; `impact-tests`: the run errored before any selection), the
    latest outcome summary for the label from the job result the checkpoint's
    `evidence_nonce` names (`crate::host_verify` store), and the verdict schema
    `{"verdict": "accept", "plan_patch": {"waive": [...]}, ...}`. Accept when the gate's
    failure is a false positive or a gate defect (cite the wiring the detector missed, or the
    error); reject when the stage must wire or remove the file.

## Done

Your one check after the stage compiles: `cargo test --lib orchestrator::adjudication::`.
