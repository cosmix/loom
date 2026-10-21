# Stage-to-Daemon Channels

> Stage agent to daemon
> routes rather than one.

## Three Routes, Chosen by What the Caller Can Actually Do

A stage agent that needs to record something about its OWN stage — block it, dispute a criterion —
is boxed in by two deliberate denials. It cannot write `.loom/work/stages/<id>.md`, because stage state
belongs to the daemon and `.loom/work/` is read-only from a worktree. And it cannot read
`.loom/work/user.token`, because that one credential authorizes every User RPC, not just the ones a
stage agent is entitled to (S-1).

`daemon::rpc::try_send_request` returns a three-way `DaemonReach`, and each arm picks a route:

| `DaemonReach` | What happened | Route taken |
| --- | --- | --- |
| `Answered(resp)` | a daemon replied | its answer stands, refusal included |
| `NotListening` | no socket file, or a stale one nothing is bound to | the caller's local fallback (an operator's direct write) |
| `Unreachable` | `PermissionDenied` on connect, a socket path `lstat` cannot look at, or a resolved socket path past `sun_path` | the worktree spool |

**Every client dials `daemon::socket_path(work_dir)`** (`daemon/socket.rs`), never `work_dir.join("orchestrator.sock")`.
In a stage worktree `.loom/work` is a symlink to the state root, and the daemon bound its socket under the
resolved, shorter path; the worktree spelling can pass the `sun_path` limit and fail `connect` with
`InvalidInput` before any syscall. `socket_path` canonicalizes the work dir (falling back to the
spelling it was given), `socket_path_fits` budgets `SUN_PATH_MAX = 104` bytes (the tighter macOS/BSD
bound, on every platform), and `try_send_request` classifies a path past the limit as `Unreachable`
(the daemon may run; this process cannot address it) while any other `InvalidInput` stays an error.
`loom run` refuses a work root whose resolved socket path does not fit, before the plan is marked in
progress (`--foreground` binds no socket and is not refused), and `loom init` warns. The completion
broker sends through `daemon::send_request` over the same path; completion is never spooled.
The TUI, web, repair, review-observer, shutdown and status clients resolve through the same function.

**A refusal is an answer; a stale socket is not.** Never fall back from `Answered` — routing around
a live daemon's refusal hands a sandboxed agent exactly the write the sandbox denies it. Do fall
back from `NotListening`: there is no authority to defer to.

## Why the Socket Alone Was Not Enough

The RPC route is authorized by connection identity rather than a secret: `SO_PEERCRED` gives the
peer's pid, the session file plus pid-file identity proves the named session is alive, and an
ancestry walk proves the caller is that process or below it (`daemon/server/peer_identity.rs`).
`daemon/server/self_service.rs` lists the three requests that route may carry — `CompleteStage`,
`DisputeCriteria`, `BlockStage` — behind an explicit match whose `_ => None` arm means a User RPC
added later is refused by default rather than silently inheriting it. Ownership is checked
separately and in both directions, so being inside session A says nothing about whether stage X is
A's to act on.

That is sound, and from a sandboxed stage it is also **unreachable**. `sandbox/settings/policy.rs`
rejects `excluded_commands` outright — naming the loom CLI in the bail message — so `loom` can never
be configured to run outside the host sandbox, and a sandboxed caller has AF_UNIX denied at
`socket()` creation, before the path is even considered. The verified consequence: `connect` returns
`PermissionDenied` whether or not a daemon is running.

**Check the socket FILE before calling `connect`.** Inside a sandbox the syscall fails identically
whether the daemon is absent or merely unreachable, so a pre-check on `exists()` is what keeps
"no daemon configured" distinguishable from "daemon I cannot reach". Note the direction: using
`exists()` as a pre-check is sound, while using it AFTER a failed connect to conclude the daemon is
absent is not (`daemon/server/core.rs` explains why — a sandbox denying `connect` may deny `stat`
too).

