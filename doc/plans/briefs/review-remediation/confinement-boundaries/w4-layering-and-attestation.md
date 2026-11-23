# confinement-boundaries / w4-layering-and-attestation: three upward imports, attestation key mode

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/runtime.md`, section CR-32;
`doc/plans/briefs/review-remediation/validation/verification.md`, section CR-36.

This unit runs no `git` command, touches no path under `.loom/`, and runs no tests. Its proof is
`cargo check --all-targets` from `loom/`. The orchestrator runs the tests named below.

## Findings covered

- CR-32 (three moves): `handoff` imports HMAC from `commands::stage`; `subagent_lifecycle` imports
  a ledger type through a `commands` shim; `stage_service` imports its own `ContractRunReport`
  back through `daemon`. The `process::sandbox_probe` import of `SUN_PATH_MAX` and
  `socket_path_fits` from `daemon`, the `handoff`/`orchestrator` cycle and the `models` edges stay
  as they are: moving the socket names touches files that four other stages of this plan own.
- CR-36 (narrowed): `refuse_unsafe_mode` accepts a group- or world-writable attestation key,
  whose writer could choose the key and forge attestations.

## Files

Own (write):
`loom/src/utils/hmac.rs` (new); `loom/src/utils.rs`; `loom/src/commands/stage/admin_hmac.rs`
(delete); `loom/src/commands/stage/mod.rs`; `loom/src/commands/stage/admin_proof.rs`;
`loom/src/handoff/completion/attest.rs`; `loom/src/handoff/completion/attest_tests.rs`;
`loom/src/subagent_lifecycle/claude.rs`; `loom/src/stage_service/contracts_choices.rs`;
`loom/src/stage_service/dispute_replacement_tests.rs`.

Read only: `loom/src/provider_data/ledger.rs`, `loom/src/commands/subagents/ledger.rs`,
`loom/src/stage_service/mod.rs`, `loom/src/verify/criteria/confine_scratch/reclaim.rs`.

Frozen (contract test, never edit): `loom/src/handoff/completion/attestation_key_contract_tests.rs`.

## Changes

### 1. HMAC to `utils` (CR-32)

- Create `loom/src/utils/hmac.rs` with the whole content of `commands/stage/admin_hmac.rs`
  (`hmac_sha256`, `constant_time_eq`, `SHA256_LEN`, `SHA256_BLOCK_LEN`). `hmac_sha256` and
  `constant_time_eq` stay `pub(crate)`; `SHA256_LEN` becomes a private `const`. Rewrite the module
  doc: HMAC-SHA256 and constant-time comparison shared by operator proofs
  (`commands::stage::admin_proof`) and completion attestations (`handoff::completion::attest`);
  pinned against the RFC 4231 vector in `admin_proof`'s tests.
- Register it in `loom/src/utils.rs` as `pub(crate) mod hmac;` next to the other `mod` lines.
- Delete `commands/stage/admin_hmac.rs` and its `pub(crate) mod admin_hmac;` line in
  `commands/stage/mod.rs`.
- `commands/stage/admin_proof.rs`: `use crate::utils::hmac::{constant_time_eq, hmac_sha256};`.
  Its test file (`commands/stage/tests/admin_proof.rs`, `use super::*`) needs no edit.
- `handoff/completion/attest.rs`: same import.

### 2. `StartedAgentTypeIndex` (CR-32)

`subagent_lifecycle/claude.rs`: `use crate::provider_data::ledger::StartedAgentTypeIndex;`. The
`commands::subagents::ledger` re-export stays: `commands/usage` and several tests still use it.

### 3. `ContractRunReport` (CR-32)

`stage_service/contracts_choices.rs` and `stage_service/dispute_replacement_tests.rs`: import
`crate::stage_service::ContractRunReport` instead of `crate::daemon::ContractRunReport`. The
`daemon` protocol re-export stays (it is the wire type) and so do its users in `verify` and
`host_verify`.

### 4. Attestation key mode (CR-36)

`handoff/completion/attest.rs::refuse_unsafe_mode`, using the metadata it already fetches:
- `metadata.uid() != geteuid()` (`// SAFETY: geteuid has no preconditions`) bails
  `completion attestation key is owned by uid {uid}, not by this user ({euid})`. It costs one
  comparison; for a non-root daemon a foreign-owned key it can read already fails the mode check,
  so this matters when loom runs as root.
- `mode & 0o044 != 0` keeps its message `completion attestation key is readable by group or other`.
- `mode & 0o022 != 0` bails `completion attestation key is writable by group or other`.
- `random_key` (two v4 UUIDs, 244 random bits from the OS generator) stays; add no dependency.
Pattern to follow: `verify/criteria/confine_scratch/reclaim.rs:80`.

## Interfaces shared with other workers

None.

## Tests (written by this unit, run by the orchestrator)

- `handoff/completion/attest_tests.rs`, beside `attestation_key_refuses_group_readable_file`:
  `attestation_key_refuses_group_writable_file` (`0o620`) and
  `attestation_key_refuses_world_writable_file` (`0o602`): each asserts `attestation_key` errs and
  the error text contains "writable by group or other". Both return the key today.
- Contract to turn green: `group-writable-attestation-key-accepted`.

## Proof

`cargo check --all-targets` from `loom/`. The orchestrator runs
`cargo test --lib handoff::completion::`, `cargo test --lib commands::stage::admin_proof` and
`cargo test --lib stage_service::`.

## Traps

- Moving `SUN_PATH_MAX` or `socket_path_fits` anyway: their importers belong to other stages.
- Leaving a re-export of the HMAC functions at the old `commands::stage` path.
- Merging the readable and writable checks into one message: each cause keeps its own text.
- Changing `random_key`.
