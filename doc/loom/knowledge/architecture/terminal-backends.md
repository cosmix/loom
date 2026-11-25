# Terminal Backends

>

## One tmux Server Per Session — Crash Containment

The tmux lane deliberately does **not** use one shared server with many windows. Each session gets its
own server on its own socket:

- socket name — `format!("loom-{}", session.id)` (`orchestrator/terminal/tmux/mod.rs::socket_name`)
- socket dir — `$TMUX_TMPDIR` else literal `/tmp`, joined with `tmux-<uid>` (`tmux/socket.rs::loom_socket_dir`)
- tmux session name inside that server — `session.tracking_key` (`models/session/methods.rs::new_contract`),
  so merge/knowledge/base-conflict prefixes resolve correctly

**Why per-session:** a wedged or killed server takes down exactly one stage. A shared server is a
single point of failure for every parallel stage at once.

**Why keyed on `session.id`, not `stage_id`:** `sun_path` is capped at 104 bytes on macOS; plan stage
ids run up to 128 chars and would silently blow the limit. See `mistakes/tmux-backend.md`.

## Liveness Uses Verified Process Identity, Not tmux

`TmuxBackend::is_session_alive` consults the shared `process::ProcessIdentity { pid, start_time }` service. There is deliberately no `tmux has-session` fallback for liveness and no raw `session.pid` fallback for signaling.

A tmux server whose pane process has died but which has not yet reaped itself still answers `has-session` with exit 0. Consulting it would make the monitor report a dead `claude` as alive, and the crash would never be filed or retried — defeating the containment property the backend exists for.
The recorded start time is mandatory identity evidence. A mismatch means the recorded process is dead; missing or unreadable evidence is `Unverifiable`. Neither outcome may signal the numeric PID.
The shared identity service gives tmux, headless and daemon integrations the same fail-closed rule. `TmuxBackend::kill_session` reads the PID identity before it tears anything down, asks the attributed tmux server to exit first, then terminates a `VerifiedAlive` pane PID through `launch::pid_only_terminate`; an `Unverifiable` entry is never signalled.

## Spawn Path and the Silent-Failure Guard

`spawn_in_tmux` cannot trust `new-session`'s exit code — tmux can print an error to stderr and still
exit 0. Two checks follow the spawn:

1. `evaluate_new_session(socket, status_success, stderr)` — a **pure** decision fn (no tmux, no
   filesystem) so the rule is unit-testable; it treats any stderr with exit 0 as failure.
2. an authoritative `tmux has-session` probe against the socket.

Every error path _after_ the server may exist routes through teardown — killing the socket server and
calling `launch::cleanup_stage_files` to drop the PID/wrapper files — before returning `Err`. Skipping
either leaks a live agent (see `mistakes/tmux-backend.md`).

After a successful spawn, `PRESENTATION_OPTIONS` is applied best-effort: `status off`, `mouse off`,
and `terminal-overrides[99]` = `*:kmous@`. The last one is load-bearing: it deletes the `kmous`
capability for every client TERM so the server can never put an attached terminal into mouse mode —
otherwise claude's own all-motion mouse tracking is mirrored out to the operator's terminal, drags
are forwarded back into the agent, and claude's clipboard copy (`tmux load-buffer -w -`) crashes
tmux 3.6a. Full chain in `mistakes/tmux-backend.md`.

## Tmux Unavailable or Failing

There is no fallback marker and no automatic lane switch (see [Live State Pollution](../mistakes/live-state-pollution.md)); tmux is the only lane. One message covers every refusal: `TMUX_MISSING_MESSAGE` (`orchestrator/terminal/backend.rs`), `"Loom needs tmux on PATH to start sessions. Install tmux and retry."`.

