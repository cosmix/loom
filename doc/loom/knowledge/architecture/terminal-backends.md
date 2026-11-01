# Terminal Backends

>

## Two Lanes Behind One Dispatcher

`SessionBackend` (`orchestrator/terminal/backend.rs`) is the single type every spawn/kill/liveness
call goes through. It wraps two concrete lanes:

| Lane               | Type                                 | Where sessions run                      |
| ------------------ | ------------------------------------ | --------------------------------------- |
| `Native` (opt-in)  | `NativeBackend` (`terminal/native/`) | a host terminal emulator window         |
| `Tmux` (default)   | `TmuxBackend` (`terminal/tmux/`)     | a detached tmux server, no GUI required |

`SessionBackend::from_config(work_dir)` constructs `NativeBackend::new` eagerly when Native is configured (`orchestrator/terminal/backend.rs::from_config`), so valid configuration alone does not guarantee success: terminal detection can fail on a headless host. Tmux configuration constructs no native backend at startup. Each native detection/process-discovery probe has a two-second deadline.

When Tmux is configured but a persisted session belongs to Native, `native_lane` constructs the native backend lazily and memoizes success or failure in `OnceLock<Result<NativeBackend, String>>` (`orchestrator/terminal/backend.rs::native_lane`). The configured Native lane uses the eagerly constructed backend. `OnceLock` keeps the dispatcher `Send + Sync` for monitor access through `Arc`; storing errors as strings permits repeated reporting without rerunning terminal probes.

## Configuration and Lane Resolution

- **Config:** `[terminal]` / `backend = "native" | "tmux"` in `.loom/work/config.toml`.
  `TerminalConfig` (`models/session/types.rs::TerminalConfig`) holds one `SessionBackendKind`
  (`models/session/types.rs::SessionBackendKind`, `#[serde(rename_all = "lowercase")]`, `#[default] Tmux`).
  Helpers `read_terminal_config` / `write_terminal_config` (`fs/work_dir/config_sections/terminal_config.rs`); a missing
  `backend` key (section absent or empty) falls through to `~/.loom/config.toml`'s `terminal.backend`, THEN the built-in default —
  absence is the inheritance channel, not a synonym for any one backend. Written at init by
  `commands/init/plan_setup.rs`, and only when someone chose explicitly.
- **CLI:** `--backend <native|tmux>` on both `loom init` (skips the interactive prompt) and
  `loom run` / `loom run --foreground` (persists to `[terminal]`). `loom run`'s
  `resolve_backend_flag` (`commands/run/mod.rs::resolve_backend_flag`) is shared by both run paths.
- **`loom init`'s prompt inherits** (`commands/init/backend.rs`): pressing Enter returns `None`, so
  no `[terminal]` section is written and the repo follows the user config live; typing `native` or
  `tmux` pins that value into the workspace. The prompt's default label comes from
  `backend_default_label`, so an operator with `terminal.backend = "tmux"` in `~/.loom/config.toml` sees
  `(tmux, from ~/.loom/config.toml)` and keeps following the user config. The tmux-missing-from-PATH warning keys off `effective_backend` (explicit
  choice, else the user config) because an inherited tmux backend arrives as `None`.
- **Per-spawn resolution** — `SessionBackend::resolve_lane` returns the configured kind verbatim; there
  is no marker and no automatic lane switch. `dispatch_spawn` dispatches on that configured kind only:
  configured tmux with no tmux on PATH returns `Err` (message names the missing binary and how to
  reconfigure), and a tmux spawn failure returns `Err` with context `tmux spawn failed for session
'<id>'` instead of retrying on the native lane.

## Session-Recorded Backend Dispatch

`Session.backend: SessionBackendKind` (`#[serde(default)]`, `models/session/types.rs::Session`) records
the lane **actually used**, and is persisted to `.loom/work/sessions/<id>.md`.

This is the load-bearing part: sessions are reconstructed from disk after a daemon restart, so
kill/liveness must route on the _session's_ recorded backend, never on the currently-configured one.
`SessionBackend::is_session_alive` and `kill_session` dispatch on `session.backend`, so a run that
changed `[terminal] backend` between sessions still kills and monitors older sessions through the lane
that spawned them.

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

`TmuxBackend::is_session_alive` and the native/headless lanes consult the shared
`process::ProcessIdentity { pid, start_time }` service. There is deliberately no `tmux has-session`
fallback for liveness and no raw `session.pid` fallback for signaling.

A tmux server whose pane process has died but which has not yet reaped itself still answers
`has-session` with exit 0. Consulting it would make the monitor report a dead `claude` as alive, and
the crash would never be filed or retried — defeating the containment property the backend exists for.
The recorded start time is mandatory identity evidence. A mismatch means the recorded process is
dead; missing or unreadable evidence is `Unverifiable`. Neither outcome may signal the numeric PID.
The shared identity service gives native, tmux, headless, and daemon integrations the same fail-closed rule.

