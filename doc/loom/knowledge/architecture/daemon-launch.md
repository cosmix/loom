# Daemon Launch

> Re-exec launch, readiness, socket path

## Launching the Daemon: Re-Exec and Readiness Pipe

`loom run` never forks the daemon. `DaemonServer::start` calls `launch::spawn_daemon`
(`daemon/server/launch.rs`), which re-executes `current_exe()` as
`loom run --daemon-child <absolute work root>` plus the run's flags (`--manual`, `-p N`,
`--no-merge`) and waits for the child to report ready. The child process:

- starts from `env_clear` plus the daemon allowlist (`DaemonEnvironment`, see
  [The Host Environment Allowlist](execution-containment.md#the-host-environment-allowlist)), with
  `LOOM_TERMINAL` set from the terminal `loom run` detected, stdin on `/dev/null`, and stdout and
  stderr on one pipe; the working directory is inherited because `loom repair` matches daemons by cwd;
- is the hidden `--daemon-child` flag (refused with `--foreground`, absent from `--help`);
  `commands/run/daemon_child.rs::execute` requires an absolute root, calls
  `signing::take_from_process()`, then `DaemonServer::serve`; `main.rs` skips terminal recovery for it
  so no escape sequences reach the pipe;
- `serve` (`daemon/server/lifecycle.rs`) calls `setsid`, takes the singleton lock before any
  destructive step, removes the stale socket, publishes the pid file and tokens, keeps the pipe,
  redirects stdout and stderr to `orchestrator.log` (rotating the previous one), writes `0x02`, then
  `run_server` binds the socket and writes `0x01` only after a successful bind (restoring the umask
  first on a failed bind).

The argv `loom run --daemon-child <root>` keeps the `pgrep -af 'loom run'` singleton check valid.

## Readiness Handshake and Failure Reports

`await_ready` reads the pipe in 50 ms slices. `0x02` means output now goes to the log, `0x01` means
ready, and any other bytes are diagnostic text (a startup error before the log redirect), of which it
keeps the last 64 KiB. Success needs `0x01` and the child still alive after a 1 s grace, and not
dumping core: when the grace ends, `ready_unless_dumping` reads `CoreDumping:` from
`/proc/<pid>/status` (`launch/child.rs::core_dumping`, always false without `/proc`). A piped core
handler (apport, systemd-coredump) holds an aborted child unreapable for about a second, past the
grace, so a dumping child is waited for up to the 10 s deadline and reported as an exit. Failures
exit 1 and name what happened plus the captured text and, once `0x02` was seen, the last 20 lines of
`orchestrator.log` (passed through `terminal_safe`):

| Outcome | Report |
| --- | --- |
| child exits before `0x01` or within the grace | its exit status or signal; the pipe is drained up to EOF or 1 s (`DRAIN_LIMIT`), because a process the child started could still hold a write end |
| child still dumping core when the grace ends | its signal once the core handler releases it, or "still dumping core" after the deadline |
| pipe closes with the child alive | the child is SIGTERMed (then SIGKILL after 2 s) and reaped |
| no `0x01` by the 10 s deadline | SIGTERM, reap, then the text and log tail |
| read or `try_wait` error | `await_ready` terminates a still-running child on every `Err` |

`terminate` signals only a child `try_wait` still reports running, so a reaped child's reusable pid
is never signalled. Unit tests never start a real daemon: `spawn_daemon` is disabled under
`cfg(test)`, and an integration target that reaches it calls `disable_spawn_for_tests` first. No
automated test runs the production re-exec; see
[No Automated Test For the Daemon Re-Exec](../concerns/platform-and-commit-gaps.md#the-production-daemon-re-exec-has-no-automated-test).

## The Socket Path Rule

`daemon/socket.rs` is the one place the socket is resolved. `socket_path(work_dir)` canonicalizes
the state root (a stage worktree's `.loom/work` is a symlink to it) and falls back to the given
spelling; the daemon's own bind keeps `work_dir.join(SOCKET_FILE)` because its root is already real.
`socket_path_fits` budgets `SUN_PATH_MAX = 104` bytes with a strict `<` (the NUL needs a byte; 104 is the
macOS/BSD bound, applied on Linux too) and `socket_path_problem` words the refusal with the longest
repository path that fits. `loom run` refuses a background run whose resolved path does not fit in
`prepare_background_run`, right after `work_dir.load()` and before the plan is marked in progress, so a
refusal never leaves the plan in progress; `--foreground` binds no socket and is not refused;
`loom init` warns from `create_or_adopt_work_dir`. Clients treat a path past the limit as
`DaemonReach::Unreachable`: see [Stage-to-Daemon Channels](../patterns/stage-daemon-channels.md).
