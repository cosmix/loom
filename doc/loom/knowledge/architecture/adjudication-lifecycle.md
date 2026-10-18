---
verified: 5546d3c47ddc1f8890b40157134f057393b8b90e
---
# Adjudication Persistence and Stage Resumption

> Dispute to verdict, per-kind rulings

## Durable Artifact Chain

Paths below are relative to `.loom/work/`, the run's state directory:

| Path | Authority | Purpose |
| --- | --- | --- |
| `disputes/<stage>/<n>/request.md` | daemon on behalf of the stage agent | Original criterion index, reason, evidence commit, captured failure output, fix-attempt count, and timestamp |
| `disputes/<stage>/<n>/verdict.json` | adjudication session | Draft passed to `loom stage adjudicate`; not authoritative |
| `disputes/<stage>/<n>/verdict.md` | guarded adjudication record path | Immutable YAML-frontmatter record containing the verdict, reasoning or questions, citations, plan patch, model, attempt count, timestamp, and judge session id |
| `disputes/<stage>/<n>/.applying` | daemon | Temporary crash-recovery marker while a verdict is being applied |
| `disputes/<stage>/<n>/applied.marker` | daemon | Zero-byte idempotency sentinel proving that the verdict was applied |
| `disputes/<stage>/<n>/attempts` | daemon | Adjudication-session respawn budget, spent when work is handed out |
| `disputes/<stage>/feedback.md` | daemon | The one adjudication message that may be injected into a later stage signal |

`loom stage adjudicate` validates the draft `verdict.json` and records it as
`disputes/<stage>/<n>/verdict.md`. The daemon's
`orchestrator/adjudication/apply.rs::apply_pending_verdicts` scans verdicts without an
`applied.marker`, applies them idempotently, updates the stage file under `stages/`, then writes
the marker. A daemon restart can therefore resume an interrupted apply from the same record.

## Retirement and Fresh-Session Boundary

Before applying a verdict, `orchestrator/core/event_handler/stage_takedown.rs::retire_disputing_agents`
writes a `HandoffOrigin::Retired` handoff, kills every non-adjudication agent attached to the stage and
confirms each is dead, and clears `stage.session`. If any writer survives, verdict application
is deferred. This keeps the old, idle agent from being adopted after the criteria change.

Escalation out of the dispute loop retires first and never defers. The adjudicator registry
(`orchestrator/adjudication/mod.rs`, `session.rs`) returns `Escalation` values (`escalation.rs`) when
a judge spawn fails or the evidence or attempt cap is reached. `Orchestrator::escalate_dispute`
(`orchestrator/core/orchestrator.rs`) calls `retire_disputing_agents`, then `Escalation::write`.
`Escalation::write` moves the stage to `NeedsHumanReview` only while it is still `NeedsAdjudication`.
When an agent survives the kill, `Escalation::with_unretired_agents` names it and
`loom stage reset <stage> --kill-session` in `review_reason`. The apply cap (`escalate_apply_cap`) and
the degenerate-verdict escalation (`record.rs`) also go through `Escalation::write`.

Every move into `NeedsHumanReview` from the dispute loop closes every dispute of the stage that has no
`applied.marker`. That covers `Escalation::write`, the degenerate verdict in `record.rs`, and
`persist_verdict_result` in `apply.rs` for Reject, the amendment cap and exhausted evidence rounds. Closing
writes `closed.marker` into each such dispute directory (`close_open_disputes`,
`orchestrator/adjudication/closed_disputes.rs`) under the `disputes/<stage>/.lock` flock the filing path
takes. No caller holds the stage-file lock while closing. `is_closed` makes the scans
(`scan_pending_requests`, `scan_pending_verdicts`), `apply_verdict_once` and `ensure_recordable` treat a
closed dispute as settled.

`loom stage human-review <stage> --approve` refuses while `orchestrator::coherence::live_worker_sessions`
finds a live worker, and names `loom stage reset <stage> --kill-session`. Adoption uses the same predicate,
so approval never queues a stage onto a live agent. Approve closes the stage's open disputes again before
queueing it, which repairs a crash between a status write and its close. An abandoned dispute therefore
never shadows a dispute filed after approval.

Once all sibling disputes have verdicts, an `Accept` or `NeedsMoreEvidence` result moves the stage
to `Queued`. The normal executor then creates a fresh session in the existing worktree. `start_stage`
starts or adopts only a stage that is `Queued` or `WaitingForDeps` on disk. For any other status on
disk it resyncs the graph node (`session_adoption.rs::sync_unstartable_node`) and returns. Its signal
contains the updated `Stage`, the eligible predecessor handoff from `handoffs/`, the latest stage
memory, and, when present, `disputes/<stage>/feedback.md`.

The successor does **not** load `disputes/<stage>/<n>/request.md` or `disputes/<stage>/<n>/verdict.md`
directly, and it does not inherit the judge's conversation. Verdict application must write every fact
the successor needs into one of the signal inputs above.

