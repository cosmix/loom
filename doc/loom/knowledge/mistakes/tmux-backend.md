# Tmux Backend

> tmux spawn-failure exit codes, cleanup-on-every-error-path discipline, and PID reuse across a retried session id.

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

## `remain-on-exit` Cannot Protect Pane 0, Because Pane 0 Predates It (2026-08-11)

**What happened:** `loom attach` failed outright with `Error: tmux new-session failed for socket 'loom-view-f8a5e1db': server exited unexpectedly`. Retrying a minute later worked. Separately, and from the same underlying event, a _later_ pane showed `[server exited unexpectedly]` over `Pane is dead (status 1, ...)` and the viewer survived.

**Why:** both are one event — a per-session tmux server exiting — hitting the viewer in two places. `new-session` creates pane 0 with `exec tmux -L <session-socket> attach-session` _already running in it_. If that inner server is gone, the client exits instantly. `remain-on-exit` is applied on the NEXT step, so pane 0 dies unprotected: dead pane → dead window → dead session → and under the default `exit-empty on`, the viewer server exits too. Panes 1+ are created after `remain-on-exit` lands, so the identical event only leaves a labelled corpse there. The old note above ("set it BEFORE the splits") is correct and still insufficient: before the splits is still _after_ pane 0.

**Prevention:** a window option cannot retroactively protect a pane created by the same command that created its window. When a pane is born running something that can fail immediately, the protection must be a GLOBAL option set on the server before that command runs.

**Fix:** `VIEWER_HARDENING` (`commands/attach/overview.rs`) runs `start-server ; set -g exit-empty off ; set -gw remain-on-exit on ; set -g remain-on-exit-format ...` as ONE `;`-separated sequence before `new-session`. Three traps in that one line:

- **`start-server` alone is a no-op.** It brings up a server with no sessions, which `exit-empty on` reaps before a second `tmux` process could connect to configure it. Only a single command sequence gets `exit-empty off` in first. tmux's own manual documents the `tmux start \; show -g` idiom for exactly this.
- **tmux abandons the rest of a sequence when one command errors.** Order entries so each sits after everything that must not be able to abort it — cosmetic and version-varying settings last.
- **`exit-empty off` leaks an idle server per repo.** Accepted: a server that reaps itself cannot be configured before pane 0 exists. The viewer socket already had no reaper.

## PID Liveness Answers "Is the Agent Alive", Not "Can I Attach" (2026-08-11)

**What happened:** the viewer built panes for sessions that were alive by every existing filter but whose tmux server was not accepting clients — a session mid-spawn (discovery admits `Spawning`), or one whose server had just been torn down while its `claude` PID lingered. Each such pane exited on contact.

**Why:** `loom attach` reused `is_session_alive`, which is PID-only by deliberate design (see `architecture/terminal-backends.md`). That is the right liveness rule and must not change — but attaching needs the SERVER, and the two disagree in both directions.

**Prevention:** when reusing a predicate, check that it answers _your_ question. "Alive" and "attachable" are different questions about the same session.

**Fix:** `tmux_endpoint_ready` (`commands/attach/mod.rs`) adds socket-exists plus an authoritative `has-session` probe, on the attach path ONLY. Both call sites report the wait explicitly instead of letting tmux's own error surface. This is why `has-session` appears in `attach/` while being forbidden in the monitor — annotate any such use, or the next reader will "fix" the inconsistency in the wrong direction.

## Servers loom Creates Inherit the Operator's `~/.tmux.conf` (2026-08-11)

**What happened:** an operator could not select text in any agent pane, and a peer investigation attributed four "sessions crashing seconds after spawn" to stray mouse selections killing agents.

**Why:** tmux reads `~/.tmux.conf` at `start-server`, so every server loom creates inherits it. The operator's config had `set -g mouse on`; loom overrode only `status off`. With capture on, tmux consumes drags into copy-mode instead of letting them reach the terminal emulator, so mouse selection of agent output is impossible.

**The mechanism, established after two wrong turns:** mouse capture arms tmux's **default** root-table bindings — no user `bind` line required. `MouseDown3Pane` opens a menu containing `Kill X { kill-pane }`. In the overview a pane hosts a stage's own server running one agent, so killing it ends the session, `exit-empty on` takes the server down, and the operator sees `[server exited unexpectedly]` then `Pane is dead (status 1)`. Loom files a crash and retries, so it presents as a stage dying unprompted.

**Two wrong turns worth keeping, because both looked rigorous:** (1) "the config has no `bind` directives, so nothing destructive can fire" — invalid, because the destructive path ships with tmux and `mouse on` is merely what enables it. Absence of configuration is not absence of capability. (2) "crashes seconds after spawn do not fit a human clicking" — the lifetimes came from `last_active - created_at`, and `last_active` was frozen by the heartbeat-ownership bug below. Real lifetimes were 43s to 63min. Two investigations reasoned carefully from a field that a separate bug had silently broken.

