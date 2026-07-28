# Daemon Singleton

> Nothing enforces one daemon: a second loom run attaches to the same .work/ and both poll, spawn and merge.

## Daemon Singleton Not Enforced: Two `loom run` Processes Alive Concurrently (2026-05-13)

**Observed:** During the `autonomous-criteria-adjudication` plan's `integration-verify` stage, `loom status` (static) reported `○ daemon stopped` even though the orchestrator log (`.work/orchestrator.log`) was still being appended every ~5 seconds. `loom status --live` was still connected in another terminal. `ps -eo pid,etime,cmd | rg 'loom run'` revealed **two** daemon processes:

```text
  64657    11:19:57  loom run    # started ~06:30 UTC
1038911    01:39:24  loom run    # started ~16:11 UTC (lock mtime 16:13:18 UTC)
```

State files in `.work/`:

| File | State |
|------|-------|
| `orchestrator.sock` | **MISSING** |
| `orchestrator.pid` | **MISSING** |
| `orchestrator.lock` | Present, contains `1038911` (no newline), mtime 16:13:18 UTC |
| `orchestrator.log` | Actively growing; first dated entry is 16:13:18 UTC (matches lock mtime), no startup banner for the 06:30 daemon survives in the file |

`loom status` (static) thinks the daemon is down because it talks to `orchestrator.sock`, which no longer exists. `loom status --live` in another terminal was bound earlier and is still rendering stale state; new clients can't connect.

**Why this matters for the user-visible "stuck integration-verify" symptom:** With the IPC socket gone, the daemon is invisible to the operator. The stage status (`status: executing`, `started_at` 19h ago) looked frozen because no fresh updates were rendered via the static command, and the dashboard's TUI was reading a cache. Meanwhile the agent inside the container was genuinely stuck on a hung cargo test (separate concern), but the operator couldn't tell whether the daemon or the agent was at fault.

**Probable cause (best hypothesis):** A second `loom run` was invoked while the first was still alive — likely as an operator recovery action after the stage looked stalled. The startup path:

1. Rewrote `.work/orchestrator.lock` to the new PID (1038911) without verifying the old PID was actually dead, OR the lock-acquire path uses a non-blocking `flock` that succeeded because the old process had released its lock (e.g., on a SIGSTOP/SIGTSTP, or a dropped guard in a code path that doesn't re-acquire).
2. Bound a new socket at `.work/orchestrator.sock` — succeeded because either (a) the old socket file had been removed by a `loom stop` that failed to kill the process, or (b) `unlink + bind` is unconditional in the daemon startup path.
3. Did NOT find an existing PID file (or failed-soft on its presence) and did NOT signal/kill the old daemon.

Result: two competing daemons sharing the same `.work/` state, the older one inert or only partially functional, the newer one doing most of the work. The socket file went missing later (a third event we have no log evidence for — possibly `loom stop` was issued against the new daemon, removing the socket but leaving both processes alive because `loom stop` over a since-disconnected socket is a no-op or because both daemons trapped the signal and ignored it).

**What's needed:**

1. **`loom run` must enforce singleton invariant at startup.** Before claiming the lock or binding the socket, walk these in order: (a) read `orchestrator.pid` if present, (b) `kill -0 <pid>` to test liveness, (c) if alive AND its argv matches `loom run`, refuse to start with a clear `error: daemon already running (pid N)` message and exit non-zero. Do NOT delete state files in this path.
2. **PID file must be written on every successful startup and removed on clean shutdown.** Current state shows `orchestrator.pid` missing despite an active daemon — either it was never written, or it was deleted by a parallel/cleanup path. Both bugs deserve their own probe.
3. **Socket file existence and the daemon's aliveness should be reconciled by `loom status`.** When the static command can't connect to the socket but a `loom run` process matches in `ps`, report something more useful than "daemon stopped" — e.g., `daemon process N alive, socket missing — try 'loom repair'`.
4. **`loom repair` should detect duplicate daemons and offer to kill the older one** (preferring the one whose PID matches `orchestrator.lock`). Today `loom repair` doesn't appear to scan for this.
5. **Investigate whether the orchestrator-log file descriptor is held by both daemons.** Multiple writers to a single file with `O_APPEND` is benign per POSIX, but if either daemon does `truncate + write_at(0)` (i.e., overwrites with `O_TRUNC`), the other daemon's writes are silently lost. The log's first surviving line being timestamped to the newer daemon's startup suggests truncation happened.
6. **Suppress the `[Polling...]` TUI status line from the orchestrator-log file.** The log currently contains hundreds of these lines (visible interleaved with real WARN entries) — the TUI subscriber output is leaking into the daemon's stderr/stdout sink. Logs should only contain structured tracing output, not the TUI dashboard.

**Detection rules for future incidents:**

- `pgrep -af 'loom run'` returning more than one row is always wrong. Add a `loom repair` check.
- `loom status` reporting "daemon stopped" while `.work/orchestrator.log` is being actively appended to is always wrong — either the daemon is alive (bug: stale socket cleanup) or the log is being written by a stale child process (bug: orphaned background work).
- `orchestrator.pid` missing while any `loom run` process exists is always wrong.

**Where to look in code:**

- `daemon/server/lifecycle.rs` — daemonization, socket binding, PID file write. Check the order of: lock-acquire → PID-file-write → socket-bind. Each step must be atomic or roll back the previous on failure.
- `commands/run/mod.rs` — `loom run` entry point. Check whether it consults `orchestrator.pid` + `kill -0` before forking.
- `commands/stop.rs` — `loom stop` must ALWAYS kill the underlying process before deleting socket/pid. Verify there's no path where the socket is removed but the process survives.
- `commands/repair.rs` — extend with a "duplicate daemon" detector and a "socket-vs-process mismatch" detector.
- `daemon/server/core.rs` — confirm `unlink(socket_path)` before `bind` is guarded by a process-liveness check on the prior owner.

**Concrete evidence captured at time of writing:**

```text
$ ps -eo pid,etime,cmd | rg 'loom run'
  64657    11:19:57  loom run
1038911    01:39:24  loom run

$ cat .work/orchestrator.lock
1038911

$ ls .work/orchestrator.sock .work/orchestrator.pid
ls: .work/orchestrator.sock: No such file or directory
ls: .work/orchestrator.pid: No such file or directory

$ stat .work/orchestrator.log | rg Modify
Modify: 2026-05-13 20:50:35 +0300   # still growing every poll cycle

$ head -10 .work/orchestrator.log
Loaded base_branch from config: main
Warning: Failed to parse skill file ...
Warning: Failed to parse skill file ...
Warning: Failed to parse skill file ...
Orchestrator started, spawning ready stages...
[K2026-05-13T16:13:18.544430Z  WARN ... Recovering orphaned stage stage_id=integration-verify status=Blocked
2026-05-13T16:13:18.544458Z  WARN ... Failed to transition to NeedsHandoff during orphan recovery, bypassing
...
```

First dated log line is `2026-05-13T16:13:18.544430Z` — within 1s of the lock file's mtime. The 06:30 daemon's earlier log entries (10 hours of operation) are not present in this file; either the log was truncated at the second startup, or the first daemon was writing to a different sink (e.g., it had `eprintln!` redirected on stdout but the new daemon repointed the log fd).
