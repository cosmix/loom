---
verified: 5546d3c47ddc1f8890b40157134f057393b8b90e
---

# Adjudication Persistence and Stage Resumption

>

## Durable Artifact Chain

Paths below are relative to `.loom/work/`, the run state directory:

| Path | Authority | Purpose |
| --- | --- | --- |
| `disputes/<stage>/<n>/request.md` | daemon on behalf of the stage agent | Criterion, reason, evidence, failure output, attempt count and timestamp |
| `disputes/<stage>/<n>/verdict.json` | adjudication session | Draft passed to `loom stage adjudicate`; not authoritative |
| `disputes/<stage>/<n>/verdict.md` | guarded adjudication record path | Immutable verdict, reasoning or questions, citations, plan patch and judge identity |
| `disputes/<stage>/<n>/applied.marker` | daemon | Terminal marker that keeps the applied verdict out of pending scans |
| `disputes/<stage>/<n>/closed.marker` | daemon | Marks an abandoned dispute settled |
| `disputes/<stage>/<n>/attempts` | daemon | Adjudication-session respawn budget spent when work is handed out |
| `disputes/<stage>/feedback.md` | daemon | Adjudication message eligible for a later stage signal |

`loom stage adjudicate` validates the draft and records the immutable verdict. The daemon scans verdicts without terminal markers and applies them under the stage dispute lock. The stage transaction records a dispute-specific receipt together with the verdict result; replay uses that receipt to finish closure and marker publication without repeating the stage charge. A legacy `.applying` file is removed on successful marker publication and is not an ownership claim.

## Retirement and Fresh-Session Boundary

Before applying a verdict, the daemon holds the stage dispute lock while checking the pending dispute, retiring its attached non-adjudication agents and applying the verdict. Retirement writes a `HandoffOrigin::Retired` handoff, confirms each writer is dead and clears `stage.session`; a surviving writer defers application. Recording, application, reset and approval coordinate through the same dispute-lock-before-stage-lock order.

Verdict application commits a dispute-specific receipt with the stage update. A replay that finds the receipt finishes marker and sibling closure work without charging the evidence round twice. Apply-cap escalation records a separate cap receipt with the `NeedsHumanReview` transition. Strict sibling closure publishes applied markers for receipt-consumed verdicts, closes abandoned siblings and propagates failures before publishing the current marker. The current applied verdict is not marked closed. Scanners and recorders treat `closed.marker` as settled.

Ordinary escalation retires writers first, then moves a stage still in `NeedsAdjudication` to `NeedsHumanReview`. `Escalation::with_unretired_agents` names any surviving agent and the reset command. Human-review approval refuses live worker sessions and requires open disputes to be closed before queueing the stage.

Once all sibling disputes have verdicts, the stage leaves `NeedsAdjudication` one of two ways, and a verdict that changes the plan always takes the first.

- **Fresh session.** An `Accept` that amends criteria, a `NeedsMoreEvidence` result, an accepted contract dispute, a freeze-refusal ruling, a findings ruling that upholds a finding, and any verdict whose stage fails the checks below move the stage to `Queued`. The executor creates a fresh session in the existing worktree. `start_stage` starts or adopts only a stage that is `Queued` or `WaitingForDeps` on disk; for another status it resyncs the graph node and returns. The successor signal contains the updated stage, eligible predecessor handoff, stage memory and any dispute feedback. A verdict that amends criteria needs this session because only a session reads the amended criteria fresh.
- **Held, then completed by the daemon.** An accepted integrity or impact-tests dispute, or a findings ruling that upholds nothing, changes nothing the agent's last completion run depended on, so `notify_and_hold` (`orchestrator/adjudication/apply_kinds.rs`) keeps the stage in `NeedsAdjudication` with no session. The sweep `Orchestrator::start_verdict_completions` (`orchestrator/core/verdict_completion.rs`) runs at the end of every verdict pass (`apply_pending_verdicts`, including the pass at startup). For each held stage it calls `host_verify::verdict_complete::submit_verdict_completion`, which, once the eligibility checks below pass, takes one of three outcomes: the daemon's completion gates (`host_verify/gates.rs::refusal`) refuse the stage (`VerdictStart::GatesRefused`, handed back at once because a held stage has no session to change what the gates read); the stage's latest `Complete` run is reused (`VerdictStart::Reused`); or a `JobKind::VerdictComplete` host job bound to the stage, not to a session, is queued. A queued job's pass runs the gates again at apply. Reuse and a passing job both move the stage `NeedsAdjudication -> Completed` through `Stage::complete_from_verdict` (`models/stage/verdict_completion.rs`), the only caller allowed that edge; `try_complete` and `try_complete_merge` keep refusing it. The receipt of a queued job is `completed_commit` plus the job's `applied` marker; no session checkpoint is written.