- `require_tmux(tmux_available: impl Fn() -> bool) -> Result<()>` returns `Err(anyhow!(TMUX_MISSING_MESSAGE))` when the probe says no. `tmux_on_path()` is the production probe (`which::which("tmux")`).
- `SessionBackend::ensure_tmux()` runs `require_tmux` on the backend's injected probe. `dispatch_spawn` calls it before any spawn writes launch state; `commands/stage/merge_resolver.rs::spawn_resolver_session` calls it as its first statement, before it publishes the merge snapshot, so a missing tmux leaves no write-ahead session or signal behind.
- `loom run` and `loom run --foreground` call `require_tmux(tmux_on_path)` before the daemon starts (`commands/run/mod.rs`, `run/foreground.rs`): a daemon started without tmux would fail every spawn. `loom init <plan>` only warns (`commands/init/plan_setup.rs::tmux_advisory`, built on `require_tmux`), since init never spawns anything.
- A tmux spawn that fails after the server may exist returns `Err` with context `tmux spawn failed for session '<id>'`, after teardown (killing the socket server and calling `launch::cleanup_stage_files`).

Either `Err` propagates to `stage_executor.rs`, which calls `block_and_undo_session` (`orchestrator/core/session_lifecycle.rs`) to mark the stage `Blocked` with `FailureType::InfrastructureError`. There is no auto-retry; the operator fixes tmux and runs `loom stage retry`.

## `loom attach` — Overview and Direct

`commands/attach/` discovers sessions with status `Running | Spawning`, a live PID
and a resolvable tmux session name, sorted oldest-first by `(created_at, id)`. `mod.rs` owns
discovery and direct attach; `overview.rs` owns the viewer server.

Both paths then apply one further precondition: `endpoint_ready`
(`orchestrator/terminal/tmux/viewer.rs`) — the session's socket exists AND `has-session` succeeds on
it. The probe also runs during spawn in `tmux/mod.rs::spawn_in_tmux` and when resolving a browser terminal. It establishes server readiness. The PID identity file establishes agent liveness, since a surviving pane or server can outlive its agent. Attach's call sites report "still spawning, or just ended — re-run in
a moment" rather than letting tmux's error surface.

Viewer identity (socket name, `loom-overview` session name, the nested-attach pane command) and
attachability discovery live in `orchestrator/terminal/tmux/viewer.rs`, shared by attach's one-shot
build and the reconciler so the two can never disagree; `commands/attach/overview.rs` keeps only the
build sequence (`build_overview_argv`, `VIEWER_HARDENING`).

- **No argument** ⇒ a tiled **overview**: a per-repo _viewer_ server on socket
  `loom-view-<sha256(canonical repo root)[..8]>`, session `loom-overview`, created detached at
  220x50, one pane per attachable session, re-tiled after **each** split.
  - `VIEWER_HARDENING` runs **before** `new-session`, as one `;`-separated sequence:
    `start-server ; set -g exit-empty off ; set -gw remain-on-exit on ; set -g mouse off ;
set -g terminal-overrides[99] '*:kmous@' ; set -g remain-on-exit-format`. The indexed
    `kmous@` entry is idempotent on purpose — this sequence re-runs against the same long-lived
    viewer server on every `loom attach`.
    Pane 0 is born already running an attach client, so only a GLOBAL option can protect it — the
    targeted `remain-on-exit` step after `new-session` is a belt-and-braces re-assertion. The
    sequence is best-effort: a tmux rejecting part of it degrades to the pre-hardening behaviour
    rather than failing the attach. See `mistakes/tmux-backend.md`.
- **With a stage id** ⇒ direct `exec` of `tmux -L loom-<session.id> attach-session -t <tracking_key>`,
  newest session wins if several match.

## Live Overview Reconciliation (Daemon-Side)

The overview is reconciled live, not a one-shot snapshot. `Monitor::poll` calls
`tmux::refresh_attached_viewer` (`orchestrator/terminal/tmux/reconcile.rs`) once per scheduler tick,
best-effort: a failed pass is logged at `warn` (not `debug`, which would hide every
later kill/add behind one bad step) and never fails the poll. The daemon never CREATES the viewer —
only `loom attach` does — it maintains one the operator already built. Gate order keeps the common
case free: viewer-socket `stat` (no subprocess, no session reads, no log when nobody is attached) →
bounded `has-session` (skip, logged at `debug` with the refusal/error, while absent or
mid-`loom attach` rebuild) → `list-panes -F` → pure diff (`reconcile_steps`, in
`reconcile/steps.rs` — the executor stays in `reconcile.rs`) → apply.