## What Each Verdict Delivers

| Verdict | Durable effect | What a later stage session sees |
| --- | --- | --- |
| `Accept` | Applies the verdict's patch to `acceptance` or `wiring`; updates the active plan and stage file; writes `plan_versions/<n>.md` and a `plan_versions/audit.md` row; clears stale feedback | Amended acceptance/wiring rendered by the normal signal, plus the predecessor handoff and stage memory. The full adjudicator reasoning remains in `disputes/<stage>/<n>/verdict.md`; it is not injected into the successor signal. |
| `NeedsMoreEvidence` | Writes the judge's questions to `disputes/<stage>/feedback.md`, increments `evidence_rounds`, and re-queues unless another dispute is unanswered or the evidence cap is exhausted | The questions appended at the end of the signal under `## Adjudicator Feedback (from your prior dispute)` |
| `Reject` | Writes reasoning and citations to `disputes/<stage>/feedback.md` and moves the stage to `NeedsHumanReview` | No automatic rerun. If a human later approves a fresh attempt, the persisted rejection feedback is available to its signal. |

An accepted amendment's audit row stores the dispute id and the optional reason inside
`plan_patch`; that reason is not necessarily the verdict's full `reasoning` field. For the complete
adjudication record, use `disputes/<stage>/<n>/verdict.md`.

Criterion adjudicators first run `loom check <stage> --no-cache` and inspect the disputed entry's reported result, including its setup and command boundary. A direct-shell pass alone cannot justify rejection of a failure inside Loom. A portable amendment that preserves the assertion is an `Accept` with `plan_patch`, applied automatically by the existing amendment path. Missing runner evidence produces specific evidence questions instead of a demand for operator criterion edits.

## Source Path

1. `commands/stage/adjudicate.rs::record_verdict` establishes the guarded verdict record.
2. `orchestrator/adjudication/session.rs::persist_verdict` writes `disputes/<stage>/<n>/verdict.md`.
3. `orchestrator/core/verdict_apply.rs::apply_pending_verdicts` retires the old agent, then delegates application.
4. `orchestrator/adjudication/apply.rs::persist_verdict_result` writes the materialized stage state and `applied.marker`.
5. `orchestrator/adjudication/feedback.rs` owns the transient successor-facing feedback file.
6. `orchestrator/core/stage_executor.rs::start_stage` selects the handoff and calls `orchestrator/signals/generate.rs::generate_signal_with_skills`, which appends feedback last.

## Dispute Kinds: Criterion, Findings, Contract, Integrity, Impact Tests

`DisputeRequest` carries a `DisputeKind` (`models/dispute.rs`, serde tag `kind`, kebab-case). `Criterion { criterion_index, field }`
disputes one entry of the list `field` names (`CriterionField`: `acceptance`, `wiring`, `wiring-tests`; the request file carries
`field` beside `criterion_index`, and `Request::DisputeCriteria` carries it too). When the dispute request file is
unreadable at apply time, the kind falls back to the acceptance criterion path. The verdict must amend the disputed field
(`orchestrator/adjudication/verdict.rs::admits`): a wiring or wiring-tests dispute may amend only its own list, an
acceptance dispute may amend `acceptance` or `wiring`. The wiring and wiring-tests briefings reuse the criterion
briefing's verdict rules, run reading, schema and evidence sections, with "the criterion" meaning the disputed entry, so
the acceptance briefing stays byte-identical (`prompt/tests_golden.rs`). `CriterionField` keeps `Default` because
test code builds `DisputeKind::criterion(Default::default(), 0)`. The four v2 kinds:

| Kind | Filed with | Budget field | Judge verdicts | Durable effect |
| --- | --- | --- | --- | --- |
| Findings | `loom stage dispute-findings <stage> --finding <id>... --reason` | `finding_disputes` | `rulings` (per finding `uphold`, `dismiss`, `defer`) or `needs-more-evidence` | rulings append to `reviews/<stage>/rulings.json`; a `defer` appends to `reviews/<target>/carried.json`; feedback lists upheld findings |
| Contract | `loom stage dispute-contract <stage> --contract <id> --reason` | `contract_disputes` | `accept`, `reject`, `needs-more-evidence` | accept re-freezes that contract's files at current worktree content and may apply a `plan_patch` to `contracts` (`AmendmentField::Contracts`); reject sends the agent to `loom stage contracts restore` |
| Integrity | `loom stage dispute-integrity <stage> --event <id>... --reason` | `integrity_disputes` | `accept`, `reject`, `needs-more-evidence` | accept upserts `reviews/<stage>/integrity.json` with the evidence snapshot filed with the dispute |
| Impact tests | `loom stage dispute-criteria <stage> --field impact-tests --test <id>... --reason` (standard stages) | `impact_disputes` | `accept`, `reject`, `needs-more-evidence` | accept upserts `reviews/<stage>/impact.json` by id with the failure as the observed record holds it (`apply_impact.rs`); reject tells the agent the failures stand |

