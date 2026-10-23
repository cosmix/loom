# refusal-adjudication: shared context

Read this first, then your own brief, then `doc/loom/knowledge/mistakes/adjudication-autonomy-deadlock.md` (its prevention rules bind every lifecycle path here). The plan's Decisions 6-9 are authoritative. Paths are relative to `loom/` unless they start with `../`.

## Rule

A stage never waits on a human for something loom or an agent can resolve, and no agent settles a disputed constraint itself. A refused contract freeze therefore becomes a dispute that an independent adjudicator decides and justifies. Nothing in the contract phase parks the stage on `WaitingForInput`.

## Pinned types and names

- `DisputeKind::FreezeRefusal { problems: Vec<String> }` in `src/models/dispute.rs` (enum at about L85, `#[serde(tag = "kind", rename_all = "kebab-case")]`, so the tag is `freeze-refusal`). `problems` is the refusal's problem list, each flattened with `context::untrusted::inline_safe`, at most 32 entries of 1 KiB.
- `impl DisputeKind { pub fn filed_by(&self, session: SessionType) -> bool }`: `FreezeRefusal` only by `SessionType::Contract`; every other variant only by `SessionType::Stage`.
- Budget: `DisputeTally.freeze_refusal_disputes` (`src/models/stage/dispute_budgets.rs`), cap `MAX_DISPUTES_PER_KIND` (3). Exhausted: `NeedsHumanReview` with reason "contract freeze refused 3 times; adjudication could not resolve it: <last problems>. Decide with: loom stage human-review <stage-id>".
- Admission: a `FreezeRefusal` dispute requires a Contract session that owns the stage (`stage_service/ownership.rs::session_owns_stage_as`), a v2 standard stage with contracts, no freeze record (`verify::contracts::store::load_freeze` is None), and status `Executing` (a legacy `WaitingForInput` stage is first resumed to `Executing` in the same update, then `try_request_adjudication`).
- The daemon-side filing helper: `pub(crate) fn file_refusal_dispute(work_dir: &Path, stage_id: &str, session_id: &str, problems: &[String]) -> Result<()>` in `src/verify/contracts/refusal.rs`. It runs the same admission and budget path as a relayed or socket `FileDispute` (call `stage_service::dispute_kinds::handle_file_dispute` or its inner function; read it first), logs every decline at warn level with the reason (prevention rule 7), and never returns `Err` for a refusal the drain would retry forever (see the `Response::Error` vs `Err` rule in `stage_service/contracts.rs` module doc L38-40).
- CLI: `loom stage contracts dispute-refusal <stage-id>`, in `src/commands/stage/contracts/dispute_refusal.rs`, reads the refusal file (`verify::contracts::refusal::REFUSAL_FILE` under `$LOOM_SCRATCH_DIR`) and sends `FileDispute { kind: FreezeRefusal { problems } }` through `commands/stage/dispute_transport.rs::send`. Relay token: `stage:contracts` with third word `dispute-refusal` → `file-dispute` in `../loom-hooks/loom-relay.sh` `relay_kind_at`.
- `AmendmentField::Harness` (kebab `harness`), value a plain glob string (`ParsedAmendmentValue::Harness(String)`).
- Verdict JSON for `freeze-refusal`:

```json
{"verdict": "reject", "justification": "<evidence-cited reason>", "instructions": "<what the next writer must change>"}
{"verdict": "accept", "justification": "<evidence-cited reason>", "plan_patch": [{"field": "harness", "op": "insert", "index": 0, "value": "tests/fixtures/**"}]}
{"verdict": "needs-more-evidence", "questions": ["..."]}
```

`plan_patch.field` is one of `contracts`, `harness`, `acceptance`; an `acceptance` patch may only replace or delete an entry that `verify::contracts::format_gate::is_format_check` recognises. Validation refuses (re-prompt): empty justification; reject without instructions; accept without a patch; any other field; a harness value for which `verify::contracts::collection::harness_source_matches` finds an existing production source file in the stage worktree.

## Flow after this stage

1. Writer runs `loom stage contracts freeze <id>` and stops.
2. Refusal at the CLI (local checks): the writer may fix and retry in the same turn; if it stops with the refusal file present, `commit-guard.sh`'s Stop hook runs `loom stage contracts dispute-refusal <id>`.
3. Refusal at the daemon (relayed admission, or the host contract run): the daemon calls `file_refusal_dispute`.
4. Stage → `NeedsAdjudication`; the verdict-apply path retires the writer (`retire_disputing_agents`, already session-type agnostic), applies the verdict, writes the feedback notice, requeues. `first_agent_kind` (`orchestrator/core/stage_spawn.rs:44-60`) starts a Contract writer because no freeze exists; its signal carries the notice.
5. A retired writer is not a crash: `contract_phase_event` requires `Executing`, so the contract respawn budget is not charged. A test pins this.
