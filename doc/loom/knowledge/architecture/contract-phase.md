# Contract Phase

> Contract session, freeze

## Contract Phase: A Second Writer Session Before the Implementer

A v2 `standard` stage with `contracts` and no `freeze.json` starts a `SessionType::Contract` session
first (`Session::new_contract`, tracking key `loom-contract-<stage_id>`, `LOOM_SESSION_TYPE=contract`).
The stage stays `Executing` across both sessions: no new `StageStatus`, no new edge. The spawn tail of
`start_stage` lives in `orchestrator/core/stage_spawn.rs` so the contract-exit handler reuses it.
`spawn_contract_session` takes the `Session` by value, like `spawn_session`.

**The contract writer** gets a contract signal (`orchestrator/signals/contract.rs`): the contracts table,
`harness`, the scaffold edits it may make (`scaffold_line`, the one description shared with the
freeze-refusal briefing), the knowledge brief and the language skills. It writes contract files, harness
files and the scaffold, every contract test must fail now, and it ends with
`loom stage contracts freeze <stage-id>`. It must not commit or complete the stage; `commit-guard.sh` gives it
a contract-specific reminder and calls no loom command for a contract session.

**Freeze** has two halves. The CLI pre-check (`commands/stage/contracts/freeze.rs::check_changes`, runs in the
agent's sandbox):

1. every changed path versus the stage base (`verify::contracts::changes::{stage_base, changed_paths}`: merge
   base with the configured target, `status -uall`, so the CLI and the daemon agree by construction) is a
   contract `file`, matches a `harness` glob, or is a scaffold edit, and the scaffold and the module chains are
   sound (`collection::freeze_scaffold`: the strict scaffold diff, then, once the diff is clean, the chain
   check of every Rust contract and the run directory of every one that sets `cwd`; see
   [contract-collection](contract-collection.md)). `git status --porcelain` names an untracked directory as
   `dir/`, so a path-by-path check must expand it. A worktree path that is not UTF-8 or holds a control
   character is refused at the listing (`changes::listed_name`), never escaped: an escaped name read back as a
   path can name a different file, so only refusal fails closed;
2. each contract file exists.

`Request::FreezeContracts` then goes over the dispute-criteria transport (socket, relay, spool). The daemon
(`stage_service::handle_freeze_contracts`) admits only the stage's running Contract session and queues a
`Freeze` host job for the HEAD and tree digest the writer saw. The job (`host_verify/freeze.rs`) re-checks
the changes with git pinned to the stage's registered git directory, runs every contract through its
adapter's `single_test_command` (or, with no adapter, its own `test` string via
`verify::contracts::contract_command`) at the stage's resolved confinement, runs the format gate, and checks
its binding (HEAD, tree digest, file hashes, contract definitions) again. `Failed` and `BuildFailed` pass the
freeze; `Passed` and `NotSelected` are errors; `Unparsed` needs a non-zero exit. Applying the job
(`freeze::apply_freeze_result`, `stage_service::freeze_with_reports`) re-checks the changes, hashes every
contract and harness-matched file, requires them to equal what the run hashed, copies them to
`.loom/work/contracts/<stage>/files/`, and writes `freeze.json`. The freeze copies and hashes EVERY existing
file a harness glob matches, tracked or not, so a glob over a production file freezes it for the implementer
and `src/**/*.rs` freezes the crate (cap 500 files, 64 MiB). Harness globs belong on test-only files. A
refusal at either half goes to adjudication ([Refused Freezes Are Disputed](#refused-freezes-are-disputed)).

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
the chain check of every Rust contract against the implementer's tree (`collection_problems`: `chain_problems`
plus `run_dir_problems` for the directory the contract runs from, so a scaffold the implementer rewrote after
the freeze cannot compile the frozen file out), and each contract `Passed` through the criteria runner
(certified cache reused); `NotSelected` fails. Whether a contract is Rust follows the adapter recorded at
freeze (`is_rust`); `run_contract` still re-detects the adapter on the implementer's worktree to resolve the
run and judge it. A contract without an adapter passes on exit 0 with a warning. `loom stage contracts show`
prints the record; `loom stage contracts restore <stage> [--contract <id>]` copies frozen content back.
`stage.resolved_base` is not what freeze diffs against.

**Session-kind filters.** A second worker kind breaks every filter keyed on `SessionType::Stage`. Six sites
missed it: `orchestrator/coherence.rs` (`worker_session_type`, now `is_worker_session_type` and
`live_worker_sessions`), `core/session_adoption.rs`, `native/wrapper.rs:208` (`LOOM_WORKTREE_PATH` export, else
`relay/emit/cwd.rs:36` and `loom-control-complete.sh` treat the agent as a main-repo agent),
`status/render/graph.rs`, `self_service.rs` (`BlockStage`), and `relay/matrix.rs` (Contract sends everything
Stage does except `Dispute`, `Verdict` and completion, plus `FreezeContracts`, which only Contract may send).
A relayed freeze is dropped unless `loom-hooks/loom-relay.sh` `relay_kind_at` maps `stage:contracts` to
`FreezeContracts` only when the token after `contracts` is `freeze`, and drops it for subagents
(`is_control()`, `drop_control_kinds`).

## Sandbox Placeholders Are Not Changes, and the Daemon Refuses Planted Special Files

Claude Code's Bash sandbox denies writes to eleven worktree-root names (`.bashrc`, `.gitconfig`,
`.mcp.json`, `.vscode`, ...) and bind-mounts `/dev/null` over one that does not already exist,
visible only inside its own mount namespace (see mistakes/parallel-worktree-shared-state.md,
"Claude Code's Bash Sandbox..."). `git status` inside that namespace lists the mount point
untracked; on the host the same name can be left behind as an empty regular file or directory.
Every in-session freeze check must not treat any of these as the writer's own change.
`changes::changed_paths`/`special_files` (`verify/contracts/changes.rs`) and
`fingerprint::compute_local` (`verify/review/fingerprint.rs`) drop an untracked path at one of the
eleven names when `verify::tool_artifacts::is_tool_artifact` (replaces the removed
`git::branch::is_device_node`) finds it a device node, an empty regular file, an empty directory,
or gone; a FIFO, a socket, or real content at that name is kept, and so is every path below the
root or under another name. Before this filter, every freeze a Claude Code contract writer
attempted was refused on these placeholders, so a v2 stage with contracts could never leave the
contract phase.

