# check-routes W3: filing a criterion dispute for any list, and for a predecessor's list

Tier: opus. Wave 2, after W0. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `tests/check_routes_contracts.rs`.

## Why

`loom stage dispute-criteria` reaches the daemon two ways: the socket RPC
(`Request::DisputeCriteria` in `src/daemon/protocol.rs`, served by
`src/daemon/server/stage_control.rs` after `self_service.rs` admits the caller) and the relay
spool a sandboxed session writes (`src/relay/payload.rs` decodes it,
`src/fs/stage_request/apply.rs::apply_request` applies it). Both end in
`crate::stage_service::dispute::handle_dispute_criteria_identified`, which validates the
indices against the FILER's own lists (`refusal`). Integration-verify re-runs completed
stages' wiring and reachable checks, so it must be able to name a predecessor's list: the
`owner`.

## Edits (you own these files)

1. `src/stage_service/dispute.rs`
   - `handle_dispute_criteria_identified`'s `criterion` tuple becomes a struct
     `pub(crate) struct CriterionTarget { pub field: CriterionField, pub criterion_indices:
     Vec<usize>, pub owner: Option<String> }`; callers pass it.
   - `refusal` measures the filer's list for every `CriterionField` (singletons:
     `usize::from(stage.dead_code_check.is_some())`, same for `regression_test`; `Live` keeps
     the live plan's `stage.live.len()`).
   - With `owner: Some(owner)`, in this order, before any file under the owner is read:
     `validate_id(owner)` (`"invalid owner: <error>"`), the filer is
     `StageType::IntegrationVerify` (`"only an integration-verify stage disputes another
     stage's checks (--owner)"`), `owner != stage_id` (`"--owner names the disputing stage
     itself; drop --owner"`), `field.reexamined_by_integration()` (`"--owner takes --field
     wiring or --field reachable"`), the owner's record loads and is `StageStatus::Completed`
     (`"stage '<owner>' is <status>, not completed: integration-verify re-verifies only
     completed stages"`), and every index is inside the OWNER's list (`"criterion index <i> out
     of range for stage '<owner>' --field <f> (it has <n> entries)"`). Every refusal is a
     `Response::Error` and writes nothing.
   - The record's kind is `DisputeKind::owned_criterion(owner, field, indices)`; the filer's
     criterion budget (`dispute_count`) is spent and only the FILER moves to
     `NeedsAdjudication`. The owner record is never written.
   - Add `pub fn handle_owned_dispute_criteria(work_dir: &Path, stage_id: &str, owner: &str,
     field: CriterionField, criterion_indices: Vec<usize>, reason: String) -> Result<Response>`
     (no evidence commit, no failure output, no request id), exported from
     `src/stage_service/mod.rs` beside `handle_dispute_criteria` (public path
     `loom::stage_service::handle_owned_dispute_criteria`). Do not touch
     `src/daemon/server/mod.rs` or `src/daemon/mod.rs`: spawn-recovery, a concurrent stage,
     edits them.
2. `src/stage_service/types.rs`: `StageRequest::Dispute` (the `dispute-criteria` request) gains `#[serde(default,
   skip_serializing_if = "Option::is_none")] owner: Option<String>`; update the doc line that
   spells the command.
3. `src/daemon/protocol.rs` and `src/daemon/protocol_debug.rs`: `Request::DisputeCriteria`
   gains the same optional `owner` (the debug form prints it).
4. `src/daemon/server/stage_control.rs`, `src/daemon/server/self_service.rs`,
   `src/fs/stage_request/apply.rs`: pass `owner` through to `CriterionTarget`.
5. `src/relay/payload.rs`: the dispute-criteria payload decodes into
   `StageRequest::Dispute` (`decode_stage_request`), so the owner field is step 2's and
   this file gains only a decode test with and without `owner` (`deny_unknown_fields` stays).
   The file is 379 lines plus the live plan's `HostJobRequest.live` (R1): keep it at or under
   400 (`wc -l`).
6. `src/commands/stage/dispute_transport.rs` and `src/commands/stage/dispute_criteria.rs`:
   `Dispute::criterion(...)` and `dispute_criteria(...)` take `owner: Option<String>` and put
   it on the RPC and the relay payload.
7. `src/cli/dispatch_stage.rs`: pass `CriteriaDispute::Entry { owner, .. }` through.
8. Existing tests in files you own that build `DisputeKind::Criterion` or call the changed
   functions: add `owner: None` / the new argument. In `src/daemon/server/dispute_tests.rs`,
   `dispute_multi_tests.rs`, `src/fs/stage_request/tests/apply.rs`, `tests/spool.rs`,
   `src/daemon/wire_tests.rs`, `src/daemon/server/self_service_tests.rs`,
   `src/daemon/server/tests/self_service_client.rs`, `src/commands/stage/dispute_criteria_tests.rs`
   change only what the compiler requires, and list every assertion line you changed (the
   stage disputes the resulting `TI-edit` events together). Add one test per owner refusal in
   `src/daemon/server/dispute_tests.rs`.

## Traps

- The owner id arrives from the wire: validate it BEFORE `load_stage(owner, ..)` joins it
  into a path (contract `owner-id-traversal-refused`).
- The per-stage dispute lock is the FILER's (`lock_stage_disputes(work_dir, stage_id)`); never
  take the owner's dispute lock: the owner files nothing.
- A refused filing must leave no `request.md` and spend no budget: every check runs before
  `write_request`.

## Done

Your one check after the stage compiles: `cargo test --lib stage_service::`.
