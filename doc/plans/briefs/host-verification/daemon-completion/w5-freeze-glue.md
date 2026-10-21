# W5: freeze as a host job, apply glue, stall suppression, end-to-end test (stage daemon-completion)

Read `doc/plans/briefs/host-verification/common.md` first. Sections 4.3, 4.4 (Freeze arms and the startup apply), 4.5 (freeze text, stall rule) and 4.6 (freeze seams) are your specification. Paths are relative to `loom/`. Stage `host-jobs` merged `src/host_verify/`; read it first.

## Territory

You own:

- `src/host_verify/freeze.rs` (new) and `tests_freeze.rs` (new);
- `src/commands/stage/contracts.rs`, `src/commands/stage/contracts/freeze.rs` and `src/commands/stage/contracts/freeze_tests.rs`;
- `src/stage_service/{types.rs, contracts.rs, mod.rs}`;
- `src/daemon/server/{contracts.rs, contracts_tests.rs, stage_control.rs}`, `src/fs/stage_request/apply.rs` and `src/fs/stage_request/tests/replay.rs`;
- `src/orchestrator/core/host_jobs.rs`;
- `src/orchestrator/monitor/{stall_line.rs, hung_latch.rs, never_worked.rs}` and `src/orchestrator/core/event_handler/recover_hung.rs`;
- `tests/host_verification_e2e.rs` (new).

W4 owns:

- `host_verify/{mod,store,wait,child,apply}.rs`, which declares `pub mod freeze;` and `#[cfg(test)] mod tests_freeze;`, calls `run_freeze_job` from the child and `apply_freeze_result` from apply, and provides `apply::unapplied_terminal_records`;
- the drain arm that calls the new `handle_freeze_contracts`.

W6 owns `src/daemon/protocol.rs` and changes `Request::FreezeContracts` to `{.., head, tree_digest}` (common.md 4.6). Your `stage_control.rs` handler and `commands/stage/contracts.rs::send_freeze` match that shape.

## Steps

1. **Freeze job** (`host_verify/freeze.rs::run_freeze_job`):
   1. Load the stage.
   2. Run the change check, `commands/stage/contracts/freeze.rs::check_changes`, already `pub(crate)`.
   3. Require `git rev-parse HEAD == record.head` and `tree_digest(worktree) == record.tree_digest`.
   4. Hash the files the freeze will record: raise `stage_service/contracts.rs::read_files` (and the `Site`/`locate` it needs) to `pub(crate)` and hash what it returns; compute `contracts_digest` = sha256 of `serde_json::to_vec(&stage.contracts)` (common.md 4.3).
   5. Run every contract through the code `checked_reports`, `run_contracts` and `run_contract` use today: make them `pub(crate)` where they are (leave `check_formatting` in `freeze.rs`; `freeze_tests.rs` calls it through `super::`). The CLI no longer calls them.
   6. Then the format gate (`check_formatting`).
   7. Re-read HEAD, recompute the digest, re-hash the files and recompute `contracts_digest`; any change is an error `worktree changed during the run` naming what changed.
   8. Write `contract-reports.json` = `{ "head", "tree_digest", "contracts_digest", "files": [{"path", "sha256"}], "reports": [..] }` through `store::save_job_file`.
   9. Outcomes are `contract:<id>`, `passed` = the contract was classified red as the freeze requires (`RED_OUTCOMES`), with the run's verdict line as summary; plus `format`.
   10. `passed` = every outcome passed.
2. **Apply** (`apply_freeze_result`), common.md 4.6:
   1. A stage already frozen (`store::load_freeze` is `Some`) → `Applied::Ignored("already frozen")`; never park it.
   2. On a `Passed` job, load `contract-reports.json`. Park unless HEAD, `tree_digest(worktree)` and the current `contracts_digest` all still equal the report's. Otherwise call `stage_service::contracts::freeze_with_reports(work_dir, stage, &record.session_id, &reports, &files)`. That is today's `handle_freeze_contracts` body, renamed, plus one rule: `prepare` compares the sha256 of every file `read_files` returns with `files` (same path set, same hashes) and refuses with `Error` on any difference, so the freeze record holds exactly the bytes the run verified. `prepare` still revalidates that the session is the stage's contract writer.
      - `ContractsFrozen { files }` → `Applied::FreezeAccepted { files }`.
      - `Error { message }` → park and `Applied::FreezeRefused(message)`.
   3. On a `Failed` or `Interrupted` job, park with a message listing each failing outcome's label and summary, or the detail (at most 4 KiB), and return `FreezeRefused`.
   4. On a `Cancelled` job, return `Applied::Ignored("cancelled: <detail>")` and park nothing: the canceller (a superseding freeze, takedown, reset, retry, worktree removal) owns the stage's next state.
   5. Parking is `verify::contracts::refusal::park_refused_relay(work_dir, stage, session, message)`.
