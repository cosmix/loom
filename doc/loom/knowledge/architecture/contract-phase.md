# Contract Phase

> Contract session, freeze, handover

## Contract Phase: A Second Writer Session Before the Implementer

A v2 `standard` stage with `contracts` and no `freeze.json` starts a `SessionType::Contract` session
first (`Session::new_contract`, tracking key `loom-contract-<stage_id>`, `LOOM_SESSION_TYPE=contract`).
The stage stays `Executing` across both sessions: no new `StageStatus`, no new edge. The spawn tail of
`start_stage` lives in `orchestrator/core/stage_spawn.rs` so the contract-exit handler reuses it.
`spawn_contract_session` takes the `Session` by value, like `spawn_session`.

**The contract writer** gets a contract signal (`orchestrator/signals/contract.rs`): the contracts table,
`harness`, the knowledge brief and the language skills. It writes only contract and harness files, every
contract test must fail now, and it ends with `loom stage contracts freeze <stage-id>`. It must not commit
or complete the stage; `commit-guard.sh` gives it a contract-specific reminder.

**Freeze** (`commands/stage/contracts/freeze.rs`, runs in the agent's sandbox):

1. changed paths versus the stage base must each be a contract `file` or match a `harness` glob. It reuses
   `verify::contracts::changes::{stage_base, changed_paths}` (merge base with the configured target,
   `status -uall`), so the CLI and the daemon re-check agree by construction. `git status --porcelain`
   names an untracked directory as `dir/`, so a path-by-path check must expand it;
2. each contract file exists;
3. each contract runs through its adapter's `single_test_command` (or, with no adapter, its own `test`
   string via `verify::contracts::contract_command`) at the stage's resolved confinement. `Failed` and
   `BuildFailed` pass the freeze; `Passed` and `NotSelected` are errors; `Unparsed` needs a non-zero exit;
4. `Request::FreezeContracts` goes over the dispute-criteria transport (socket, relay, spool).

**Daemon handler** (`daemon/server/contracts.rs`) requires the caller to be the stage's current Contract
session, re-checks step 1 and file existence, hashes every contract and harness-matched file, copies them to
`.loom/work/contracts/<stage>/files/`, and writes `freeze.json`. It trusts the CLI's red-run outcomes and does
not re-run agent-written tests: that would execute untrusted code outside the sandbox, and completion
re-runs every frozen contract. Every refusal returns `Response::Error`; only a failed freeze write returns
`Err`, because `fs/stage_request/apply.rs` retries an `Err` each tick without truncating the spool and a
permanent refusal would wedge it. The freeze copies and hashes EVERY existing file a harness glob matches,
tracked or not, so a glob over a production file freezes it for the implementer and `src/**/*.rs` freezes the
crate (cap 500 files, 64 MiB). Harness globs belong on test-only files.

**Handover.** `Detection::detect_session_changes` (`orchestrator/monitor/detection.rs`, `contract_phase_event`)
raises `ContractPhaseFinished` when an `Executing` stage's Contract session has a `freeze.json`, and
`ContractSessionEnded` when the writer died without one. Both carry `session_id` and are level-triggered
re-raises while the stage still names that session, so a failed handler is retried and a stale event cannot
act on a successor. The handler (`core/event_handler/contract_phase.rs`) takes the writer down, then spawns
the `Stage` session; a survivor defers to the next tick. Respawn budget: `MAX_CONTRACT_RESPAWNS = 3`
(`core/contract_budget.rs`), checked then spent right before each respawn, the first writer free; the fourth
end escalates to `NeedsHumanReview`. Orphan recovery charges the budget only for an `Executing` stage whose
dead current session is Contract and unfrozen; `NeedsHandoff` and `Blocked` requeue uncharged. A respawn
failure keeps the worktree and branch, because the handover worktree holds the writer's uncommitted files.
Manual mode never polls monitor events, so `run_tick` runs `hand_off_frozen_contract_phases` there; a
Contract session with no PID identity is released (record marked, signal dropped, operator told to exit it),
not taken down.

**Completion** (`verify/contracts/completion.rs`) requires `freeze.json`, every frozen file's sha256 unchanged,
and each contract `Passed` through the criteria runner (certified cache reused); `NotSelected` fails. A
contract without an adapter passes on exit 0 with a warning. `loom stage contracts show` prints the record;
`loom stage contracts restore <stage> [--contract <id>]` copies frozen content back. `stage.resolved_base` is
not what freeze diffs against.

**Session-kind filters.** A second worker kind breaks every filter keyed on `SessionType::Stage`. Six sites
missed it: `orchestrator/coherence.rs` (`worker_session_type`, now `is_worker_session_type` and
`live_worker_sessions`), `core/session_adoption.rs`, `native/wrapper.rs:208` (`LOOM_WORKTREE_PATH` export, else
`relay/emit/cwd.rs:36` and `loom-control-complete.sh` treat the agent as a main-repo agent),
`status/render/graph.rs`, `self_service.rs` (`BlockStage`), and `relay/matrix.rs` (Contract sends everything
Stage does except `Dispute`, `Verdict` and completion, plus `FreezeContracts`, which only Contract may send).
A relayed freeze is dropped unless `loom-hooks/loom-relay.sh` `relay_kind_at` maps `stage:contracts` to
`FreezeContracts` only when the token after `contracts` is `freeze`, and drops it for subagents
(`is_control()`, `drop_control_kinds`).
