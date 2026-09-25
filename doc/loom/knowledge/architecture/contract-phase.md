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

## Device Nodes Are Not Changes, and the Daemon Refuses Planted Ones

Claude Code's Bash sandbox bind-mounts `/dev/null` over eleven worktree-root dotfiles, visible
only inside its own mount namespace (see mistakes/parallel-worktree-shared-state.md, "Claude
Code's Bash Sandbox..."). `git status` inside that namespace lists each mount point untracked,
so every in-session freeze check must not treat it as the writer's own change.
`changes::changed_paths` (`verify/contracts/changes.rs`) and
`fingerprint::compute` (`verify/review/fingerprint.rs`) drop an untracked path when
`git::branch::is_device_node` (`git/branch/status.rs`) confirms it via `symlink_metadata` — char
or block device only, so a FIFO or socket a session could actually create is kept. Before this
filter, every freeze a Claude Code contract writer attempted was refused on these placeholders,
so a v2 stage with contracts could never leave the contract phase.

The daemon's own check (`daemon/server/contracts.rs::check_changes`, calling
`changes::special_files`, walk in `verify/contracts/special_walk.rs`) walks the worktree — skip
`.git`, skip git-ignored — and refuses the freeze outright if it finds a FIFO, socket, or device
node anywhere: git shows none of those, and leaving one where the implementer will write blocks
its first `open()`. The writer can still act during the walk, so every directory is opened from
the worktree root fd through `fs::safe_read::list_dir_no_follow` (`openat2` with
`RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH` on Linux, per-component `O_NOFOLLOW` elsewhere). A
directory swapped for a symlink mid-walk fails the open, and the freeze is refused; the walk
never enumerates outside the worktree. Non-UTF-8 names reach the refusal `\xNN`-escaped. `read_bounded`
(`fs/safe_read.rs`) opens `O_NONBLOCK`, so a FIFO planted as a contract file cannot hang the
daemon's own read of it.

## A Refused Freeze Parks the Stage

`verify::contracts::refusal` (new module) tracks a contract writer's latest freeze attempt. In
relay mode — the writer's sandbox cannot reach the daemon socket directly — a refusal is left at
`$LOOM_SCRATCH_DIR/contract-freeze-refused.txt` and cleared on the next attempt that reaches the
daemon, so the file always reflects the latest attempt. `loom-hooks/commit-guard.sh`'s Stop hook
sees the file when the writer's session stops and runs `loom stage waiting <id>`
(`park_contract_stage`). A relayed freeze the daemon itself refuses parks the same way straight
from the inbox drain (`orchestrator/core/inbox_drain/apply.rs::park_refused_freeze` →
`refusal::park_refused_relay`), without waiting for the writer to stop — the relay already told
it to end its turn. Either path moves the stage `Executing` → `WaitingForInput` with
`review_reason = "contract freeze refused; fix and freeze again: <problems>"`, the reason `loom
status` shows as NEEDS INPUT.

Resume paths: the writer runs a tool (the monitor moves a waiting stage back to `Executing` on
its session's next tool call), `loom stage resume <id>`, or a later freeze the daemon accepts
(`daemon/server/contracts.rs::end_refusal_wait`). `loom stage retry` still refuses a stage in
`WaitingForInput`. A writer that stops before ever attempting a freeze leaves no refusal file, so
nothing is parked.

## Visible Everywhere `loom status` Renders

Contract phase = `StageStatus::Executing` + `SessionType::Contract` on a `StageType::Standard`
stage (`render/attention_model.rs::is_contract_phase`). Static/compact: a magenta `contracts` tag
right after `[model]` (`render/graph.rs::stage_tags`), added to the legend while such a stage
exists. Live TUI: the activity cell reads `writing contract tests`, or once stale, `contract
writer idle <duration>` (`ui/tui/ledger/rows.rs`). Web dashboard: `web/src/api/schema.ts` accepts
`session_type: "contract"` (before this it did not, and every frame carrying a contract-writer
stage was rejected — see mistakes/web-dashboard-server.md); the stage node shows a violet
`contracts` tag and a dashed outline (`data-phase="contract"`, `graph.css`), the stage-strip label
reads `... · contract phase`, and the stage dialog's session-type row reads `contract writer`
(`web/src/lib/format.ts::isContractPhase`/`sessionTypeLabel`).
