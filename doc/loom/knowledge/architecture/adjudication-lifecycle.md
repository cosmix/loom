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
writes a `HandoffOrigin::Retired` handoff, kills and confirms the death of every non-adjudication
agent attached to the stage, and clears `stage.session`. If any writer survives, verdict application
is deferred. This prevents the old, idle agent from being adopted after the criteria change.

Once all sibling disputes have verdicts, an `Accept` or `NeedsMoreEvidence` result moves the stage
to `Queued`. The normal executor then creates a fresh session in the existing worktree. Its signal
contains the updated `Stage`, the eligible predecessor handoff from `handoffs/`, the latest stage
memory, and—when present—`disputes/<stage>/feedback.md`.

The successor does **not** load `disputes/<stage>/<n>/request.md` or `disputes/<stage>/<n>/verdict.md`
directly, and it does not inherit the judge's conversation. Verdict application must materialize
every fact the successor needs into one of the signal inputs above.

## What Each Verdict Delivers

| Verdict | Durable effect | What a later stage session sees |
| --- | --- | --- |
| `Accept` | Applies the verdict's patch to `acceptance` or `wiring`; updates the active plan and stage file; writes `plan_versions/<n>.md` and a `plan_versions/audit.md` row; clears stale feedback | Amended acceptance/wiring rendered by the normal signal, plus the predecessor handoff and stage memory. The full adjudicator reasoning remains in `disputes/<stage>/<n>/verdict.md`; it is not injected into the successor signal. |
| `NeedsMoreEvidence` | Writes the judge's questions to `disputes/<stage>/feedback.md`, increments `evidence_rounds`, and re-queues unless another dispute is unanswered or the evidence cap is exhausted | The questions appended at the end of the signal under `## Adjudicator Feedback (from your prior dispute)` |
| `Reject` | Writes reasoning and citations to `disputes/<stage>/feedback.md` and moves the stage to `NeedsHumanReview` | No automatic rerun. If a human later approves a fresh attempt, the persisted rejection feedback is available to its signal. |

An accepted amendment's audit row stores the dispute id and the optional reason inside
`plan_patch`; that reason is not necessarily the verdict's full `reasoning` field. For the complete
adjudication record, use `disputes/<stage>/<n>/verdict.md`.

## Source Path

1. `commands/stage/adjudicate.rs::record_verdict` establishes the guarded verdict record.
2. `orchestrator/adjudication/session.rs::persist_verdict` writes `disputes/<stage>/<n>/verdict.md`.
3. `orchestrator/core/verdict_apply.rs::apply_pending_verdicts` retires the old agent, then delegates application.
4. `orchestrator/adjudication/apply.rs::persist_verdict_result` writes the materialized stage state and `applied.marker`.
5. `orchestrator/adjudication/feedback.rs` owns the transient successor-facing feedback file.
6. `orchestrator/core/stage_executor.rs::start_stage` selects the handoff and calls `orchestrator/signals/generate.rs::generate_signal_with_skills`, which appends feedback last.

## Dispute Kinds: Criterion, Findings, Contract, Integrity

`DisputeRequest` carries a `DisputeKind` (`models/dispute.rs`, serde tag `kind`, kebab-case). `Criterion { criterion_index }`
is the original dispute and behaves as before, including when the dispute request file is unreadable at apply time
(the kind falls back to the criterion path so v1 behaviour is unchanged). The three v2 kinds:

| Kind | Filed with | Budget field | Judge verdicts | Durable effect |
| --- | --- | --- | --- | --- |
| Findings | `loom stage dispute-findings <stage> --finding <id>... --reason` | `finding_disputes` | `rulings` (per finding `uphold`, `dismiss`, `defer`) or `needs-more-evidence` | rulings append to `reviews/<stage>/rulings.json`; a `defer` appends to `reviews/<target>/carried.json`; feedback lists upheld findings |
| Contract | `loom stage dispute-contract <stage> --contract <id> --reason` | `contract_disputes` | `accept`, `reject`, `needs-more-evidence` | accept re-freezes that contract's files at current worktree content and may apply a `plan_patch` to `contracts` (`AmendmentField::Contracts`); reject sends the agent to `loom stage contracts restore` |
| Integrity | `loom stage dispute-integrity <stage> --event <id>... --reason` | `integrity_disputes` | `accept`, `reject`, `needs-more-evidence` | accept upserts `reviews/<stage>/integrity.json` with the evidence snapshot filed with the dispute |

Each request may name several ids, so one retire-and-respawn covers a whole review round. The three counters
plus `evidence_rounds` and `amendments_applied` live in one `#[serde(flatten)]` struct
(`models/stage/dispute_budgets.rs`, `stage.tally`), which keeps the on-disk YAML keys unchanged and keeps the
ledgered `Stage::default` literal (`models/stage/defaults.rs`) from growing; `dispute_count` stays a top-level
field because 35 files read it. Each counter allows `MAX_DISPUTES_PER_KIND` (3); exhaustion goes to
`NeedsHumanReview`. The daemon handler is `daemon/server/dispute_kinds.rs` (`Request::FileDispute`); it checks
every id exists (an open or carried finding, a frozen contract, a current integrity event) before spending.

**Verdict rules** (`orchestrator/adjudication/verdict.rs`, `verdict_kinds.rs`, `apply.rs`, `apply_kinds.rs`).
`FindingRuling.ruling` reuses `verify::review::store::RulingKind`. `defer` is valid only when the disputing
stage is not integration-verify and `target_stage` transitively depends on it and is not already finished;
anything else is coerced to `needs-more-evidence` naming why. Integration-verify never defers. A contract or
integrity `accept` without a plan patch stores `PlanPatch { inner: {} }` because `{}` is the only
round-trip-stable "none" (a flattened `Null` serialises as `{}`). Rulings and carried entries append if absent;
integrity acceptance is an upsert by event id. Requeue still goes through
`requeue_or_hold_for_remaining_disputes`, so a sibling dispute holds the stage. `parse_and_validate(raw)` remains
the criterion and relay pre-check; `parse_and_validate_for(raw, kind)` is the kind-aware entry (a signature change
made after a truncated caller search broke `commands/stage/adjudicate.rs:109`).

**Prompts** (`orchestrator/adjudication/prompt.rs` and `prompt/{findings,contract,integrity}.rs`) keep the shared
instructions, the 100,000-byte cap and the JSON contract per kind. `prompt::build` computes the verdict draft
path itself. `ExecutionSite` carries `root` (worktree root, or repo root once the worktree is gone) and
`worktree()`; deriving the root from `path` minus `working_dir` breaks on `./x` or `../x`. Findings prompts
quote the file ±20 lines around each finding, the round and the stage diff; contract prompts quote the spec,
frozen and current content and their diff; integrity prompts quote the event and the removed lines.
`prompt/tests_golden.rs` pins the criterion prompt byte for byte.

**Relay.** `loom-hooks/loom-relay.sh` `relay_kind_at` maps `stage:dispute-findings|dispute-contract|dispute-integrity`
to `file-dispute`. A new `RequestKind` needs a `relay_kind_at` case, a `drop_control_kinds` entry when
`is_control()`, and a `loom-hooks/tests/*.sh` test registered in `run-all.sh` (a static `run_test` list, not a
glob) in the same change; the dispute kinds shipped without the first, and the regression test never ran until
the second was found.

**Carried findings.** A `defer` makes the finding a carried finding of the target stage (`origin/F-1-2`); the
target stage's review gate counts it open until a later round resolves it or a ruling closes it.
