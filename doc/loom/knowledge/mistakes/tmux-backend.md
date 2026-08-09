# Tmux Backend

> Topic notes for the mistakes knowledge area.

## `tmux new-session` Exits 0 When the Server Fails to Start

**What happened:** `tmux new-session -d` can print `error creating <path> (Operation not permitted)` to stderr and **still exit 0**. Trusting the exit code alone means `spawn_in_tmux` returns success with no server, no pane and no agent — a silent spawn loss the orchestrator would later read as a crashed session.

**Reproduction (confirmed, not theoretical):** point `TMUX_TMPDIR` at a directory the sandbox permits **writes** to but not `AF_UNIX` **bind** (e.g. `/tmp/claude/<x>`). `mkdir` succeeds, `new-session -d` prints the error and exits 0, and the follow-up `has-session` then fails with `No such file or directory`. An earlier stage had recorded this as "a sandbox/seccomp state no CI runner reproduces" — that was wrong, and the guard is load-bearing.

**Prevention:** never treat exit status as success for a command that creates an OS resource. Assert on the resource. `spawn_in_tmux` therefore applies two checks after `new-session`:

1. `evaluate_new_session(socket, status_success, stderr)` — a **pure** decision fn (extracted so it is testable without tmux) that treats _any_ stderr with exit 0 as failure.
2. an authoritative `tmux has-session` probe against the socket.

**Trade-off to know about (see `concerns.md`):** rule 1 is deliberately blunt — a benign `~/.tmux.conf` deprecation warning also lands on stderr while the session is created fine, so it fails a working spawn. The `has-session` probe is the signal that could distinguish the two.

## Any Spawn Helper That Starts a Process Must Clean Up on _Every_ Post-Start Error Path

**What happened:** `TmuxBackend::spawn` tore down the tmux server on only **one** failure branch (`await_tmux_session_pid`). `spawn_in_tmux`'s own post-server branches — the `has-session` bail and the `evaluate_new_session` stderr rule — returned `Err` with a **live server and a live `claude` already running in the worktree**. `SessionBackend::dispatch_spawn` then swallowed the error and retried on the native lane, so **two `claude` agents ran in the same worktree on the same stage**. A stray `~/.tmux.conf` warning was enough to trigger it.

**Prevention:** route every error path after process start through one cleanup closure. Detection rule: grep for `?` between the line that starts the process and the final `Ok(session)` — each one is a leak unless it goes through cleanup.

## A Retry That Reuses a Session Id Adopts the Previous Attempt's PID

**What happened:** the native retry in `dispatch_spawn` reuses the **same** `Session`, so `prepare_session_launch` derives the **same** `pid_key` (`{tracking_key}-{session_id}`). `create_wrapper_script` does not truncate a pre-existing `.work/pids/<pid_key>.pid`, and `await_session_pid` returns the _first live_ PID it reads there. The native retry therefore adopted the orphaned tmux `claude`'s PID while stamping `session.backend = Native` — so `loom attach` hid it (backend filter), `kill_session` killed the wrong process, and the monitor tracked a stranger.

**Prevention:** a retry that reuses a session id **must** clear that session's PID and wrapper files first (`native::cleanup_stage_files`) before re-launching. Stale PID files are silent: they hand back a plausible, live, wrong PID rather than an error.

## Never Persist a Fall-_Back_ Marker Before Proving the Fallback Target Works

**What happened:** `dispatch_spawn` wrote the sticky `.work/terminal-backend-fallback` marker and retried natively **without checking the native lane could be built**. On a headless Linux box — exactly where the tmux backend exists to be used — `NativeBackend::new` bails in `detect_terminal()`, so the retry was guaranteed to fail _and_ the marker permanently disabled tmux for every later spawn until someone ran `loom run --backend tmux`.

**Fix:** the marker is now written only when the native lane is actually constructible. With no native lane, loom returns the **original tmux error** instead of retrying — a doomed native retry replaces the only useful diagnostic with `No terminal emulator found`.

