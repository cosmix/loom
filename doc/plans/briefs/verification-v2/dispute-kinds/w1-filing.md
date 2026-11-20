# dispute-kinds / W1 — dispute model, CLI, daemon filing, budgets

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D15. Knowledge:
`architecture/adjudication-lifecycle.md` (all); `conventions/dispute-and-adjudication.md`
(all); `mistakes/adjudication-autonomy-deadlock.md` "A Verdict Re-queued the Stage While
Another Dispute Was Unanswered". Code: `models/dispute.rs` (185 lines; `DisputeRequest`,
`DisputeVerdict`, layout helpers); `daemon/server/dispute.rs` (510 lines ledgered;
`handle_dispute_criteria` L38-213, 176 lines ledgered); `commands/stage/dispute_criteria.rs`
(283 lines; the transport); `daemon/protocol.rs` (`Request::DisputeCriteria` L121-129,
`debug_dispute` L209); `models/stage/methods.rs` (`dispute_budget_exhausted` L394-404,
`try_request_adjudication` L372-392).

## Files you own

`loom/src/models/dispute.rs`, `loom/src/models/stage/types.rs` (ledgered),
`loom/src/models/stage/methods.rs` (ledgered), `loom/src/models/stage/dispute_budgets.rs` (new),
`loom/src/cli/types_stage.rs` (≤ 400 lines), `loom/src/cli/types_stage_amend.rs`,
`loom/src/cli/types_stage_disputes.rs` (new),
`loom/src/cli/dispatch_stage.rs` (`dispatch_stage` 51 ledgered), `loom/src/commands/stage/mod.rs`,
`loom/src/commands/stage/dispute_criteria.rs`, `loom/src/commands/stage/dispute_transport.rs` (new),
`loom/src/commands/stage/dispute_kinds.rs` (new), `loom/src/daemon/protocol.rs`,
`loom/src/daemon/server/mod.rs`, `loom/src/daemon/server/dispute.rs`,
`loom/src/daemon/server/dispute_kinds.rs` (new), `loom/src/relay/{matrix,kind,payload,tests_matrix}.rs`,
`loom/src/fs/stage_request/{types,apply}.rs`, `loom/src/orchestrator/core/inbox_drain/{apply,tests_matrix}.rs`.

## Types you publish (pinned for W2, W3 and the prompt units)

```rust
// models/dispute.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingSnapshot { pub id: String, pub origin_stage: Option<String>, pub round: u32, pub finding: crate::verify::review::report::Finding }
pub type IntegritySnapshot = crate::verify::integrity::IntegrityEvent;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DisputeKind {
    Criterion { criterion_index: usize },
    Findings { finding_ids: Vec<String>, evidence: Vec<FindingSnapshot> },
    Contract { contract_id: String },
    Integrity { event_ids: Vec<String>, evidence: Vec<IntegritySnapshot> },
}
// DisputeRequest: `criterion_index` moves into `kind` (no migration: loom takes none).
// daemon/protocol.rs
Request::FileDispute { auth_token: String, stage_id: String, session_id: String, kind: DisputeKind, reason: String, evidence_commit: Option<String> }
// Response::DisputeCreated { id } is reused.
```

## Tasks

1. `models/dispute.rs` as pinned. Every existing reader of `criterion_index` reads it from
   `DisputeKind::Criterion`. Find them with `rg -n criterion_index loom/src loom/tests`.
   `request.md` frontmatter carries `kind`.
2. Budgets: `Stage.finding_disputes`, `contract_disputes`, `integrity_disputes` (`u32`, serde
   default 0), with `dispute_budgets.rs::exhausted(stage, kind) -> bool` (3 each) and
   `spend(stage, kind)`. Pay for the added `Stage` lines in `models/stage/types.rs` by moving the
   existing dispute-budget methods (`dispute_budget_exhausted` and neighbours, methods.rs:394-421)
   into `dispute_budgets.rs`. Lower the ledger entries.
3. Transport: extract the socket → relay → spool logic of `dispute_criteria.rs` into
   `dispute_transport.rs` so `dispute-criteria` and the three new commands share it. The
   `dispute-criteria` user interface and behaviour stay identical.
4. CLI (`types_stage_disputes.rs` holds the argument structs; `types_stage.rs` gets three one-line
   variants: `DisputeFindings(..)`, `DisputeContract(..)`, `DisputeIntegrity(..)`):
   - `loom stage dispute-findings <stage> --finding <id>... --reason <text>`;
   - `loom stage dispute-contract <stage> --contract <id> --reason <text>`;
   - `loom stage dispute-integrity <stage> --event <id>... --reason <text>`.

   The client snapshots the evidence (findings from `verify::review::store::open_findings`,
   events from `verify::integrity::current_events`) into the request.
   Also add `AmendField::Contracts` to the `loom stage amend` mirror (contract-phase moved it
   to `types_stage_amend.rs`), mapped in `to_field` to `AmendmentField::Contracts`, which W2
   adds.
5. Daemon `dispute_kinds.rs::handle_file_dispute`, mirroring `handle_dispute_criteria`: validate
   `stage_id` first, canonical `work_dir`, the per-stage flock, then:
   - every id exists: an open finding (own or carried), a frozen contract (`load_freeze`), or a
     current event. The daemon re-derives the open findings itself and does not trust the
     client's snapshot for existence;
   - the per-kind budget; exhausted ⇒ `NeedsHumanReview` exactly like the criterion path;
   - `request.md` written create-new;
   - `NeedsAdjudication` via `try_request_adjudication`.

   `handle_dispute_criteria` is ledgered: route the new request in `daemon/server/mod.rs` without
   growing it.
6. Relay and spool, mirroring every place `Dispute` appears
   (`rg -ln "StageRequest::Dispute|RequestKind::Dispute" loom/src`): `RequestKind::FileDispute`
   (`relay/kind.rs`) and its payload, `StageRequest::FileDispute` applied by
   `fs/stage_request/apply.rs`, and matrix rows for every session type (allowed for `Stage`;
   refused for `Contract`, `Adjudication`, `Merge`, `Knowledge`), applied in
   `inbox_drain/apply.rs` like the criterion dispute, with the matrix tests updated.

## Named tests (binding)

- `finding_dispute_records_kind_and_evidence` (`daemon/server/dispute_kinds.rs` tests): a v2
  stage with one open finding `F-1-1`; filing a findings dispute writes `request.md` with
  `kind: findings`, the id and the snapshot, and moves the stage to `NeedsAdjudication`.
- `finding_dispute_budget_escalates_to_human_review`: the fourth findings dispute moves the stage
  to `NeedsHumanReview` and writes no request.
- `criterion_dispute_behaviour_is_unchanged`: a `dispute-criteria` filing produces the same
  `request.md` fields as before (with `kind: criterion`) and the same transition.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib daemon::server::dispute`

## Report

Files changed; exact new counts of every ledgered item touched; the proof result.