3. **Entry points.** The new `handle_freeze_contracts(work_dir, stage_id, session_id, head, tree_digest, request_id)` in `stage_service/contracts.rs`, re-exported through `daemon/server/contracts.rs` as today:
   - runs the admission checks `prepare` runs before it reads reports (stage in its contract phase, caller its contract writer; read `prepare` for the exact checks);
   - submits a `Freeze` job;
   - returns `Response::Ok` (queued or joined) or `Response::Error`.

   Route the spool (`fs/stage_request/apply.rs`) and socket (`daemon/server/stage_control.rs`) `FreezeContracts` paths to it with `new_job_id()` as request id. `StageRequest::FreezeContracts` becomes `{ head: String, tree_digest: String }`, and `StageRequest` rejects unknown keys (common.md 4.1: container-level `deny_unknown_fields`; read `stage_service/types.rs` and `relay/payload.rs::decode_stage_request`, and confirm no producer sends an undeclared field). `fs/stage_request/tests/replay.rs` (about line 72) builds the new shape.
4. **Freeze CLI** (`commands/stage/contracts/freeze.rs::freeze` and `commands/stage/contracts.rs::{send_freeze, queue_freeze}`), common.md 4.5:
   1. Run `check_changes` locally and refuse with today's text on problems.
   2. Compute `head` and `tree_digest`.
   3. `Relay` (contract session): relay `freeze-contracts` with the new payload (`end_turn: true`, as today's relayed freeze). `Operator`: send `Request::FreezeContracts { head, tree_digest, .. }` over the socket, or queue `StageRequest::FreezeContracts { head, tree_digest }` on the spool when the daemon is unreachable. `Legacy`: refuse like `loom stage complete` (common.md 4.5), exit 1.
   4. Print the common.md 4.5 text, exit 0.
   No CLI path runs contracts any more.
5. **Apply glue** (`orchestrator/core/host_jobs.rs`):
   - In `tick_host_jobs`, for every record the runner returns, call `crate::host_verify::apply::apply_finished(&work_dir, &record)`.
   - At startup (where stage 1 calls `recover_after_restart`), apply every record it returns and every record `apply::unapplied_terminal_records` returns. A Freeze job interrupted by a restart then parks its stage instead of stranding a contract session that has already ended its turn.
   - Retry (common.md 4.4 "Durability"): keep each record whose `apply_finished` returned `Err` in a list beside the runner with its attempt count; retry the list at the start of every `tick_host_jobs`; after the tenth failure write the marker with `apply failed: <error>` through `store::save_job_file` and log at `error`.
   - Log the `Applied` value at `info`, an error at `warn`. Never propagate.
   - A `Completed` result needs nothing more: apply marks the session record and removes the signal; the monitor then kills the session and merges (`orchestrator/core/completion_handler.rs`). Confirm that by reading it, and report it.
6. **Stall suppression.** While `host_verify::store::load_current(work_dir, stage)` is non-terminal (any kind), the stage's sessions are not escalated. The `SessionHung` warning may still be raised; escalation, meaning handoff and kill through `recover_hung`, is held until the job is terminal or past `runner::job_deadline_secs(&stage)` from its `started_at`.
   - Keep the signatures of `stall_line::is_stall_escalation` and `running_completion_line`: `monitor/never_worked.rs` and `core/event_handler/recover_hung.rs` call them too.
   - Hold the escalating report in `hung_latch.rs::stage_silence_event` (the work dir reaches it through `session_silence_event`'s config), reading the record there with a path-qualified `crate::host_verify::store::load_current(` call (the stage's wiring checks for it in `hung_latch.rs`). If either other caller escalates without passing through it, add the same hold there (both files are yours).
   - Keep the read to one small file per stage per check.
   - Test the held case and the released case (job terminal, or past its deadline).
7. **End-to-end test** (`tests/host_verification_e2e.rs`). Mirror `tests/integration/relay_commit_e2e.rs` (fixture, `session_loom`, `run_hook`, `drain_once` through an in-process `Orchestrator`), including a `config.toml` and plan file in the state root. Read `tests/integration/binary_spawn_guard.rs` first, and spawn the binary only in a way it permits. Open each test with the confinement guard of common.md section 5.
   1. A committed stage worktree with acceptance `["true"]`.
   2. Run `loom stage complete stage-a` in the session environment: exit 0, first line `Host job: requested (complete, `, a ticket.
   3. Run the relay hook on that Bash call's payload (exit 0, the relay line last).
   4. Build one in-process `Orchestrator` as `drain_once` does and keep it for the whole test. Install a runner with `orchestrator.set_host_job_runner(HostJobRunner::new(work_dir, RunnerConfig { program: <the built loom binary>, args_prefix: ["stage", "verify-host"], slots: 1 }))` (`env!("CARGO_BIN_EXE_loom")` as a value, never inside `Command::new(..)`, which the guard forbids; if the guard still objects, use its sanctioned helper for the path). Call `orchestrator.drain_session_inboxes()`: the job is queued. Print `e2e: queued <job id>`.
   5. Call `orchestrator.tick_host_jobs()` (the production tick, which starts the child, reaps it and applies the result) every 200 ms until the job directory holds the `applied` marker, at most 120 s; print `e2e: ran <state>` from the record and `e2e: applied` with the marker present. Never call `HostJobRunner::tick` or `apply_finished` directly here: the test proves the tick wires both.
   6. Assert the stage is `Completed`, and that `loom stage complete stage-a` in the session now prints `Host job: completed`.
   7. A second test does the same with acceptance `["false"]` and asserts `Executing`, a `ToolFailed` checkpoint, and `Host job: failed` with exit 1.

## Tests you write

- `tests_freeze.rs` (behind the confinement guard):
  - all contracts red → `Passed` and `FreezeAccepted`;
  - one contract green → `Failed` and the stage parked with its id in the reason;
  - a changed tree before the run → error;
  - a contract file edited while the contracts run (the contract command itself appends to the harness file) → error `worktree changed during the run`;
  - contract files changed after a passing run → parked at apply;
  - a commit on the stage branch after a passing run (HEAD moved, tree clean) → parked at apply;
  - a contract's command amended in the stage file after a passing run, its id kept → parked at apply;
  - `freeze_with_reports` given a `files` list whose hash differs from the file on disk → `Error`, nothing frozen;
  - a superseded (`Cancelled`) freeze job → `Ignored`, stage not parked; a stage already frozen → `Ignored`, not parked;
  - a contract that never ends is stopped by the job deadline (use the runner's `with_deadline_override`);
  - a request from a session that is not the stage's writer → `Response::Error`.
- The stall tests above.
- The retry list: an `apply_finished` that fails twice and then succeeds is applied on the third tick; one that fails ten times gets the `apply failed:` marker (inject the failure the way the W4 test does, an unwritable checkpoint directory).
- `contracts_tests.rs` and `freeze_tests.rs`: adjust to the new signatures.

## Proof (run once each, from `loom/`, after W4 and W6 have written; if the build fails only on their missing items, report that and stop)

```bash
cargo test --lib host_verify::tests_freeze 2>&1 | tail -10
cargo test --lib stage_service:: 2>&1 | tail -10
cargo test --lib orchestrator::monitor:: 2>&1 | tail -10
cargo test --lib daemon::server::contracts 2>&1 | tail -10
cargo test --test host_verification_e2e -- --nocapture 2>&1 | tail -20
```

## Report

Files changed; where the contract-running code now lives; the stall seam (`file:line`); how the e2e test spawns the binary under the guard; the proof output.