Both processes must resolve the same tmux socket directory: the orchestrator records its
`TMUX_TMPDIR` in `.loom/work/tmux-tmpdir` at start (`fs/tmux_tmpdir.rs`, removed at exit) and
`loom attach` adopts it while a daemon is alive, so a shell with a different `TMUX_TMPDIR` cannot
make the reconciler stat the wrong directory. The daemon's `work_dir` is absolute (`loom run`
passes its cwd, never `.`) because `viewer_socket_name` hashes the canonical repo root and a
relative path silently diverges on any `canonicalize` failure.

Panes are attributed by parsing the inner socket out of `#{pane_start_command}` (survives for
`new-session` and `split-window` panes; verified tmux 3.6a) with a `loom-` prefix requirement, so an
operator's own splits — including a manual `tmux attach-session` — are `None`-attributed and never
touched. Diff rules: missing attachable session ⇒ `split-window` re-tiled after EVERY split (the 6+
pane trap); dead pane whose server still lives ⇒ `respawn-pane`; dead pane whose session is gone ⇒
`kill-pane`, but never the last pane (a killed last pane collapses window and session despite
`exit-empty off`); duplicate panes for one socket (attach-rebuild race) collapse to one keeper; all
adds, then all respawns, then all kills, then ONE `select-layout tiled` iff any pane was killed.
`select-layout` steps are cosmetic: a failed one is logged at `debug` and the pass continues; every
other verb stops the pass on first failure. Every tmux call is bounded (`run_tmux_control` + probe
timeout) because it runs on the single scheduler loop. `tests/e2e/tmux_reconcile.rs` drives
`reconcile_viewer` against a real tmux (add, kill, floor); it skips loudly where AF_UNIX bind is
denied — every Claude Code sandbox — so it only truly runs on a developer host or CI.

Each overview pane nests into an inner server by running `unset TMUX; exec tmux -L <sock>
attach-session -t <key>` — passed as **separate `sh -c` argv words**, not one string, so the shell is
guaranteed POSIX (see `mistakes/tmux-backend.md`). `build_overview_argv` is a pure builder taking the
viewer socket and `(session_socket, tracking_key)` pairs as parameters, so it is testable without
tmux; socket derivation stays in `tmux::socket_name` and `viewer_socket_name`.

`loom attach` requires a TTY (`require_tty`), so its `exec` paths are unreachable from a non-TTY
harness — but the empty-set, unknown-stage and multi-match messages are emitted _before_ the TTY check
and are testable.

## Socket Reaping at init/clean

`cleanup_orphaned_sessions` reaps only sockets **positively attributed to this work dir** — never
"every `loom-*` socket with no matching session file", because the socket dir is per-user and would
match another checkout's live servers.

`SessionReapMode` distinguishes the two callers: `OrphansOnly` (normal path, skips live sessions) vs
`IncludeLiveBeforeClean`. `loom init --clean` deletes `.loom/work/` immediately afterwards, and `.loom/work/` is
the _only_ thing that makes attribution possible — so a live session left running through `--clean`
would become permanently unattributable and leak forever. `--clean` therefore reaps attributed sockets
even when alive; the normal path stays conservative.

`list_loom_sockets` skips names starting `loom-view-` so the overview viewer is not reported as
unattributable. Explicit clean and init retire this repository's viewer through its separate
ownership-checked path (`tmux/viewer/retire.rs`). An owned socket nothing listens on is what a dead
server leaves, and also what a starting one shows between its bind and its listen, so it is removed only
under tmux's own start lock, an exclusive `flock` on `<socket>.lock` (`tmux/viewer/start_lock.rs`, read
from tmux 3.6). A socket that listens and cannot be verified is refused. Residual: a server rebinding its
socket on SIGUSR1 takes no such lock.

