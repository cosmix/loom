# W2: Re-freeze the Whole Frozen Set on an Accepted Contract Dispute

Tier: opus. Files owned are listed in the stage's worker table. Read `common.md` first.

## Defect (verified at `1b5a0c91` against host-jobs dispute 2)

- The first freeze stores every contract file plus every existing harness-glob match (`stage_service/contracts.rs:279-300` `read_files`, via `verify/contracts/changes.rs:167` `harness_files`).
- `apply_contract::accept` (`orchestrator/adjudication/apply_contract.rs:27-65`) collects only contract files: `contract_file`, `recorded_file`, and the `file` of each `refrozen_contracts` entry.
- `store::refreeze` (`verify/contracts/store.rs:238-276`) upserts only the paths it is given.

As a result, a harness file the accepted change edited keeps its old hash. The completion hash check (`verify/contracts/completion.rs:104-126`) then fails forever, and `loom stage contracts restore` writes back a stale file that can break the build. The adjudicator prompt shows only the contract file (`orchestrator/adjudication/prompt/contract.rs:210-223`), so the judge never sees the harness change.

## Steps

1. **Shared reader.**
   - Move `read_files` and its caps (`MAX_FROZEN_FILES` 500, `MAX_FROZEN_FILE_BYTES`, `MAX_FROZEN_TOTAL_BYTES`, the no-symlink `read_bounded`) into a new `verify/contracts/frozen_set.rs`, as `pub fn read_frozen_set(...) -> Result<Vec<(String, Vec<u8>)>>`.
   - Inputs: the stage's contract files, the re-expanded harness matches, and optionally extra recorded paths.
   - Rules: a missing contract file is an error; a missing harness or recorded path is skipped.
   - Declare the module in `verify/contracts/mod.rs`. `stage_service/contracts.rs` calls it; the first freeze's behaviour is unchanged.
2. **Store.** Add `store::refreeze_set(work_dir, stage_id, files, contracts)`:
   - It replaces `record.files` wholesale with exactly `files`, so a path absent from `files` is dropped.
   - It removes the dropped files' copies under `files/`.
   - It upserts `contracts` the way `refreeze` does, under the same `locked_dir_update`.
   - Keep `refreeze` only if a caller remains; otherwise delete it (no orphans).
3. **Accept.**
   - `accept` builds the set from: every contract file of the amended `stage.contracts`; `harness_files(git, working_dir, &stage.harness)` re-expanded now; and every `record.files` path.
   - It reads them through `read_frozen_set` and calls `refreeze_set`. `refrozen_contracts` is unchanged.
   - The feedback message lists the re-frozen paths and names any dropped ones.
   - A retry after a partial failure gives the same record, because the set derives from the stage and the worktree.
4. **Evidence.** In `prompt/contract.rs`, include for every frozen path whose worktree bytes differ from the frozen copy:
   - a bounded unified diff (reuse the diff helper the contract file already uses);
   - a line naming each deleted harness file.

   Add a test in `prompt/contract_tests.rs`.
5. **Unit tests.**
   - In `apply_contract_tests.rs`, using `fixture`, `frozen_hash` and `write_worktree_file`, with harness `tests/support/**`:
     - a changed harness file is re-frozen;
     - a new harness file is added;
     - a deleted one is dropped.
   - In `completion_tests.rs`: after such an accept, the hash check reports no changed frozen file.
6. **Doctrine note for W5** (report it; do not edit): `skills/loom-plan-writer/references/v2-contracts.md:129` says an accepted contract dispute "re-freezes that contract at its current content". The new truth is that it re-freezes the stage's contract and harness files at their current content and drops deleted harness files.

## Contracts you must turn green

`refreeze-covers-changed-harness-files` and `refreeze-drops-deleted-harness-files`.

## Proof

`cargo test --lib verify::contracts::`, `cargo test --lib orchestrator::adjudication::`, `cargo test --test gate_dedup_contracts refreeze`.
