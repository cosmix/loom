# W5: freeze as a host job, apply glue, stall suppression, end-to-end test (stage daemon-completion)

Read `doc/plans/briefs/host-verification/common.md` first. Sections 4.3, 4.4 (Freeze arms), 4.5 (freeze text, stall rule) and 4.6 (freeze seams) are your specification. Paths are relative to `loom/`. Stage `host-jobs` merged `src/host_verify/`; read it first.

## Territory

You own:

- `src/host_verify/freeze.rs` (new) and `tests_freeze.rs` (new);
- `src/commands/stage/contracts/freeze.rs`;
- `src/stage_service/{types.rs, contracts.rs, mod.rs}`;
- `src/daemon/server/{contracts.rs, contracts_tests.rs, stage_control.rs}` and `src/fs/stage_request/apply.rs`;
- `src/orchestrator/core/host_jobs.rs`;
- `src/orchestrator/monitor/{stall_line.rs, hung_latch.rs}`;
- `tests/host_verification_e2e.rs` (new).

W4 owns:

- `host_verify/{mod,child,apply}.rs`, which declares `pub mod freeze;`, calls `run_freeze_job` from the child and `apply_freeze_result` from apply;
- the drain arm that calls the new `handle_freeze_contracts`.

W6 owns `src/daemon/protocol.rs` and changes `Request::FreezeContracts` to `{.., head, tree_digest}` (common.md 4.6). Your `stage_control.rs` handler matches that shape.

## Steps

1. **Freeze job** (`host_verify/freeze.rs::run_freeze_job`):
   1. Load the stage.
   2. Run the change check, `commands/stage/contracts/freeze.rs::check_changes`, already `pub(crate)`.
   3. Require `tree_digest(worktree) == record.tree_digest`.
   4. Run every contract through the code `checked_reports`, `run_contracts` and `run_contract` use today. Move those functions into `host_verify/freeze.rs` or make them `pub(crate)` where they are; the CLI no longer calls them in a session.
   5. Then the format gate (`check_formatting`).
   6. Write the `ContractRunReport` list to `verification/<stage>/<job>/contract-reports.json` through an atomic, `O_NOFOLLOW` write; reuse the store's writer by making it `pub(crate)`, or ask W4 in your report if it lives in a file you do not own.
   7. Outcomes are `contract:<id>`, `passed` = the contract was classified red as the freeze requires (`RED_OUTCOMES`), with the run's verdict line as summary; plus `format`.
   8. `passed` = every outcome passed.
2. **Apply** (`apply_freeze_result`):
   1. Load the reports.
   2. On a `Passed` job, call `stage_service::contracts::freeze_with_reports(work_dir, stage, &record.session_id, &reports)`. That is today's `handle_freeze_contracts` body, renamed and otherwise unchanged.
      - `ContractsFrozen { files }` → `Applied::FreezeAccepted { files }`.
      - `Error { message }` → `verify::contracts::refusal::park_refused_relay(work_dir, stage, session, message)` and `Applied::FreezeRefused(message)`.
   3. On a `Failed`, `Cancelled` or `Interrupted` job, park with a message listing each failing outcome's label and summary (at most 4 KiB), and return `FreezeRefused`.
3. **Entry points.** The new `handle_freeze_contracts(work_dir, stage_id, session_id, head, tree_digest, request_id)` in `stage_service/contracts.rs`, re-exported through `daemon/server/contracts.rs` as today:
   - runs the admission checks `prepare` runs before it reads reports (stage in its contract phase, caller its contract writer; read `prepare` for the exact checks);
   - submits a `Freeze` job;
   - returns `Response::Ok` (queued or joined) or `Response::Error`.

   Route the spool (`fs/stage_request/apply.rs`) and socket (`daemon/server/stage_control.rs`) `FreezeContracts` paths to it with `new_job_id()` as request id. `StageRequest::FreezeContracts` becomes `{ head: String, tree_digest: String }`, keeping `deny_unknown_fields` or the enum's existing strictness; read `stage_service/types.rs` and `relay/payload.rs::decode_stage_request`.
4. **Freeze CLI** (`commands/stage/contracts/freeze.rs::freeze`):
   - In a contract session (Relay mode):
     1. Run `check_changes` locally and refuse with today's text on problems.
     2. Compute `head` and `tree_digest`.
     3. Relay `freeze-contracts` with the new payload (`end_turn: true`, as today's relayed freeze).
     4. Print the common.md 4.5 text, exit 0.
   - Operator mode, outside a session, keeps today's behaviour: run the contracts locally and freeze through `freeze_with_reports` directly or over the socket, whichever it does now; read it.
5. **Apply glue** (`orchestrator/core/host_jobs.rs`): in `tick_host_jobs`, for every record the runner returns, call `crate::host_verify::apply::apply_finished(&work_dir, &record)`.
   - Log the `Applied` value at `info`, an error at `warn`. Never propagate.
   - A `Completed` result needs nothing more. The monitor already detects the stage transition, retires the session and auto-merges (`orchestrator/core/completion_handler.rs`). Confirm that by reading it, and report it.
6. **Stall suppression** (`orchestrator/monitor/stall_line.rs`, `hung_latch.rs`). While `host_verify::store::load_current(work_dir, stage)` is non-terminal, the stage's sessions are not escalated. The `SessionHung` warning may still be raised; escalation, meaning handoff and kill through `recover_hung`, is held until the job is terminal or past its deadline.
   - Find the escalation decision (`is_stall_escalation`, `running_completion_line`).
   - Thread a `host_job_running: bool` (or the job's deadline) into it from the caller that has the stage and `work_dir`. Keep the read to one small file per stage per check.
   - Test the held case and the released case (job terminal, or past its deadline).
7. **End-to-end test** (`tests/host_verification_e2e.rs`). Mirror `tests/integration/relay_commit_e2e.rs` (fixture, `session_loom`, `run_hook`, `drain_once` through an in-process `Orchestrator`). Read `tests/integration/binary_spawn_guard.rs` first, and spawn the binary only in a way it permits.
   1. A committed stage worktree with acceptance `["true"]`.
   2. Run `loom stage complete stage-a` in the session environment: exit 3 and a ticket.
   3. Run the relay hook with that Bash payload.
   4. Drain once: the job is queued.
   5. Drive a `HostJobRunner` whose `RunnerConfig.program` is the built loom binary (`env!("CARGO_BIN_EXE_loom")`, if the guard allows; otherwise ask the guard's helper for the binary path) until the record is terminal, then call `apply_finished`.
   6. Assert the stage is `Completed`.
   7. A second test does the same with acceptance `["false"]` and asserts `Executing` plus a `ToolFailed` checkpoint.

   This test runs real confinement for its criterion. It passes on the host and in this plan's sessions.

## Tests you write

- `tests_freeze.rs`:
  - all contracts red → `Passed` and `FreezeAccepted`;
  - one contract green → `Failed` and the stage parked with its id in the reason;
  - a changed tree → error;
  - a request from a session that is not the stage's writer → `Response::Error`.
- The stall tests above.
- `contracts_tests.rs`: adjust to the new signature.

## Proof (run once, from `loom/`, after W4 and W6 have written; if the build fails only on their missing items, report that and stop)

```bash
cargo test --lib host_verify::tests_freeze stage_service:: orchestrator::monitor:: daemon::server::contracts 2>&1 | tail -30
cargo test --test host_verification_e2e 2>&1 | tail -20
```

## Report

Files changed; where the contract-running code now lives; the stall seam (`file:line`); how the e2e test spawns the binary under the guard; the proof output.