**Prevention:** when a config file the tool does not own is read into the tool's runtime, enumerate what it can change and pin the settings that matter. Do not promote a correlation to a documented mechanism in user-facing docs — a wrong cause in a README sends every future reader chasing their mouse instead of the real bug.

**Fix:** `PRESENTATION_OPTIONS` (`orchestrator/terminal/tmux/mod.rs`) forces `status off` and `mouse off` on stage servers; `VIEWER_HARDENING` (`commands/attach/overview.rs`) does the same for the viewer. Both tests assert the VALUE, not mere presence: `("mouse", "on")` would be worse than omitting the entry, since it would force capture on for an operator who had turned it off.

**Partially superseded (2026-08-11, same day):** `mouse off` disarmed tmux's own bindings (the menu kill) but did NOT stop selection from killing agents, and did not give the operator selection back. The full mechanism and the complete fix are in the next section.

## `mouse off` Did Not Stop Mouse Selection From Killing Agents — the Agent's Own Clipboard Copy Crashes tmux 3.6a (2026-08-11)

**What happened:** with `mouse off` verified live on both the stage server and the viewer, the operator drag-selected in a pane and the stage still died: viewer pane showed `[server exited unexpectedly]` then `Pane is dead (status 1)`, monitor filed `Process no longer running`, stage retried. Same user-visible bug the previous fix claimed to close.

**The verified chain (each link reproduced, not inferred):**

1. claude enables all-motion mouse tracking (`1000/1002/1003` + SGR `1006`) in its pane. tmux mirrors the ACTIVE pane's mouse mode out to the attached client's terminal gated ONLY on that terminal having the `kmous` capability — the `mouse` server option plays no part in the mirroring (tmux 3.6a `tty.c:369,485,884`). Through the nested viewer this reaches the operator's real terminal, which is why drags stopped being native selection in the first place.
2. With `mouse off`, incoming client mouse input BYPASSES key tables entirely: `server_client_key_callback` hits "Forward mouse keys if disabled" and forwards straight into the pane app. So the pre-fix world (default `MouseDrag1Pane` binding → `send -M`) and the post-fix world deliver the same drag into claude by different routes. `mouse off` changed the route, not the outcome.
3. claude treats the drag as TUI text selection and copies it to the clipboard; inside tmux it does this by running `tmux load-buffer -w -` against the stage server (the binary contains the string; the dying server's `-vv` log shows the client connect, the 19-byte `load-buffer`, `after-load-buffer`, then nothing).
4. **tmux 3.6a crashes serving `load-buffer -w` while a client is attached.** Minimal repro, no claude and no mouse involved: pane runs `printf x | tmux load-buffer -w -` with any client attached → server dead. Plain `load-buffer -` is fine; detached is fine; crashes with AND without `~/.tmux.conf`, so it is an upstream 3.6a bug in the `-w`/OSC-52 client-write path, not a config interaction.
5. Server dies → claude gets SIGHUP → PID gone → crash filed → retry. The attach client in the viewer pane loses its connection, which is exactly the message: tmux prints `server exited unexpectedly` only for a LOST connection; a clean pane-exit cascade prints `[server exited]`.

**Fix:** delete the `kmous` capability for every client TERM on every server loom creates: `terminal-overrides[99]` = `*:kmous@` in `PRESENTATION_OPTIONS` (stage servers) and `VIEWER_HARDENING` (viewer). No loom server can then put any client terminal into mouse mode, so drags stay native emulator selection (the operator finally gets selection back) and no mouse event ever reaches the agent. The indexed slot makes re-application idempotent — the viewer hardening re-runs against the same long-lived server on every `loom attach` — and preserves the operator's own override entries (e.g. truecolor). Verified end-to-end: with the override, zero `\e[?100xh` reaches the emulator and the exact event burst that reliably killed claude + server is harmless.

**Debugging trap that nearly falsified the truth:** a dummy pane app (`exec cat > log`) received 0 bytes during injected drags, which "proved" tmux swallows mouse events — wrong. The pane tty was in CANONICAL mode and mouse sequences contain no newline, so the line discipline buffered them forever. tmux's own `-vv` server log (`writing mouse ... to %0`) was the ground truth. When testing "did bytes reach the app", put the receiving end in raw mode or read the server-side log; `cat > file` on a tty silently lies.

**Residual risk, recorded deliberately:** any future claude feature that runs `tmux load-buffer -w` from a keyboard path still crashes a tmux 3.6a stage server. The deeper shield — dropping `TMUX`/`TMUX_PANE` from the agent environment in the wrapper so claude never talks to loom's tmux at all — was considered and NOT taken: the wrapper allowlists them today and the blast radius of lying to the agent about its terminal is unassessed. Revisit if a non-mouse `load-buffer` death appears.