## Spawn Path and the Silent-Failure Guard

`spawn_in_tmux` cannot trust `new-session`'s exit code — tmux can print an error to stderr and still
exit 0. Two checks follow the spawn:

1. `evaluate_new_session(socket, status_success, stderr)` — a **pure** decision fn (no tmux, no
   filesystem) so the rule is unit-testable; it treats any stderr with exit 0 as failure.
2. an authoritative `tmux has-session` probe against the socket.

Every error path _after_ the server may exist routes through teardown — killing the socket server and
calling `native::cleanup_stage_files` to drop the PID/wrapper files — before returning `Err`. Skipping
either leaks a live agent (see `mistakes/tmux-backend.md`).

After a successful spawn, `PRESENTATION_OPTIONS` is applied best-effort: `status off`, `mouse off`,
and `terminal-overrides[99]` = `*:kmous@`. The last one is load-bearing: it deletes the `kmous`
capability for every client TERM so the server can never put an attached terminal into mouse mode —
otherwise claude's own all-motion mouse tracking is mirrored out to the operator's terminal, drags
are forwarded back into the agent, and claude's clipboard copy (`tmux load-buffer -w -`) crashes
tmux 3.6a. Full chain in `mistakes/tmux-backend.md`.

## Tmux Unavailable or Failing

There is no fallback marker and no automatic lane switch (removed 2026-09-13, see
[Live State Pollution](../mistakes/live-state-pollution.md)). A spawn dispatches on the configured
backend only:

- Configured tmux with no tmux on PATH → `Err("terminal backend \"tmux\" is configured but tmux is not
on PATH; install tmux or set [terminal] backend = \"native\" ...")`.
- A tmux spawn that fails after the server may exist → `Err` with context `tmux spawn failed for
session '<id>'`, after teardown (killing the socket server and calling
  `native::cleanup_stage_files`).

Either `Err` propagates to `stage_executor.rs`, which calls `block_and_undo_session`
(`orchestrator/core/session_lifecycle.rs`) to mark the stage `Blocked` with
`FailureType::InfrastructureError`. There is no auto-retry; the operator fixes tmux (or reconfigures
the backend) and runs `loom stage retry`.

`loom run` / `loom run --foreground` fail startup outright when the effective backend is tmux and tmux
is not on PATH (`resolve_backend_flag_with_probe`, `commands/run/mod.rs`); `loom init`'s equivalent
check stays an advisory warning, since `loom init` only persists config and never spawns anything.
See [Configuration Is the Only Authority for Configurable Behavior](../conventions.md) for why no
marker may override the configured backend.

## `loom attach` — Overview and Direct

`commands/attach/` discovers sessions with `backend == Tmux`, status `Running | Spawning`, a live PID
and a resolvable tmux session name, sorted oldest-first by `(created_at, id)`. `mod.rs` owns
discovery and direct attach; `overview.rs` owns the viewer server.

Both paths then apply one further precondition: `endpoint_ready`
(`orchestrator/terminal/tmux/viewer.rs`) — the session's socket exists AND `has-session` succeeds on
it. Attach and the viewer reconciler (below) are the only legitimate `has-session` callers, and it
is not a contradiction of the liveness rule above: PID liveness answers "is the agent alive",
attaching needs "is the server accepting clients", and the two disagree in both directions (a
`Spawning` session has a live wrapper PID before its server is up; a torn-down server can outlive
its `claude` PID by a moment). Attach's call sites report "still spawning, or just ended — re-run in
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
ownership-checked path.

## macOS Terminal Detection Priority

1. `LOOM_TERMINAL` env var (explicit override)
2. `TERMINAL` env var (user preference)
3. Parent process detection (walks process tree up to 10 levels via `ps`)
4. Cross-platform binary check (ghostty, kitty, alacritty, wezterm via `which`)
5. macOS native apps (`/Applications/Ghostty.app`, `/Applications/iTerm.app`, `Terminal.app` fallback)

Note: `$TERM_PROGRAM` is NOT checked.

## find_claude_path() (src/claude.rs)

Shared binary resolution: `which::which("claude")` -> `~/.claude/local/claude` -> `~/.local/bin/claude` -> `~/.cargo/bin/claude` -> `/usr/local/bin/claude` -> `/opt/homebrew/bin/claude`.

## Terminal History