**Prevention:** a sticky degradation marker is a promise that the degraded path works. Prove the target is usable _before_ recording the fallback, and never let a fallback discard the root-cause error.

## AF_UNIX Socket Paths: 104 Bytes, and Never Under `std::env::temp_dir()`

- `sun_path` is capped at **104 bytes on macOS**. Key loom tmux sockets on `session.id` (`session-<uuid8>-<unixts>`, ~25 chars), **never** on `stage_id` — plan stage ids run up to 128 chars and would silently blow the limit.
- On macOS `std::env::temp_dir()` is a ~57-byte `/var/folders/<...>/T/` path; adding tmux's own `tmux-<uid>/` plus a `loom-session-<uuid8>-<ts>` name overflows 104 and tmux fails with **`File name too long`**.

**Prevention:** `loom_socket_dir()` and the tmux e2e both use `TMUX_TMPDIR`-else-literal-`/tmp`, matching tmux's own convention. Any AF_UNIX path built from loom identifiers must be budgeted against 104 bytes explicitly. Detection: `File name too long` from `tmux new-session` under a redirected `TMUX_TMPDIR`.

## `kill-server` Does Not Reliably Unlink Its Own Socket

Verified on tmux 3.7b: after `kill-server` exits 0 the socket file persists with no listener. `TmuxBackend::kill_session` must explicitly `std::fs::remove_file()` the socket after `kill_socket_server()`, or callers and tests asserting the socket is gone after teardown will see it linger.

## A Destructive Sweep Must Read "Cannot Read the Evidence" as "Do Not Destroy"

**What happened:** an orphan-socket sweep keyed on _"session file exists AND session not alive"_ had a hidden failure mode — if the liveness helper returned `false` for an **unparseable** session file, a file caught mid-write would make the sweep kill a **live** session of its own repo.

**Fix (`tmux/socket.rs` `socket_session_is_alive`):** absent file ⇒ `false`; existing-but-unreadable/unparseable ⇒ **`true`**.

**Related rule, same sweep:** reap only sockets **positively attributed to this work dir**. The tmux socket dir is per-_user_, `loom init` calls `cleanup_orphaned_sessions()` unconditionally and _before_ the "`.work/` already initialized" bail, and at init time `.work/` may not exist at all — so "no matching session file" matches **every** socket, including another checkout's live ones. Unattributable sockets are reported, never killed.

## tmux Layout and Option Traps (all verified on tmux 3.7b)

- **`select-layout tiled` once, after all splits, hard-fails at 6+ panes.** Every `split-window -t <session>` targets the session's _current_ pane — the one the previous split just made — so heights halve 50 → 25/24 → 12/11 → 5/5 → 2/2 → `size or position no space for a new pane` on split 5 (that is 6 sessions, not the 7 naive halving suggests). **Fix:** re-tile after _each_ split. A pane-count test must exercise >5 panes or it will never see this.
- **`remain-on-exit` must be set BEFORE the splits, not after.** It is a **window** option, so it governs every pane regardless of creation order. Setting it early additionally protects panes that die _during_ the build — the realistic race, when a loom session ends between the liveness scan and its split. Setting it late leaves every pane unprotected for the whole build.
- **A single-string pane command runs under `default-shell`, i.e. the user's LOGIN shell** (verified: `show-options -g default-shell` = `/bin/zsh`), not `/bin/sh`. So `unset TMUX; exec tmux ...` as one string only clears `$TMUX` under sh/bash/zsh: under csh the pane env still has `TMUX=` (csh needs `unsetenv`), and fish has no `unset` builtin at all — the nested attach is refused. **Fix:** pass `sh` `-c` `<string>` as **separate argv words**; tmux `execvp`s when `argc > 1`, guaranteeing POSIX sh. Detection: a pane that dies instantly with `sessions should be nested with care` is a shell problem, not a tmux-flag problem.
- **`list-clients` on the INNER socket is the only proof a nested attach worked.** Pane count and `pane_current_command=tmux` look identical whether the attach succeeded or was refused.
