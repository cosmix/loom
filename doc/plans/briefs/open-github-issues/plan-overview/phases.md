# Brief: phases (stage phase spans)

Tier: `loom-software-engineer` (sonnet). Wave 1. Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/models/stage/phases.rs` (new), `loom/src/models/stage/phases_tests.rs` (new), `loom/src/models/stage/mod.rs`, `loom/src/models/stage/types.rs`, `loom/src/verify/transitions/persistence.rs`.

Frozen, read-only: `loom/src/models/stage/phases_contract_tests.rs`.

## Facts (verified)

- `Stage` is `models/stage/types.rs:207`; the file is 591 lines and its ledger row is 591, so adding the field needs an equal number of lines moved out (for example a doc block or a small impl into a sibling module). Never edit the ledger.
- `StageStatus` (types.rs:461): `Executing`, `NeedsAdjudication`, `Completed`, `Skipped`, `WaitingForDeps`, `Queued`, and others.
- `SessionType` (`models/session/types.rs:11-28`): `Stage`, `Contract`, `Adjudication`, and others.
- Every persisted stage write goes through `verify/transitions/persistence.rs::update_stage_file_locked` (152-180), which already compares status and session before and after and calls `record_status_change` (184-207). The session type is known there when the session changed (173-176); read `stage.session_type` or the session record the same way `record_status_change` does.
- `retry_count` (types.rs:261).

## Change

1. `phases.rs`: `Phase`, `PhaseSpan`, `phase_for`, `record`, and `MAX_PHASE_SPANS = 512`, exactly as the stage PUBLIC SURFACE states. `phase_for` mapping: Executing + Contract session -> Contract; Executing + Stage session -> Executing when `retry_count == 0`, else Retry; NeedsAdjudication -> Adjudication; Completed and Skipped -> None; WaitingForDeps with no span yet -> None; every other status -> Waiting.
2. `Stage.phases: Vec<PhaseSpan>` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`.
3. `update_stage_file_locked`: after `modify` succeeds, when the status, the session id or the session type changed, call `phases::record(&mut stage.phases, phase_for(..), Utc::now())` before the write. One call site; the 74 raw `stage.status = ..` assignments elsewhere all reach the disk through this function.
4. `mod.rs`: `pub mod phases;` and the test module declarations.

## Tests (`phases_tests.rs`)

`record` opens, closes, ignores an unchanged phase, closes only on None, and extends the last span at 512. `phase_for` covers every row of the mapping.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib models::stage::` and `--lib verify::transitions::` pass, including the frozen contract; `cargo build --manifest-path loom/Cargo.toml --all-targets` clean.

## Traps

- The status can stay `Executing` while the session switches from Contract to Stage: that switch must close the Contract span (the frozen contract checks it).
- Do not record a span from `create_stage`.
