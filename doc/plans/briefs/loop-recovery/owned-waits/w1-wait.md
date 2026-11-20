# W1 — Exact owned-worker wait

Plan stage: owned waits, first worker. Lane: Codex `gpt-5.6-sol`, effort `xhigh`, Bash timeout `600000` ms.

Replace transcript-directory polling with one bounded runtime wait over an explicit, immutable set of
Claude and Codex worker invocations. This stage follows worker-evidence; use its versioned lifecycle
journal and exact start/authorization indexes. Workers NEVER spawn subagents.

Do not run git or write `.loom/`. Do not run the full test suite, formatter, linter, or type checker.
You may run one narrow check once if certain. Report changed files, assumptions, and unresolved issues.

## Owned paths

Existing:

- `loom/src/commands/subagents/mod.rs`
- `loom/src/commands/subagents/render.rs`
- `loom/src/commands/subagents/resolve.rs`

New:

- `loom/src/commands/subagents/wait/mod.rs`
- `loom/src/commands/subagents/wait/model.rs`
- `loom/src/commands/subagents/wait/identity.rs`
- `loom/src/commands/subagents/wait/lease.rs`
- `loom/src/commands/subagents/wait/engine.rs`
- `loom/src/commands/subagents/wait/tests.rs`
- `loom/tests/subagent_owned_wait.rs`

## Command and identity contract

Keep `list` and `harvest` as one-shot diagnostics. Change `watch` to require at least one repeatable
`--worker <kind>:<id>` (`claude:<agent-id>` or `codex:<unit-id>`), with optional `--session
<Claude-parent-UUID>` only as an exact disambiguator. A stage-owned watch also requires safe
`LOOM_STAGE_ID`, `LOOM_SESSION_ID`, and `LOOM_WORK_DIR`; the Loom session ID is never accepted as the
Claude parent UUID.

Expose `wait::run` from the new wait module and make the `Watch` dispatch arm in
`commands/subagents/mod.rs` call it. The artifact must be reachable through the built CLI.

At entry, resolve every requested worker through the predecessor stage's authoritative indexes using
exact `(stage_id, loom_session_id, worker kind/id)`. Require one common Claude parent UUID, current
stage-to-Loom-session ownership, canonical worktree, and one unique invocation for every item. Bind
that result once as `WaitIdentity`; never call `most_recent_session_dir` or re-resolve from mtime in
the loop. Sort and deduplicate the worker set, rejecting aliases and duplicate/conflicting IDs.

The bound Claude entry includes agent ID/type, normalized transcript path and start-record identity.
Terminal length/digest is a fresh evidence cursor read during waiting, not an unknown future hash
frozen at entry. The Codex entry includes logical unit, authorization identity, and exact
companion job ID or direct execution thread/tool-use identity. A valid bound start with no terminal event is `Active`; missing identity,
ambiguous, stale, malformed, or cross-session evidence is `Unknown`, never success. Teammate `Idle`
is nonterminal. Revalidate lifecycle records on every read: a Claude success
requires the record's transcript length plus terminal-record digest to match the current final turn.
Further transcript growth invalidates the old success until a newer correlated stop arrives. Codex
success requires the same real terminal companion/direct evidence accepted by `codex_lifecycle`.

Legacy `watch --dir` or a watch with no explicit worker set must fail with migration guidance; an
unresolvable session, empty directory, unknown worker, or no worker is never “settled.” `list` and
`harvest` may retain clearly labelled nonauthoritative transcript fallback for diagnostics.

## Lease and wait engine

Put ephemeral coordination only under
`$TMPDIR/loom-subagent-waits/<repo-fingerprint>/<stage>/<loom-session>/<parent-uuid>/`, where the plan
sets `TMPDIR=/tmp/loom-loop-checks`. Never put wait coordination in `.loom/work`. Canonicalize the
repository and scratch roots; validate every path component; refuse symlinks; create directories mode
0700 and files mode 0600 with exclusive creation and atomic replacement.

`WaitLease` stores schema version, random wait ID, canonical repo, source revision, full
`WaitIdentity`, a monotonic deadline measured from the OS boot clock plus boot identity,
owner `ProcessIdentity { pid, start_time }`, and terminal result. A changed boot identity makes the
lease Interrupted; never deserialize a process-local Rust Instant as a cross-process deadline.
Use `crate::process::{process_start_time, verify_process_identity}` so a recycled PID cannot inherit a
lease. Missing start metadata is unverifiable: preserve the live-looking lease until its deadline and
return an explicit busy/unknown result. Never signal or kill a process or worker.

Allow one active wait per parent session. An exact duplicate returns `AlreadyWaiting { wait_id }`
without starting another monitor; a different set returns `Busy { wait_id, bound_workers }`. A dead,
identity-mismatched owner can be replaced only after recording `Interrupted`; deadline expiry records
`TimedOut` and releases only the lease. Neither outcome says a worker died. Retain terminal result
records for 24 hours, then safely prune only files whose validated lease identity is terminal.

The engine uses a clock/sleeper trait and fresh lifecycle replay at bounded intervals. It emits at
most one initial machine-readable record and one terminal record, with no unchanged tables. Outcomes:
all succeeded (exit 0), timeout (2), worker failed/cancelled (3), already waiting/busy (4), and
unknown identity/evidence (5). JSON output includes wait ID, bound parent and Loom session IDs, exact
workers, deadline, outcome, and evidence references. Human output carries the same distinctions.

## Regression proof

Unit tests use a fake monotonic clock. Replay T3's 902 two-second observations plus two simultaneous
watchers and prove one lease owner, one bounded monitor, no model-facing unchanged output, and no
redispatch/release of a live writer. Cover exact duplicate and different-set contention, PID reuse,
missing birth metadata, owner death, lease expiry, timeout without worker death, and terminal cache
cleanup.

The integration target drives the real CLI dispatch and predecessor journal/index formats. Cover two
parents in one cwd, newer unrelated transcript mtimes, distinct Claude-parent and Loom-session IDs,
mixed Claude/Codex sets, idle/running/succeeded/failed/cancelled/unknown, no workers, stale replay,
same transcript gaining a new turn after stop, daemon restart evidence, and malformed/symlinked
scratch state. Spawned children are always joined.

Done means `watch` is an exact owned runtime operation, a second caller cannot create another watcher,
and only fresh correlated success for every bound invocation returns exit 0.
