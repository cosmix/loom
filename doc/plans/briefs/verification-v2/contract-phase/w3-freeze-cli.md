# contract-phase / W3 — `loom stage contracts freeze | show | restore`

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D5 (trait and `classify`), D8.
Knowledge: `patterns/stage-daemon-channels.md` (how a stage agent reaches the daemon);
`mistakes/sandbox-state-channels.md`. Code:
`loom/src/commands/stage/dispute_criteria.rs` (283 lines) is the transport to mirror: socket
first (`dispute_via_socket` L97, `try_send_request`), relay (`dispute_via_relay` L136),
spool fallback (`queue_dispute_request` L199).

Pinned from W4: `Request::FreezeContracts { auth_token, stage_id, session_id, reports: Vec<ContractRunReport> }`,
`ContractRunReport { contract_id: String, adapter: Option<String>, outcome: String, exit_code: Option<i32> }`
(outcome is `failed`, `build_failed` or `exit_nonzero_unverified`),
`Response::ContractsFrozen { files: usize }`, relay request kind `FreezeContracts`, and
`crate::verify::contracts::store::{load_freeze, frozen_file_path(work_dir, stage_id, rel: &str) -> PathBuf, FreezeRecord}`.

## Files you own

`loom/src/cli/types_stage.rs` (392 lines), `loom/src/cli/types_stage_amend.rs` (new),
`loom/src/cli/types_stage_contracts.rs` (new),
`loom/src/cli/dispatch_stage.rs` (`dispatch_stage` 51 ledgered: pay for the arm by extraction),
`loom/src/commands/stage/mod.rs`, `loom/src/commands/stage/contracts.rs` (new module root),
`loom/src/commands/stage/contracts/{freeze,show,restore}.rs` (new), `loom/src/sandbox/settings.rs`
(445 ledgered; only the `STATE_READ_DIRS` array at L39, edited in place).

## Tasks

0. Make room first: three later stages add variants to `types_stage.rs`, which must stay
   ≤ 400 lines. Move `AmendField`, `AmendOp` and their `to_field`/`to_patch` impls (L14-61) into
   `types_stage_amend.rs`, re-exported so no importer changes.
1. CLI: `loom stage contracts freeze <stage-id>`, `show <stage-id>`,
   `restore <stage-id> [--contract <id>]` (`ContractsCommands` in `types_stage_contracts.rs`).
2. `freeze.rs`: design the core as a pure function the tests drive:

   ```rust
   pub(crate) struct FreezeInputs<'a> {
       pub contracts: &'a [ContractSpec],
       pub harness: &'a [String],
       pub changed_paths: &'a [String],   // relative to working_dir
       pub existing: &'a dyn Fn(&str) -> bool,
   }
   pub(crate) fn check_changes(inputs: &FreezeInputs<'_>) -> Result<(), Vec<String>>;
   pub(crate) fn judge_run(contract: &ContractSpec, adapter: Option<&dyn TestRunnerAdapter>, summary: RunSummary, exit: Option<i32>) -> Result<ContractRunReport, String>;
   ```

   `execute` wires the real inputs. Load the stage exactly the way `commands/stage/complete.rs`
   does inside a session. Changed paths are the committed diff against the stage base,
   tracked working-tree changes and untracked files (reuse `git::branch::status::list_working_tree_changes`
   and the diff pattern of `verify/wiring_detection.rs:110-115`), minus
   `git::worktree::is_worktree_scaffold_path`. Each contract runs through the criteria
   executor (`verify/criteria/executor.rs`, confined, `working_dir`, 300 s) with
   `adapter.single_test_command(file, test, package_dir)`. Then apply the D8 rules. Finally send
   the request over the three channels, mirroring `dispute_criteria.rs`. Its own messages tell
   the agent what failed and how to fix it.
3. `show.rs`: print the freeze record (contracts with outcomes, frozen files with hashes) and
   the frozen copy paths. `restore.rs`: copy frozen files back into the worktree, only paths
   inside the worktree, refusing symlink targets (`fs::safe_read` has the bounded-read pattern).
4. `sandbox/settings.rs` L39: `STATE_READ_DIRS` gains `"contracts"` (array length updated in
   place).

## Named tests (binding), in `commands/stage/contracts/freeze.rs`'s test module

- `freeze_rejects_non_contract_changes`: changed paths include `src/lib.rs` beside the contract
  file → error naming `src/lib.rs`.
- `freeze_rejects_passing_contract`: `judge_run` with a summary classified `Passed` → error
  containing `passes before implementation`.
- `freeze_rejects_unselected_contract`: `executed == Some(0)` → error containing
  `did not select`.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib commands::stage::contracts`

## Report

Files changed; exact new counts of `dispatch_stage` and `sandbox/settings.rs`; the proof result.
