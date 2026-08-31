# Stage-to-Daemon Channels

> How a stage agent reaches the daemon to change its own stage's state, and why there are three
> routes rather than one.

## Three Routes, Chosen by What the Caller Can Actually Do

A stage agent that needs to record something about its OWN stage — block it, dispute a criterion —
is boxed in by two deliberate denials. It cannot write `.work/stages/<id>.md`, because stage state
belongs to the daemon and `.work/` is read-only from a worktree. And it cannot read
`.work/user.token`, because that one credential authorizes every User RPC, not just the ones a
stage agent is entitled to (S-1).

Both denials are correct. Together they once left the agent with no way to say anything at all: a
stage finished, correct and committed, sat waiting on a human because two of its acceptance
criteria were defective and it could report neither fact.

`daemon::rpc::try_send_request` returns a three-way `DaemonReach`, and each arm picks a route:

| `DaemonReach` | What happened | Route taken |
| --- | --- | --- |
| `Answered(resp)` | a daemon replied | its answer stands, refusal included |
| `NotListening` | no socket file, or a stale one nothing is bound to | the caller's local fallback (an operator's direct write) |
| `Unreachable` | `PermissionDenied` on connect | the worktree spool |

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
WORKTREE the daemon drained the entry from — an agent cannot forge the worktree it is running in,
but it could trivially forge a field. That is what replaces peer identity when there is no
connection to identify, and it is the whole security argument for the route.

Two rules the drain depends on:

- **Apply through the daemon's own handlers**, not a second implementation. A spooled block calls
  the same `handle_block_stage` the RPC calls, so the id allocation, the `validate_id` guard, the
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