Git for every one of these checks runs through a `WorktreeGit` (`git/worktree/pinned.rs`). The
writer's own freeze pre-check (`commands/stage/contracts/freeze.rs`) uses the worktree's own
`.git` (`WorktreeGit::discovered`): it already runs inside the writer's own sandbox with the
writer's own privileges. The daemon's re-check (`daemon/server/contracts.rs`) and the change
fingerprint (`fingerprint::compute_local`) pin git to the worktree's git directory as registered
in the main repository (`WorktreeGit::pinned`), so a `.git` file the writer rewrote to point
elsewhere cannot choose which configuration git reads.

The daemon's own check (`daemon/server/contracts.rs::check_changes`, calling
`changes::special_files`, walk in `verify/contracts/special_walk.rs`) walks the worktree — skip
`.git`, skip git-ignored — and refuses the freeze outright if it finds a FIFO, socket, or device
node anywhere, except a device-node placeholder at one of the eleven protected root names
(`special_walk::sandbox_mount`, the same `is_tool_artifact` rule): git shows none of the rest, and
leaving one where the implementer will write blocks its first `open()`. The writer can still act
during the walk, so every directory is opened from
the worktree root fd through `fs::safe_read::list_dir_no_follow` (`openat2` with
`RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH` on Linux, per-component `O_NOFOLLOW` elsewhere). A
directory swapped for a symlink mid-walk fails the open, and the freeze is refused; the walk
never enumerates outside the worktree. Non-UTF-8 names reach the refusal `\xNN`-escaped. `read_bounded`
(`fs/safe_read.rs`) opens `O_NONBLOCK`, so a FIFO planted as a contract file cannot hang the
daemon's own read of it.

## Visible Everywhere `loom status` Renders

Contract phase = `StageStatus::Executing` + `SessionType::Contract` on a `StageType::Standard`
stage (`render/attention_model.rs::is_contract_phase`). Static/compact: a magenta `contracts` tag
right after `[model]` (`render/graph.rs::stage_tags`), added to the legend while such a stage
exists. Live TUI: the activity cell reads `writing contract tests`, or once stale, `contract
writer idle <duration>` (`ui/tui/ledger/rows.rs`). Web dashboard (`web/src/lib/format.ts::isContractPhase`):
the graph node takes the contract tone in place of its status tone
(`graph/stage-node.tsx`, `toneClass("contract")`), so its rail, breathing glow, wash and hover
shadow turn purple and return to the executing tone when the implementer session takes over. The
node carries no `contracts` tag: node tags name stage types, and a contract-writing stage is a
standard stage. The state key adds a non-interactive `writing contracts` chip while such a stage
exists (`graph/state-key.tsx`), the legend lists `writing contract tests` under activity, the
stage-strip label reads `... · contract phase`, and the stage dialog's session-type row reads
`contract writer` (`sessionTypeLabel`). `web/src/api/schema.ts` must accept
`session_type: "contract"` (see mistakes/web-dashboard-server.md).