Every tmux server starts with `set-option -g history-limit 50000` and `set-option -g remain-on-exit on` ahead of `new-session` (`tmux/history.rs` `SERVER_OPTIONS`), so the first pane keeps 50,000 lines and an agent that exits leaves its dead pane behind. Liveness stays PID-based, so a dead pane never reads as a live session.

`TmuxBackend::kill_session` saves the scrollback before it kills the server: `capture-pane -p -e -J -S - -E -` (`capture.rs` `capture_history_argv`), written through `safe_fs` to `.loom/work/terminal-history/<session-id>.terminal`, the id validated, symlinks refused, the tail kept within 4 MiB. A failed capture never blocks the kill, and an empty capture does not overwrite an earlier file. Every daemon and CLI teardown reaches `kill_session`; an agent that exits by itself reaches it through `monitor/session_events.rs` `mark_terminal` -> `release_ended_session` (`terminal/backend.rs` `end_tmux_server`, one retry, then an error naming `loom clean`). The sweeps in `loom clean --sessions` and `loom init` kill dead servers without saving history. Native sessions keep no history.

`terminal-history/` is a `STATE_SUBDIRS` entry (`fs/work_dir.rs`), so it exists before any session starts: every capsule denies it to Bash by its real path (`session_settings/contents.rs`), since the sandbox skips a deny whose path is missing at spawn, and `credential-guard.sh` refuses it to a session's file tools ([gaps](../concerns/sandbox-and-confinement-gaps.md#terminal-history-is-guarded-by-a-hook-not-a-boundary)). The web host serves a transcript read-only at `GET /api/terminal-history/<session-id>` (`commands/status/web/terminal/history.rs`, behind the terminal lane's Origin and cookie checks), and the dashboard replays it in an xterm with no input and a link handler that refuses navigation. Tmux tests guard every server they start with `TmuxServerGuard`, because `remain-on-exit` keeps a panicked test's server alive.

## Terminal Backend Entry Points

**Dispatcher and lanes**

- `loom/src/orchestrator/terminal/backend.rs` — `SessionBackend`: `from_config`, `resolve_lane`,
  `dispatch_spawn` and the lazy `OnceLock` native lane. Tests in `terminal/backend/tests.rs`.
- `loom/src/orchestrator/terminal/tmux/mod.rs` — `TmuxBackend`, `socket_name`,
  `spawn_in_tmux`, `evaluate_new_session`, `kill_session`.
- `loom/src/orchestrator/terminal/tmux/socket.rs` — `loom_socket_dir`, `socket_path_for`, `list_loom_sockets`, `socket_session_is_alive`, `kill_socket_server`.
- `loom/src/orchestrator/terminal/native/pid_guard.rs` — the deduped PID-file liveness layers shared by
  both lanes (six former copies).
- `loom/tests/e2e/tmux_backend.rs` — external e2e; `TmuxTmpDirGuard` works around the sandbox. Note
  `tests/` can only reach `pub` items, never `pub(crate)`.

**`[terminal]` config**

- `loom/src/models/session/types.rs` — `SessionBackendKind` (`native`/`tmux`, default
  `tmux`), `TerminalConfig`, `Session.backend`.
- `loom/src/fs/work_dir/config_sections/terminal_config.rs` — `TERMINAL_SECTION`, `read_terminal_config` /
  `write_terminal_config`.
- `loom/src/commands/init/plan_setup.rs::initialize_with_plan` — writes the section at init.

**`--backend` flag**

- `loom/src/cli/types.rs::Commands` — the `backend` option on `loom init` and `loom run`,
  `value_parser = ["native", "tmux"]`.
- `loom/src/commands/run/mod.rs::resolve_backend_flag` — shared by `run/mod.rs::execute_background` and
  `run/foreground.rs::execute`. Rejects unknown values, refuses to persist a change while the daemon is
  running (prints a restart hint), and calls `resolve_backend_flag_with_probe`, which FAILS startup
  (not just warns) when the effective backend is tmux and tmux is not on PATH.
- `loom/src/commands/init/backend.rs::resolve_backend_choice`; flag wins, else prompts when
  both stdin and stdout are TTYs, else inherits (user config, then the built-in tmux default).

**`loom attach`**

- `loom/src/commands/attach/mod.rs` — `execute`, `matches_for_stage`, `pick_newest`, `wait_for_stage_target`,
  `attach_direct`, `require_tty`; `attach/overview.rs` — `build_overview_argv` (pure), `run_overview`;
  `attach/wait.rs` and `attach/diagnose.rs` — endpoint wait and failure diagnosis. Tests in `attach/tests.rs`.
- `loom/src/cli/types.rs::Commands` — `Attach { stage_id: Option<String> }`; dispatch in
  `cli/dispatch.rs` (`Commands::Attach`). Shell completions complete live stage ids after `loom attach`.