Each request may name several ids, so one retire-and-respawn covers a whole review round. The four counters
plus `evidence_rounds` and `amendments_applied` live in one `#[serde(flatten)]` struct
(`models/stage/dispute_budgets.rs`, `stage.tally`), which keeps the on-disk YAML keys unchanged and keeps the
ledgered `Stage::default` literal (`models/stage/defaults.rs`) from growing; `dispute_count` stays a top-level
field because 35 files read it. Each counter allows `MAX_DISPUTES_PER_KIND` (3); exhaustion goes to
`NeedsHumanReview`. The handler is `stage_service/dispute_kinds.rs` (`Request::FileDispute`, reached through
`daemon/server/dispute_kinds.rs`); it checks every id against what loom itself holds (an open or carried finding, a
frozen contract, a current integrity event, a failure in `reviews/<stage>/impact-observed.json`) before spending, and
cuts the reason to `DISPUTE_REASON_MAX_BYTES`.

**Impact-test evidence comes from the observed record, never the filing agent's request.** The CLI sends ids
alone (`IT-<runner>:<package>`, checked by `verify::impact_tests::check_failure_id`), and the handler drops any
evidence a request carries and copies each named failure from the observed record
(`verify/impact_tests/observed.rs`). That record is the stage session's unauthenticated report: its failing
`loom stage complete` prints it as the last two non-empty lines, a `LOOM_CONTROL_IMPACT_V1` line and a
`LOOM_CONTROL_IMPACT_EOF` line (`commands/stage/completion_evidence/impact_record.rs`), and the completion broker,
which the PostToolUse hook runs outside the sandbox on the exact pinned command's output, writes it. The run executes
the stage's code, which can print a forged record and kill loom before loom prints its own, so the adjudicator is the
trust gate: it re-runs each disputed selection before accepting, and an accepted failure excuses only a later
failure identical to it (kind, count and test set; `verify/impact_tests/accepted.rs`). A host completion (no stage
session) writes the record directly. An id the record does not hold is refused; every failing completion replaces
the record, so only failures of the latest run can be disputed.

**Verdict rules** (`orchestrator/adjudication/verdict.rs`, `verdict_kinds.rs`, `apply.rs`, `apply_kinds.rs`).
`FindingRuling.ruling` reuses `verify::review::store::RulingKind`. `defer` is valid only when the disputing
stage is not integration-verify and `target_stage` transitively depends on it and is not already finished;
anything else is coerced to `needs-more-evidence` naming why. Integration-verify never defers. A contract,
integrity or impact-test `accept` without a plan patch stores `PlanPatch { inner: {} }` because `{}` is the only
round-trip-stable "none" (a flattened `Null` serialises as `{}`). Rulings and carried entries append if absent;
integrity and impact acceptance are upserts by id. Requeue still goes through
`requeue_or_hold_for_remaining_disputes`, so a sibling dispute holds the stage. `parse_and_validate(raw)` remains
the criterion and relay pre-check; `parse_and_validate_for(raw, kind)` is the kind-aware entry (a signature change
made after a truncated caller search broke `commands/stage/adjudicate.rs:109`).

**Prompts** (`orchestrator/adjudication/prompt.rs` and `prompt/{findings,contract,integrity,impact}.rs`) keep the shared
instructions, the 100,000-byte cap and the JSON contract per kind. `prompt::build` computes the verdict draft
path itself. `ExecutionSite` carries `root` (worktree root, or repo root once the worktree is gone) and
`worktree()`; deriving the root from `path` minus `working_dir` breaks on `./x` or `../x`. Findings prompts
quote the file ±20 lines around each finding, the round and the stage diff; contract prompts quote the spec,
frozen and current content and their diff; integrity prompts quote the event and the removed lines; impact prompts
give the stage base loom reads at adjudication time, quote each failure with every field (outcome, command, base
result, tests, output) labelled as reported by the stage session, tell the adjudicator to reproduce each selection in
the worktree and the base in a separate checkout before accepting, label the reason as the agent's, then add the stage
diff (`truncate` halves the diff first).
`prompt/tests_golden.rs` pins the criterion prompt byte for byte.

**Relay.** `loom-hooks/loom-relay.sh` `relay_kind_at` maps `stage:dispute-findings|dispute-contract|dispute-integrity`,
and `dispute-criteria` with `--field impact-tests`, to `file-dispute`. A new `RequestKind` needs a `relay_kind_at` case, a `drop_control_kinds` entry when
`is_control()`, and a `loom-hooks/tests/*.sh` test registered in `run-all.sh` (a static `run_test` list, not a
glob) in the same change; the dispute kinds shipped without the first, and the regression test never ran until
the second was found.

**Carried findings.** A `defer` makes the finding a carried finding of the target stage (`origin/F-1-2`); the
target stage's review gate counts it open until a later round resolves it or a ruling closes it.