## find_claude_path() (src/claude.rs)

Shared binary resolution: `which::which("claude")` -> `~/.claude/local/claude` -> `~/.local/bin/claude` -> `~/.cargo/bin/claude` -> `/usr/local/bin/claude` -> `/opt/homebrew/bin/claude`.

## Terminal History

Every tmux server starts with `set-option -g history-limit 50000` and `set-option -g remain-on-exit on` ahead of `new-session` (`tmux/history.rs` `SERVER_OPTIONS`), so the first pane keeps 50,000 lines and an agent that exits leaves its dead pane behind. Liveness stays PID-based, so a dead pane never reads as a live session.

**Fullscreen sampling.** A pane on tmux's alternate screen has no scrollback, so what it showed would vanish with the server. `Monitor::poll` (`monitor/core.rs`) calls `tmux::fullscreen::sample_fullscreen` each poll: every live tmux session on its alternate screen gets its visible screen saved as `history::save_last_screen`, replacing the previous frame. Sampling is best effort on the monitor's single thread: one tmux operation per session, a `SAMPLE_BUDGET` of 2 s for starting samples, and a start offset that rotates across polls, so a wedged server costs at most the budget plus one `TMUX_PROBE_TIMEOUT` and delays the sessions after it by one poll. `save_last_screen` records `Session.fullscreen_seen` (sticky, persisted in the session file) before it writes the frame, and writes nothing for an empty frame. Tests inject tmux through the `PaneReads` trait in `tmux/capture.rs` (`save_history_with`, `sample_fullscreen_with`).

`TmuxBackend::kill_session` saves the history before it kills the server, and `history.rs::capture_mode(fullscreen_seen, alternate_now)` decides what: a pane on the alternate screen now is captured `Visible`; otherwise a session ever seen fullscreen keeps its last sampled frame (`None`, the file stays); only a session never seen fullscreen gets `Full` scrollback, `capture-pane -p -e -J -S - -E -` (`capture.rs` `capture_history_argv`). The result is written through `safe_fs` to `.loom/work/terminal-history/<session-id>.terminal`, the id validated, symlinks refused, the tail kept within 4 MiB. A failed capture never blocks the kill, and an empty capture does not overwrite an earlier file. Every daemon and CLI teardown reaches `kill_session`; an agent that exits by itself reaches it through `monitor/session_events.rs` `mark_terminal` -> `release_ended_session` (`terminal/backend.rs` `end_tmux_server`, one retry, then an error naming `loom clean`). The sweeps in `loom clean --sessions` and `loom init` kill dead servers without saving history. The status data layer reports a stopped session's file as `HistoryKind::LastScreen` when `fullscreen_seen` is set (`status/data/stopped.rs`; the attention view labels it "last screen"), else as scrollback.

`terminal-history/` is a `STATE_SUBDIRS` entry (`fs/work_dir.rs`), so it exists before any session starts: every capsule denies it to Bash by its real path (`launch/session_settings/contents.rs`), since the sandbox skips a deny whose path is missing at spawn, and `credential-guard.sh` refuses it to a session's file tools ([gaps](../concerns/sandbox-and-confinement-gaps.md#terminal-history-is-guarded-by-a-hook-not-a-boundary)). The web host serves a transcript read-only at `GET /api/terminal-history/<session-id>` (`commands/status/web/terminal/history.rs`, behind the terminal lane's Origin and cookie checks), and the dashboard replays it in an xterm with no input and a link handler that refuses navigation. Tmux tests guard every server they start with `TmuxServerGuard`, because `remain-on-exit` keeps a panicked test's server alive.

## Terminal Backend Entry Points

**Backend and tmux lane**