**Reuse** (`host_verify/verdict_reuse.rs`): the agent's own passing `Complete` run is not run again. `verdict_reuse::decides` holds when the newest `Complete` record is `Passed`, its result passed at the record's head, and no `VerdictComplete` record is newer by `(submission_order, queued_at)` (a failed verdict rerun at the same head must not be overridden by the older pass). `complete_from_run` then completes the stage through `verdict_complete::complete_held` under the dispute lock and the stage file, the code that applies a passing `VerdictComplete` job (`bind_and_complete_held`, whose `verify_bound_run` re-checks the committed head and policy hash under the lock). No applied marker or checkpoint is written; the receipt is `stage.completed_commit`. A run that failed only its impact tests, followed by an accepted impact-tests dispute, is not `Passed` and still reruns.

The held stage is eligible only when all of these hold: no other dispute of the stage is open; its worktree is clean; its HEAD equals the HEAD of its newest `Complete` job record; that record has a saved result that either passed in the child (only the daemon-applied integrity or review gate refused it) or failed with every failed outcome labelled `impact-tests` (`IMPACT_TESTS_LABEL`; impact tests run in the child, so the run an impact-tests dispute follows is always `Failed`); and the result's `policy_hash` equals the stage's current `effective_policy_hash` (`handoff/completion/policy.rs`). A sibling verdict of the same batch that amended criteria, re-froze contracts or changed the plan since that run moves the hash, and the stage then takes the fresh-session path whatever order the verdicts arrived in. A knowledge stage never qualifies (its completion needs the session). A stage the operator holds (`Stage.held`, `loom stage hold`) is never completed by the daemon: the sweep waits while a `VerdictComplete` job for it is unapplied, then `hand_back_held_stage` (`orchestrator/adjudication/requeue.rs`) re-queues it and it stays held.