## Where a Contract Runs

`verify/contracts/run_dir.rs::resolve_run` picks the directory every contract command runs from, and is the only builder of a contract command (freeze CLI, daemon freeze record, completion, dispute re-freeze, contract signal, adjudication prompt). Priority:

1. the plan contract's optional `cwd`, relative to the worktree root (`v2_fields.rs` requires the file to lie beneath it); it always wins;
2. the cwd `freeze.json` recorded for the contract's current file, found by `store::recorded_cwd` (completion, dispute re-freeze, adjudication prompt), so completion runs where the freeze ran even if a runner config is planted since;
3. the nearest ancestor of the contract file holding one of the adapter's `run_root_markers()` (regular files only, symlinks ignored), walking up to the worktree root;
4. the stage's `working_dir`.

The file is passed to the runner relative to that directory. The daemon records each contract's normalized `file` (relative to `working_dir`) and resolved `cwd` in `FrozenContract`. `store::recorded_cwd` trusts an entry only for the file it was recorded with: the entry with the contract's id and file, else any entry with its file (a renamed id keeps its cwd, so markers the agent plants cannot move it), else an entry with its id and no `file`. A contract re-pointed to another file gets no recorded cwd and falls to rules 3-4; so does an entry without `cwd`.

An accepted contract dispute re-freezes the whole frozen set, including the disputed contract; its `cwd` values still come from `store::recorded_cwd` as above. See [Accepted Dispute Re-Freeze](#accepted-dispute-re-freeze). See mistakes/verification-v2-delivery.md, "Contracts in a Sub-Workspace Ran Under the Root's Runner Config".

## Accepted Dispute Re-Freeze

An accepted `dispute-contract` (`orchestrator/adjudication/apply_contract.rs`) re-freezes the stage's whole frozen set, including the disputed contract:

- the files read by `verify/contracts/frozen_set.rs::read_frozen_set` are the amended contract files, the harness files re-expanded from `stage.harness` (`verify/contracts/changes.rs::harness_files`), every path the record lists, and the disputed contract's file before the amendment;
- `store::refreeze_set` (`verify/contracts/store.rs`) replaces the record's `files` with exactly that set, upserts each contract the accept names with its adapter, file and `cwd` (`store::recorded_cwd`), and adds a contract the record lacks with outcome `REFROZEN_OUTCOME` (`refrozen`), because it never ran at freeze;
- `freeze.json` is rewritten before the dropped copies are removed, so the record never names a copy that is not on disk. A failed removal leaves an orphan copy that no record names; restore and completion read only the copies the record names, so the orphan is harmless;
- the stage must already be frozen; `refreeze_set` is called by the accept only.

The adjudicator's accept notice lists the re-frozen paths; see the cap suggestion in [Gate-Dedup and Integration-Verify Review Suggestions](../concerns/verification-v2-followups.md#gate-dedup-and-integration-verify-review-suggestions).

## Refused Freezes Are Disputed

A refused freeze never waits for the operator. loom files it as a `freeze-refusal` dispute
(`DisputeKind::FreezeRefusal`), an adjudicator decides it, and a fresh contract writer starts with the verdict
(see [mistakes/contract-phase-human-blocking.md](../mistakes/contract-phase-human-blocking.md)).

**Where a refusal is kept.** `verify::contracts::refusal` holds both sources.

- The CLI pre-check in the writer's sandbox cannot write the state directory, so it leaves its problems at
  `$LOOM_SCRATCH_DIR/contract-freeze-refused.txt` (`REFUSAL_FILE`, mirrored by `commit-guard.sh`) and clears the
  file when an attempt reaches the daemon, so it always stands for the latest attempt. The file is the agent's
  text: read bounded, never through a symlink, filed with the `writer-reported:` prefix (`WRITER_PREFIX`).
- A refusal the daemon makes (a relayed admission in `orchestrator/core/inbox_drain/freeze.rs`, a host Freeze
  job in `host_verify/freeze.rs`) is written to `.loom/work/contracts/<stage>/refused.json`
  `{session_id, problems, at, source, declined}`, a file only the daemon writes. Interrupted or daemon-stopped
  host runs and unreadable reports or bindings carry a first problem `infrastructure:`
  (`INFRASTRUCTURE_PREFIX`); host error, job detail and log text is one `run-output:` problem
  (`run_output_line`), escaped to a single line and rendered `[run output, unverified]` in a fence so a writer
  cannot forge `[loom]` labels. The inbox drain never retries (delivery is at most once): a failed
  `refused.json` write settles the request refused, the writer sees a failure and freezes again, and hung
  detection is the backstop. A Freeze job that exhausts `MAX_APPLY_ATTEMPTS` records no refusal at all (see
  [concerns/source-graph-review-backlog.md](../concerns/source-graph-review-backlog.md#give-up-leaves-freeze-refusals-unfiled)).

**The one filer.** `Orchestrator::dispute_refused_freezes` (`orchestrator/core/event_handler/freeze_refusal.rs`)
runs in `run_tick` before `hand_off_frozen_contract_phases`, manual mode included, for each `Executing` v2
standard stage with contracts, nothing frozen and a current Contract session. It files a daemon record at once
when it names the current writer (stale otherwise: dropped), and the writer's own file once the file and the
session heartbeat are `REFUSAL_QUIET_SECS = 300` old and the session runs no subagent, since the writer may
still fix and retry in the same turn. Filing goes through `stage_service::file_freeze_refusal`
(`dispute_kinds.rs`); the transport entries refuse a freeze refusal outright, `DisputeKind::filed_by` admits
only a Contract session, and a stage with no current session admits every kind but this one. The receipt key is
`freeze-refusal-<first 32 hex of sha256(request key)>` because `validate_id` rejects `:`, `+` and `.`, and a
retried filing reuses one request file. A filed refusal's record is removed; a record the admission declined is
marked (`declined`) and never filed again. A fresh writer has a new session id, so neither source re-files for
it.

**Budget.** `tally.freeze_refusal_disputes` against `MAX_DISPUTES_PER_KIND = 3`. An exhausted budget makes
`file_dispute_inner` decline (`FreezeRefusalFiling::Declined`) instead of escalating, because
`escalate_exhausted` would write `NeedsHumanReview` while the writer still lives; the tick step retires the
writer, then calls `escalate_freeze_refusal` (the 4th refusal), and a failed escalation after retirement is
logged with the stage id. Approving the escalated stage in human review resets the respawn budget (`commands/stage/human_review.rs::handle_approve`) but not this tally.

**The verdict** is checked when recorded (`orchestrator/adjudication/verdict_freeze_refusal.rs`,
`RecordContext {work_dir, stage}`; the worktree is located only for an accept). Accept needs
`plan_patch.patches` on `contracts`, `harness` or `acceptance`, replayed in array order over a copy of the
stage's fields so each index reads as the apply reads it; an `acceptance` entry may only replace a format-check
criterion with another that sets no output expectation; a harness glob may match no production source and no
path the derived scaffold names; the entries replayed over the plan's own stage definition must leave a plan
`schema::validate` accepts after EACH entry (`plan::apply_amendment` validates per patch and never lints),
and the final plan adds no blocking `plan verify` lint compared with the unpatched one
(`verdict_freeze_plan.rs`). Reject needs `instructions` for the next writer. Needs-more-evidence needs a
question; the writer answers in `loom memory note` and freezes again, and a refusal of that freeze is a new
filing, so the budget bounds the exchange. The briefing (`prompt/freeze_refusal.rs`,
`freeze_refusal_evidence.rs`) re-derives the change-set check from `changes::special_files`,
`collection::freeze_scaffold` and `changes::changed_paths` so the judge sees every changed path classified.

**Apply** (`orchestrator/adjudication/apply_freeze_refusal.rs`): the writer is retired first. Each accepted
patch is one amendment applied in order with dispute id `<dispute>.<k>`, so a retried apply skips what an
earlier attempt applied; at the per-stage amendment cap (default 10, `plan/amendment.rs`) `amend_plan`
escalates and nothing more applies. A `harness` patch value is a bare glob, never YAML (`**/x.json` reads as an
alias, `a #1` loses its tail); every other field takes a YAML body. Every verdict re-queues the stage, held
while a sibling dispute is unanswered, for a fresh writer on the same worktree: no freeze exists, so the
contract phase starts again and the signal carries the notice (`disputes/<stage>/feedback.md`). No freeze
record is read or written.