- `loom/src/orchestrator/terminal/backend.rs` — `SessionBackend` (`new`, `ensure_tmux`, the private `dispatch_spawn`, `kill_session`, `release_ended_session`, `is_session_alive`), `require_tmux`, `tmux_on_path` and `TMUX_MISSING_MESSAGE`. Tests in `terminal/backend/tests.rs`; `terminal/contract_tests.rs` pins the missing-tmux contract.
- `loom/src/orchestrator/terminal/tmux/mod.rs` owns `socket_name`, `spawn_in_tmux` and `evaluate_new_session`. It re-exports `TmuxBackend`; the implementation, including `kill_session`, lives in `terminal/tmux/backend.rs`.
- `loom/src/orchestrator/terminal/tmux/socket.rs` — `loom_socket_dir`, `socket_path_for`, `list_loom_sockets`, `socket_session_is_alive`, `kill_socket_server`.
- `loom/src/orchestrator/terminal/session_tail.rs` — `session_tail(session, lines)`: the last lines of a session's tmux pane, for the park report of a hung session (`orchestrator/core/event_handler/recover_hung.rs`); it takes no work dir.
- `loom/tests/e2e/tmux_backend.rs` — external e2e; `TmuxTmpDirGuard` works around the sandbox. Note
  `tests/` can only reach `pub` items, never `pub(crate)`.

**Launch machinery (`loom/src/orchestrator/terminal/launch/`)**

- `mod.rs` — `build_claude_command`, plus the re-exports the tmux lane and the rest of the crate use (`cleanup_stage_files`, `read_pid_entry`, `wrapper_script_path`, `create_wrapper_script`, `session_settings_path`, `prepare_session_launch`).
- `prepare.rs` (with `prepare/command.rs`, `prepare/host.rs`) — assembles one session's launch: capsule, settings, wrapper script, build cache.
- `capsule.rs`, `session_settings.rs` (+ `session_settings/contents.rs`, `host_sockets.rs`), `wrapper.rs` (+ `wrapper/script_text.rs`, `wrapper/host_env.rs`), `build_cache.rs`, `session_log.rs`.
- `pid_tracking.rs`, `pid_wait.rs` and `pid_guard.rs` — PID/wrapper files, the wait for the `exec`ed process, and the PID-file liveness and termination layers.

**`loom attach`**

- `loom/src/commands/attach/mod.rs` — `execute`, `matches_for_stage`, `pick_newest`, `wait_for_stage_target`,
  `attach_direct`, `require_tty`; `attach/overview.rs` — `build_overview_argv` (pure), `run_overview`;
  `attach/wait.rs` and `attach/diagnose.rs` — endpoint wait and failure diagnosis. Tests in `attach/tests.rs`.
- `loom/src/cli/types.rs::Commands` — `Attach { stage_id: Option<String> }`; dispatch in
  `cli/dispatch.rs` (`Commands::Attach`). Shell completion offers live stage ids after `loom attach`.

## One tmux Lane Behind SessionBackend

`SessionBackend` (`orchestrator/terminal/backend.rs`) is the single type every spawn, kill and liveness call goes through. It is a thin facade over `TmuxBackend` (`terminal/tmux/`): every session runs in a detached tmux server, no GUI required. It holds two fields, the `TmuxBackend` (which owns the work dir) and an injectable `tmux_available: fn() -> bool` probe; a work-dir field of its own would be read by nothing and fail clippy `dead_code`. `SessionBackend::new(work_dir)` reads no configuration; `with_tmux_probe` (test-only) swaps the probe. Nothing selects a lane (no config section, flag or environment variable, and a `Session` records no backend), and `is_session_alive` and `kill_session` forward straight to `TmuxBackend`.

`dispatch_spawn` is the one private seam all five spawn entry points share (stage, contract, merge, knowledge, adjudication). It calls `ensure_tmux()` FIRST, before anything writes a wrapper or PID file, then runs the tmux spawn wrapped in the context `tmux spawn failed for session '<id>'`. Shared launch code (capsule, session settings, wrapper script, build cache, PID tracking and the PID-file liveness layers in `pid_guard.rs`) lives in `orchestrator/terminal/launch/`; see Terminal Backend Entry Points.