A failed, cancelled or interrupted `VerdictComplete` job, a gate or binding refusal (a `GatesRefused` start carries the full refusal as the fresh session's feedback failure; `hand_back_cause` in `orchestrator/core/verdict_completion.rs` maps each outcome), a missing or unreadable record and any error while the sweep checks eligibility hand the stage back: `hand_back_held_stage` appends the failure section to `disputes/<stage>/feedback.md` after the transition to `Queued` succeeds, so a retried hand-back lists the failures once, and the fresh session sees them. No job is resubmitted. The sweep reads stage files, never `self.graph`, whose statuses were synced before the verdicts applied. A test that saves a `Complete` run and then builds an `Orchestrator` sees a stale policy: `write_terminal_config` writes into the state root's config file, which `effective_policy_hash` digests, so build the daemon first and record the run after.

The successor does not load dispute request or verdict files directly and does not inherit the judge conversation. Verdict application writes the facts needed by the successor into those signal inputs.

## What Each Verdict Delivers

| Verdict             | Durable effect                                                                                                                                                                                                                                                                                                                         | What a later stage session sees                                                                                                                                                                                                                                                                                                                                                                         |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Accept`            | Applies each of the verdict's `patches` to `acceptance` or `wiring` as its own amendment (`apply_accept.rs`), in descending index order with audit dispute ids `<id>.<k>`; updates the active plan and stage file; writes `plan_versions/<n>.md` and one `plan_versions/audit.md` row per patch; requeues through `notify_and_requeue` | Amended acceptance/wiring rendered by the normal signal, plus `disputes/<stage>/feedback.md`: which indices were replaced or deleted, the adjudicator's reasoning (control characters stripped, capped at 2 KB), and the instruction to run `loom stage complete` before anything else and to dispute, never block, anything still failing. The full record stays in `disputes/<stage>/<n>/verdict.md`. |
| `NeedsMoreEvidence` | Writes the judge's questions to `disputes/<stage>/feedback.md`, increments `evidence_rounds`, and re-queues unless another dispute is unanswered or the evidence cap is exhausted                                                                                                                                                      | The questions appended at the end of the signal under `## Adjudicator Feedback (from your prior dispute)`                                                                                                                                                                                                                                                                                               |
| `Reject`            | Writes reasoning and citations to `disputes/<stage>/feedback.md` and moves the stage to `NeedsHumanReview`                                                                                                                                                                                                                             | No automatic rerun. If a human later approves a fresh attempt, the persisted rejection feedback is available to its signal.                                                                                                                                                                                                                                                                             |

The three verdicts above amend criteria or ask for evidence, so each re-queues the stage for a fresh session. The other kinds differ:

- An accepted **integrity** or **impact-tests** dispute records the acceptance (integrity events stay pinned, see [verification-v2-gates](verification-v2-gates.md); an accepted impact-tests failure stays accepted while it gets no worse) and holds the stage with no session.
- A **findings** ruling that upholds nothing closes every disputed finding (`dismiss` or `defer`) and holds the stage the same way; a ruling that upholds one finding re-queues it, because the finding stays open.
- The daemon finishes a held stage whose committed tree passes (see "Retirement and Fresh-Session Boundary"); a held stage that fails the eligibility checks gets a fresh session with the failure listed in `disputes/<stage>/feedback.md`.
- An accepted **contract** dispute re-queues and re-freezes (see "Accepted Dispute Re-Freeze" in [contract-phase](contract-phase.md)).

An accepted amendment's audit row stores the dispute id and the optional reason inside
`plan_patch`; that reason is not necessarily the verdict's full `reasoning` field. For the complete
adjudication record, use `disputes/<stage>/<n>/verdict.md`.

Criterion adjudicators first run `loom check <stage> --no-cache` and inspect the disputed entry's reported result, including its setup and command boundary. A direct-shell pass alone cannot justify rejection of a failure inside Loom. A portable amendment that preserves the assertion is an `Accept` with `plan_patch`, applied automatically by the existing amendment path. The briefing tells the judge to rule on every disputed criterion and every other criterion `loom check` reports failing. A failure caused only by where completion runs (no network, git metadata, the command timeout, a sandbox denial) is accepted with a portable `replace` (an extended criterion may set `timeout_secs`, 1-3600) or a `delete` that names what it leaves to integration-verify; the judge runs the criterion itself and never asks for evidence no agent can produce, such as an unsandboxed run. A need only a person can meet (credential, host install) is a `reject` naming it.

## Source Path

1. `commands/stage/adjudicate.rs::record_verdict` establishes the guarded verdict record.
2. `orchestrator/adjudication/session.rs::persist_verdict` writes `disputes/<stage>/<n>/verdict.md`.
3. `orchestrator/core/verdict_apply.rs::apply_pending_verdicts` retires the old agent, then delegates application.
4. `orchestrator/adjudication/apply.rs::persist_verdict_result` writes the materialized stage state and `applied.marker`.
5. `orchestrator/adjudication/feedback.rs` owns the transient successor-facing feedback file.
6. `orchestrator/core/stage_executor.rs::start_stage` selects the handoff and calls `orchestrator/signals/generate.rs::generate_signal_with_skills`, which appends feedback last.

## Dispute Kinds: Criterion, Findings, Contract, Integrity, Impact Tests

`DisputeRequest` carries a `DisputeKind` (`models/dispute.rs`, serde tag `kind`, kebab-case). `Criterion { criterion_indices, field }`
disputes one or more entries of the list `field` names (`CriterionField`: `acceptance`, `wiring`, `wiring-tests`; the request
file carries `field` beside `criterion_indices`, sorted and deduplicated, and `Request::DisputeCriteria` carries both). One
filing spends one unit of the budget however many indices it names, and the verdict's `plan_patch.patches` may patch each
of them (`plan_patch.rs::normalize` also reads a single `patch`; a repeated index is `needs-more-evidence`). When the dispute request file is
unreadable at apply time, the kind falls back to the acceptance criterion path. The verdict must amend the disputed field
(`orchestrator/adjudication/verdict.rs::admits`): a wiring or wiring-tests dispute may amend only its own list, an
acceptance dispute may amend `acceptance` or `wiring`. The wiring and wiring-tests briefings reuse the criterion
briefing's verdict rules, run reading, schema and evidence sections, with "the criterion" meaning the disputed entry, so
the acceptance briefing stays byte-identical (`prompt/tests_golden.rs`). `CriterionField` keeps `Default` because
test code builds `DisputeKind::criterion(Default::default(), vec![0])`. The four v2 kinds:

| Kind         | Filed with                                                                                           | Budget field         | Judge verdicts                                                                | Durable effect                                                                                                                                                                                          |
| ------------ | ---------------------------------------------------------------------------------------------------- | -------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Findings     | `loom stage dispute-findings <stage> --finding <id>... --reason`                                     | `finding_disputes`   | `rulings` (per finding `uphold`, `dismiss`, `defer`) or `needs-more-evidence` | rulings append to `reviews/<stage>/rulings.json`; a `defer` appends to `reviews/<target>/carried.json`; feedback lists upheld findings                                                                  |
| Contract     | `loom stage dispute-contract <stage> --contract <id> --reason`                                       | `contract_disputes`  | `accept`, `reject`, `needs-more-evidence`                                     | accept re-freezes that contract's files at current worktree content and may apply a `plan_patch` to `contracts` (`AmendmentField::Contracts`); reject sends the agent to `loom stage contracts restore` |
| Integrity    | `loom stage dispute-integrity <stage> --event <id>... --reason`                                      | `integrity_disputes` | `accept`, `reject`, `needs-more-evidence`                                     | accept upserts `reviews/<stage>/integrity.json` with the evidence snapshot filed with the dispute                                                                                                       |
| Impact tests | `loom stage dispute-criteria <stage> --field impact-tests --test <id>... --reason` (standard stages) | `impact_disputes`    | `accept`, `reject`, `needs-more-evidence`                                     | accept upserts `reviews/<stage>/impact.json` by id with the failure as the observed record holds it (`apply_impact.rs`); reject tells the agent the failures stand                                      |

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
(`verify/impact_tests/observed.rs`). The host job that runs a completion writes the record after a failing run
(`host_verify/child_complete.rs::record_failed_impact`), and an operator's own completion writes it directly. Both
runs execute the stage's code, so the record is unauthenticated: a run can print a forged failure. The adjudicator is
the trust gate: it re-runs each disputed selection before accepting, and an accepted failure excuses only a later
failure identical to it (kind, count and test set; `verify/impact_tests/accepted.rs`). An id the record does not hold
is refused. A failing run replaces the record; a passing run leaves it, so a dispute names failures of the latest
failing run.

**Verdict rules** (`orchestrator/adjudication/verdict.rs`, `verdict_kinds.rs`, `apply.rs`, `apply_kinds.rs`).
`FindingRuling.ruling` reuses `verify::review::store::RulingKind`. `defer` is valid only when the disputing
stage is not integration-verify and `target_stage` transitively depends on it and is not already finished;
anything else is coerced to `needs-more-evidence` naming why. Integration-verify never defers. A contract,
integrity or impact-test `accept` without a plan patch stores `PlanPatch { inner: {} }` because `{}` is the only
round-trip-stable "none" (a flattened `Null` serialises as `{}`). Rulings and carried entries append if absent;
integrity and impact acceptance are upserts by id. Requeue still goes through
`requeue_or_hold_for_remaining_disputes`, so a sibling dispute holds the stage. `parse_and_validate(raw)` remains
the criterion and relay pre-check; `parse_and_validate_for(raw, kind)` is the kind-aware entry (a signature change
made after a truncated caller search broke `commands/stage/adjudicate.rs::relay_verdict`).

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

## Durable attempt reservations

Adjudication charges its spawn budget before handing out a session. The historical `apply_failures` file stores a durable reservation for every verdict-application attempt, including a successful one; its cap remains three. Missing counters begin at zero, while malformed or unreadable existing counters stop the operation instead of resetting the budget.

## Filing Under the Stage Lock and Orphaned Requests

`file_under_stage_lock` (`stage_service/dispute_store.rs`) runs one filing under the stage lock, so no independent stage writer comes between its admission checks and its status change. A status change before admission publishes nothing and spends nothing. `check_filer` reads the session with the unlocked loader: taking `sessions/` while holding `stages/` inverts the order `complete_stage` uses and would deadlock.

A failed stage write publishes nothing, so the request written for it is withdrawn: a filing reported failed leaves no open request. A crash between the two writes leaves a request the stage does not name:

- A retried filing with a receipt reuses that request, because the receipt reserved its id (`dispute-published:<id>` in `tally.request_receipts`). The receipt of the attempt past capacity settles without a new document.
- A later filing names another request. The adjudication scan then settles the orphan unjudged (`settle_orphaned_request`, `orchestrator/adjudication/mod.rs`) under the dispute lock, so one budget unit pays for one judgement. A filing holds that lock across both its writes, so a filing in flight is never settled. A `NeedsAdjudication` stage that names no dispute counts as having no orphans.
- `requeue_or_hold_for_remaining_disputes` skips a request the filing never published (`dispute_is_orphaned`), so an orphan holds no verdict's re-queue.

**Verdicts that no longer apply.** A verdict for a stage that left adjudication closes uncharged and leaves sibling disputes open (`orchestrator/adjudication/apply.rs`).

**Failures in the apply pass.** `apply_pending_verdicts` reports one stage's lock or verdict failure and goes on to the others (`orchestrator/core/verdict_apply.rs`); a lasting lock failure is reported once until the lock works again. An unreadable dispute directory does not end the tick.
