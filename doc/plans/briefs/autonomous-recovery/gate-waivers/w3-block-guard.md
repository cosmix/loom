# gate-waivers W3: the block guard refuses every failure that has a dispute route

Tier: opus. Wave 2, after W0. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `src/stage_service/recovery_contract_tests.rs`.

## Why

`src/stage_service/control_block.rs::handle_agent_block_stage` refuses an agent's block only
while the latest signed completion attempt has a failing `acceptance-<i>` criterion, or a
failing `live` check that the live-test-lane plan's rule refuses. That plan's live-checks R2
split the old `failing_criterion_indices` into `latest_failing_labels(work_dir, stage_id) ->
Vec<String>` (every failing label of the current session's latest trusted `ToolFailed`
attempt) and the acceptance indices derived from it, and added, when the labels hold `live`, a
call of `crate::live::completion::block_refusal(&stage.id, &labels, &stage.live,
receipt.as_ref(), current.as_ref())` with the receipt from `crate::live::receipt::load` and
`current` from `crate::live::receipt::current_binding` of the stage worktree
(`crate::git::worktree::stage_worktree_path`). It also added two tests to
`control_block_agent_tests.rs`. Every other failing check (goal-backward gaps, after-stage,
contracts, impact tests, loom's own gates, integration-verify's re-verification) and every open
review finding or integrity event passes the guard, so an agent can park a stage for a human on
a check that has a dispute route (`doc/loom/knowledge/mistakes/completion-runner-blocks.md`,
top section).

## Edits (you own these files)

1. New `src/stage_service/block_guard.rs`: `pub(crate) fn dispute_routes(work_dir: &Path,
   stage: &Stage) -> Vec<String>`, one line per route, in this order:
   - From the current session's latest trusted checkpoint, phase `ToolFailed`, each failing
     label (`CriterionResult { passed: false }`; read them with R2's `latest_failing_labels`,
     moved here):
     - `acceptance-<i>`: one line `loom stage dispute-criteria <id> --criterion-index <i> [...]`
       naming every failing index;
     - `goal-backward`: `loom stage dispute-criteria <id> --field <list> --criterion-index <n>`
       for each `[<list> n]` gap `loom stage complete` printed;
     - `after-stage`: the same with `--field after-stage`;
     - `contracts`: `loom stage dispute-contract <id> --contract <contract-id>`, only while the
       stage has a freeze record (`crate::verify::contracts::store::load_freeze`); with none,
       `dispute-contract` would refuse, so the label gives no route;
     - `impact-tests`: `--field impact-tests --test <id>`, or `--field gate --gate
       impact-tests` when the run named no failing test;
     - `reachable`, `aggregated-wiring` (integration-verify): `--owner <stage> --field
       reachable|wiring --criterion-index <n>` for each `[<stage>/<list> n]` gap;
     - `unwired`, `duplicates`: `--field gate --gate <label>`;
     - `live`: the live plan's `crate::live::completion::block_refusal`, loaded exactly as R2
       loads it (move R2's receipt and binding read from `control_block.rs` into this file):
       `Some(text)` is one route whose text is pushed as is (it names `loom check --live` or
       `--field live --criterion-index <i>`); `None` (every failing required entry could not
       run for want of the live credential) is no route, so that block stays allowed. Never
       re-derive the rule here; Decision 9 and contract `block-keeps-the-live-credential-rule`
       pin it;
     - any other label (`progress-truncated`, an unknown one): no route.
   - Plan version 2, whatever the checkpoint: open review findings
     (`crate::verify::review::store::open_findings`) give `loom stage dispute-findings <id>
     --finding <ids>`; current integrity events not yet accepted
     (`crate::stage_service::observer::integrity_events`; read how acceptance is recorded and
     keep only unaccepted ones) give `loom stage dispute-integrity <id> --event <ids>`.
   - An unreadable checkpoint, store or record yields no route for that source and a
     `tracing::warn!`: the agent's way to stop must not hinge on a read.
2. `src/stage_service/control_block.rs`: `handle_agent_block_stage` refuses when
   `dispute_routes` is non-empty with `cannot block stage '<id>': its failing checks have a
   dispute route, so they are disputed, never blocked:` followed by one `  - <route>` line each,
   then: `A filing past a dispute budget goes to the operator automatically. A check that
   cannot pass until a person installs a host tool, provides a credential or decides is
   disputed too: the adjudicator blocks the stage with that need; a required live entry that
   could not run for want of the live credential is not refused.` Delete the acceptance-index
   helper R2 left and `failed_completion_refusal`; the live refusal now comes from the guard, so
   `control_block.rs` keeps no second call of `block_refusal`. Rewrite the doc comments to the
   current behaviour.
3. The orchestrator's foundation already created `block_guard.rs` and declared it in
   `src/stage_service/mod.rs`; you fill the file and never edit `mod.rs`.
4. `src/stage_service/control_block_agent_tests.rs`: existing tests keep passing where the
   behaviour is unchanged (an acceptance failure still names `--criterion-index 2`, and the live
   plan's two live tests still block on `Unavailable { Credential }` and refuse on `Failed` with
   `--field live`); change only assertion text the new message changes, list each changed line.
   Add tests for each label route and for the v2 findings route.

## Traps

- The guard reads; it never files or blocks. A block that passes the guard goes through
  `handle_block_stage` unchanged, so an operator's block and the daemon's signing block are
  never refused (they call `handle_block_stage` directly).
- The checkpoint belongs to the stage's CURRENT session (`stage.session`): an earlier
  session's failure does not refuse a new session's block.

## Done

Your one check after the stage compiles: `cargo test --lib stage_service::`.