## The Spool, and Where Attribution Comes From

`fs/stage_request/` mirrors `fs/memory/spool.rs`: the agent appends to
`<worktree_root>/.loom/stage-request-spool.jsonl`, inside the worktree's own write boundary so no
new sandbox grant is needed, and the daemon drains it from `orchestrator/core/spool_drain.rs` on its
poll loop.

**The payload carries no stage id and no session id, on purpose.** Attribution comes from WHICH
WORKTREE the daemon drained the entry from: a worktree agent cannot write outside its own worktree,
and a field it wrote would be forgeable. That is what replaces peer identity when there is no
connection to identify.

**The guarantee holds only against worktree agents.** Main-checkout sessions
(Knowledge, Merge, Adjudication) run with the whole checkout writable, every
`.worktrees/<id>/.loom/*-spool.jsonl` included, so one of them can append a request the daemon
attributes to another stage. Closing that is part of the pending `.loom` confinement work; see
[Live State Pollution](../mistakes/live-state-pollution.md).

Two rules the drain depends on:

- **Apply through the daemon's own handlers**, not a second implementation. A spooled block calls
  the same `handle_agent_block_stage` the RPC calls for a request naming a session (refused while
  the session's latest trusted completion checkpoint shows a failing criterion), so the id allocation, the `validate_id` guard, the
  dispute budget and every refusal message cannot drift between the two ways a request arrives.
- **A refusal is counted, never returned as `Err`.** An `Err` from the sink stops the spool being
  truncated, so one request the stage can never take — a block of an already-completed stage —
  would redeliver forever and wedge every request behind it. Reserve `Err` for genuine I/O failure,
  where retrying next tick is right.

## Wiring a New Spool Path

A new file under a worktree's `.loom/` must be added to `git/worktree/settings.rs` in both places
that already name the memory spool: `is_worktree_scaffold_path`, so `git status` does not read it as
*agent work*, and `WORKTREE_EXCLUDE_PATTERNS`, so it reaches `info/exclude` and cannot be committed.
Deliberately not a blanket `.loom/` — a project may legitimately track `.loom/config.toml`.

## Relay Kinds and Their Hook Mapping

A session Bash call that relays a request prints one `LOOM_RELAY_V1` line as its last stdout line.
The PostToolUse hook `loom-hooks/loom-relay.sh` derives the kinds the command may write from its
argv (`relay_kind_at`), and `loom hook relay` admits the line only when the ticket's kind equals the
line's kind (`commands/hook/relay/ticket_check.rs`). A command whose kind the hook does not derive
is never relayed. The nine control kinds (`RequestKind::is_control`, `loom/src/relay/kind.rs`) are
dropped from a subagent by `drop_control_kinds` before an inbox entry exists, so a new control kind
needs an entry there.

| Session command | Kind | Control |
| --- | --- | --- |
| `loom memory note`, `decision`, `change`, `question`, `resolve` | `memory` | no |
| `loom stage block` | `block` | yes |
| `loom stage commit` | `commit` | yes |
| `loom stage complete` | `complete` | yes |
| `loom stage dispute-criteria` | `dispute`; `file-dispute` with `--field impact-tests` | yes |
| `loom stage dispute-findings`, `dispute-contract`, `dispute-integrity` | `file-dispute` | yes |
| `loom stage merge --resolved` | `merge-resolved` | yes |
| `loom stage adjudicate` | `verdict` | yes |
| `loom stage contracts freeze` | `freeze-contracts` | yes |
| `loom knowledge context` | `telemetry` | no |
| `loom check` | `check` | no |
| `loom handoff` | `handoff` | yes |

`check` is not a control kind, so a subagent may relay it. `loom check` and `loom stage complete`
in a session are host jobs (below): the daemon runs the gate on the host, and only a passing
`complete` job lets the daemon complete the stage.

**The completion pin is a PreToolUse hook, not a bridge.** `loom-hooks/loom-control-complete.sh`
refuses every spelling of the completion command except the exact pinned
`<loom-bin> stage complete <stage-id>` (exit 2, `LOOM_CONTROL_ERROR`) and writes the completion
heartbeat the stall monitor reads. It carries no verdict. A Bash call whose text names the
completion subcommand is refused unless it is that exact command, so a heredoc that edits relay or
hook scripts is refused too: make such edits with the Edit tool. Run the pinned command bare, with
no redirect or pipe, since the verdict comes from the daemon's host job rather than from output.

## Every Relay Command Exits 0

Claude Code runs PostToolUse hooks only after a Bash call that exits 0. A non-zero exit raises
`PostToolUseFailure` (its payload carries `error` and no `tool_response`), and loom registers no hook for that
event. A command that prints a relay line and then exits non-zero is never relayed.

Every command that emits a relay ticket therefore exits 0 and reports its outcome in its output
(`commands/stage/commit.rs::relay_commit` is the precedent). The host-job waits follow the same rule:
`host_verify/wait.rs` exits 0 while a job is requested or pending.

## Host Jobs: First Line and Exit Codes

`loom check` in a session (`commands/verify_relay.rs`) and `loom stage complete` in a session
(`commands/stage/complete_relay.rs`) request a host job and follow it across Bash calls. Every call
prints `Host job: <state> (<kind>, <short head>)` as its first stdout line, where `<kind>` is `check`
or `complete` and `<short head>` is the first 12 characters of HEAD (`host_verify/wait_report.rs::heading`).
A call buffers its output, so the first line is the one the agent reads when the call ends. The
states and exit codes:

| First line | Printed when | Exit |
| --- | --- | --- |
| `requested` | the ticket was emitted; the job is queued | 0 |
| `pending` | the job is still running; `N criteria done` | 0 |
| `passed` | `check` job passed | 0 |
| `failed` | the job failed; each failing criterion follows | 1 |
| `stopped` | the job was cancelled or interrupted | 4 |
| `refused` | the call could not request or follow a job (a legacy session, a bad state, an emit failure) | 1 |
| `completed` | `complete` only: the daemon applied a passing run and completed the stage | 0 |
| `rejected` | `complete` only: the daemon rejected the application | 1 |

A `complete` call that follows a passing run waits for the daemon's application, so `passed` never
appears for it. A session that cannot relay (no `LOOM_SCRATCH_DIR`) is refused; the operator's shell
completes locally.

**Known limit.** `wait_or_request` writes the `requested` lines before the emit closure runs. If the
emit then fails, the call prints `Host job: refused` after them, so a call can open with `requested`
and end with `refused`. The test that pins the order (`host_verify/tests_wait.rs`,
`with_no_state_the_request_lines_print_before_the_relay_runs`) must change before the order can
change. Revisit when the relay sink can fail before the first line is written.

## The Host Job State Directory

Job state lives under `<state root>/verification/<stage>/` (`host_verify/store.rs`):

- `current.json`, the stage's current job record, and `.lock`, the flock that serializes writes.
- `<job_id>/`, one directory per job, holding `record.json` (a copy of the record), `progress.jsonl`,
  `result.json`, `groups` (the criterion process groups), `child.log`, and files saved through
  `save_job_file` such as the `applied` marker.

Sessions read the directory (`sandbox/settings.rs`, `STATE_READ_DIRS`). `store_fs` opens every path
beneath the state root with no symlink followed at any component.

Writes other than `save_current` (`save_result`, `save_criterion_log`, `save_job_file`, the `append_*`
functions) require the job directory to exist: `write_atomic` does not recreate a job directory that was
pruned. `save_current` creates the job directory, so a test or a caller that writes a job file first
gets `ENOENT`. `save_job_file` refuses the five store files (`record.json`, `progress.jsonl`,
`result.json`, `groups`, `child.log`) and accepts other names, including `applied`.
