# Web Terminal

> loom status --web --terminals: a browser terminal attached to a session

## Enablement and the port-scoped cookie token

`loom status --web --terminals` turns on a write lane; bare `--web` never mints a
`TerminalLane` and every `/ws/terminal/*` request 404s before any other check runs
(`terminal/upgrade.rs::admit`, `lane` argument `None`). The token itself comes from
`terminal::token::mint()` (`terminal/token.rs:12-16`), called from `mod.rs::auth_token`
whenever the bind is remote or `--terminals` is set; `TerminalLane::from_token`
(`terminal/upgrade.rs:28-33`) wraps it with a cookie name from
`TerminalLane::cookie_name(port)` — named `loom_dashboard_<bound port>` because cookies
carry no port scope and loom deliberately runs several dashboards at once
(`listener::bind_first_available_port(host, start)`); one fixed name would let opening a
second instance's tokenized URL silently steal the first instance's cookie. In remote mode
this same token also backs the dashboard-wide cookie (`auth::Auth`, `ServeOptions` requires
`dashboard_token`/`terminal_token` to agree when both are set).

Bootstrap: `GET`/`HEAD /?token=<presented>` (`bootstrap.rs::bootstrap_terminal_token`,
:55-82) — right token sets the cookie via a 302 redirect to `/` with `Set-Cookie:
loom_dashboard_<port>=<token>; Path=/; HttpOnly; SameSite=Strict`
(`TerminalLane::bootstrap_cookie`, `terminal/upgrade.rs:39-50`); wrong token is a 403; no
`token` query param falls through to the ordinary page. In remote mode this route also
checks `Origin` (optional but, if present, must match the connection's own address) ahead of
the token exchange. The printed startup line (loopback, terminals enabled) is
`http://127.0.0.1:<port>/?token=<token>  (terminals enabled; Ctrl-C to stop)` (`mod.rs:176-179`).

## Admission order (`terminal/upgrade.rs::admit`, :85-134)

Checked in this order, each a hard refusal before the next runs — re-asserted at the lane
rather than trusted to the HTTP router above it, because this is a keystroke-injection
surface:

1. `policy.host_allowed(local, head)` — DNS-rebinding gate in loopback posture, exact
   socket-identity gate in remote posture; shared with the rest of the dashboard.
2. Lane present (`--terminals` was passed) — else 404, indistinguishable from an unknown
   route.
3. `origin_matches_host` (`terminal/upgrade.rs:168-180`) — `Origin` must be `http`/`https`
   with an authority byte-equal to `Host`; WebSockets ignore same-origin policy so this is
   the browser-side half of the gate.
4. `lane.cookie_matches(head.cookie)` — the cookie half; constant-time token compare
   (`token::matches`).
5. `terminal_path` (`terminal/upgrade.rs:182-191`) parses `/ws/terminal/<stage-id>/<view|control>`
   and rejects a `stage_id` that is not `is_plain_identifier`.
6. `acquire_terminal_slot` against `MAX_TERMINALS = 8` (`limits.rs:25`).
7. `running` (server not mid-shutdown).

Only after all seven does `handle_upgrade` (`terminal/upgrade.rs:59-82`) call
`tungstenite::accept_with_config` (capped at `MAX_INBOUND_BYTES = 64 KiB`, `limits.rs:36`,
one shared definition also used by `ws.rs` and `bridge::configure`) and `start_bridge`
(`terminal/upgrade.rs:136-165`) resolves the target and spawns the PTY.

## Close-code contract (`protocol.rs:61-73`, pinned byte-for-byte against

`web/src/api/terminal.ts:146-159`, via
`tests_route.rs::close_codes_match_the_literal_wire_values_the_browser_depends_on`. Three
classes of refusal close code, `4004/4008/4009`, plus the two ordinary WebSocket codes:

| Code | Name | Meaning | Page reaction |
| --- | --- | --- | --- |
| 1000 | `CLOSE_ENDED` | tmux attach client exited normally | `ended` |
| 1001 | `CLOSE_SERVER_STOPPING` | dashboard shutting down | `dropped` ("server stopping") |
| 1009 | `CLOSE_TOO_LARGE` | frame past `MAX_INBOUND_BYTES`, input stream desynced | `refused` ("input too large") — never retried, since a retry resends the same oversized frame |
| 4004 | `CLOSE_UNKNOWN_STAGE` | no stage with that id | `refused`, permanent, page never retries |
| 4008 | `CLOSE_REFUSED` | `NativeBackend`, `Unrenderable`, or `Stop::Stalled` (gate never reopened) | `refused`, permanent |
| 4009 | `CLOSE_NOT_YET` | `NoSession`/`NotReady` — stage exists, may become attachable | `waiting`, page retries after `RETRY_WAITING_MS = 2000` |

## Resolver: from stage id to a tmux target (`resolve.rs`)

`resolve_target` (`:108-118`) composes three independent lookups, never trusting a status
field: `known_stage` from `verify::transitions::load_stage` (does the stage exist at all),
`all` from `load_all_sessions` (every session ever recorded, for a better refusal reason),
and `live` from `live_tmux_sessions(work_dir.root())` — the PID-verified liveness list
(`orchestrator/terminal/tmux/viewer.rs:193-220`; a recorded session is live only if its PID
still resolves to the right process identity, so a `Paused` session — no live PID — is never
in `live`). `resolve` (`:57-73`) filters `live` to the stage via `matches_for_stage`, then
`pick_newest` (`attach/mod.rs:84-86`, max by `created_at`) picks the same session `loom
attach` would. `target_for` (`:88-106`) then requires `tmux_session_name`/`socket_name` to
both be `is_plain_identifier` (`Refusal::Unrenderable` otherwise) and the tmux endpoint
`ready` (`endpoint_ready`, else `Refusal::NotReady`). No match at all falls back to
`refusal_without_live_session` (`:75-86`), which re-checks `all_sessions` for a non-terminal
session on a non-tmux backend to report `NativeBackend` instead of a generic `NoSession`.

## The poll-based bridge loop (`bridge.rs::run_with`, :75-123)

One thread per terminal, `nix::poll` over the PTY master fd and the TCP fd, no async
runtime. Per iteration: check `running`, check `gate_stalled`, `poll_ready`, drain PTY→socket
(`pump_pty`), drain socket→PTY (`pump_socket`, gated), drain the pending-input queue into the
PTY (`drain_pending`), check `socket_gone`.

- **Non-blocking master.** `PtyChild::spawn` (`pty.rs:22-66`) sets `O_NONBLOCK` on the master
  fd via `fcntl` after `openpty` (which never sets it) — `read`/`write_some` translate
  `EAGAIN` to `Ok(None)`/`Ok(0)` rather than blocking the bridge thread. It also sets
  `FD_CLOEXEC` on the master (openpty gives that to neither end): left unset, the master
  survives into the tmux child across `spawn`'s fork+exec, so `shutdown`'s `drop(master)`
  never closes the last reference and the slave never sees a hangup — see
  mistakes/web-dashboard-server.md.
- **Bounded pending-input queue.** `MAX_PENDING_INPUT = 64 KiB` (`:24`) staged in a
  `VecDeque<u8>` between the socket and the PTY. `poll_ready` (`:158-192`) narrows the TCP
  event mask to `POLLIN` only while `pending < MAX_PENDING_INPUT`; at the cap it asks for
  nothing, so the kernel's own TCP receive window pushes back on the browser instead of the
  queue growing without bound. `MAX_INBOUND_MESSAGES = 64` (`:42`) additionally bounds how
  many messages one `pump_socket` pass parses, since view-mode input and resize frames are
  parsed and discarded without ever reaching `pending` and could otherwise starve PTY output
  reads in the same iteration.
- **Timeouts.** `READ_TIMEOUT = 5ms` / `WRITE_TIMEOUT = 250ms` on the TCP stream
  (`configure`, `:147-156`); `POLL_TIMEOUT_MS = 50` per `poll` call; `MAX_WRITE_BUFFER = 2
  MiB` on the WebSocket's own outbound buffer.
- **Gate-stall deadline.** Masking `POLLIN` off the gated socket means a half-closed peer
  (FIN → `POLLIN`/`POLLRDHUP` on Linux, never `POLLHUP` — see mistakes file) is invisible for
  the whole gated window, and no read-based EOF probe works either, since the gate exists
  precisely because the receive queue is left unread. `gate_stalled` (`:132-138`) instead
  times the queue sitting at its cap; past `GATE_STALL_TIMEOUT = 30s` the loop gives up
  (`Stop::Stalled` → close code 4008). This is the only mechanism that also covers a peer
  that is alive but has wedged the PTY, where no readiness flag or FIN is ever sent.
- **Shutdown drain.** `finish` (`:323-352`) closes the WebSocket (4008 with reason for
  `Stop::Stalled`, matching what `terminal.ts` already labels "refused") then calls
  `PtyChild::shutdown` (`pty.rs:115-131`): drop the master (now hangs up the slave correctly
  with `FD_CLOEXEC` set), poll `try_wait` up to 2s, then `kill`+`wait`. A terminal slot, its
  PTY, and its tmux child stay held for up to that 2s even on a clean close — see
  concerns/web-dashboard-latent-issues.md.

## View mode: `-f read-only`, not `ignore-size` (`resolve.rs::attach_args`, :120-134)

Both modes attach to a PTY sized to the browser window (`WindowSize { cols: 120, rows: 36 }`
at spawn, then `TIOCSWINSZ` via `PtyChild::resize` on each resize frame) — this was a
deliberate plan decision, not an oversight: `ignore-size` was considered and rejected because
it would force the agent's real terminal size (commonly ~220 columns) into the dashboard's
well. `View` mode instead passes tmux `-f read-only` (input blocked at the tmux layer; the
client still resizes the window) while `Control` mode is a plain `attach-session`. `loom
attach`'s own nested-session flow reflows the same way, so this keeps the dashboard
consistent with the rest of the tool rather than a special case.

Server-side, the bridge additionally never enqueues `ClientFrame::Input` while in `Mode::View`
— the frame is parsed and discarded, not merely rejected by tmux — so a compromised or buggy
client cannot even reach the PTY write path in view mode.

## Frontend: the `Emulator` interface and the lazy xterm chunk

`web/src/components/terminal/emulator.ts` defines `Emulator` (`:6-17`) — `open(host)`,
`write(data: Uint8Array)`, `onData(handler): () => void`, `fit(): EmulatorSize | null`,
`focus()`, `setReadOnly(readOnly)` ("view = stdin disabled, hollow bar cursor, no blink;
control = block cursor, blink"), `dispose()` — and `EmulatorFactory = () =>
Promise<Emulator>`, so `TerminalView`/`useTerminal` can be tested against a fake factory
without ever importing xterm. `createXtermEmulator` (`:110-156`) is the real factory: it
dynamically `import()`s `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-webgl`, and
`@xterm/addon-unicode11` (`:112-115`) precisely so the main dashboard bundle does not carry
them — they land in their own chunk (`dist/assets/xterm.js`), paid only by a page that
actually opens a terminal. WebGL renders when available; xterm falls back to its DOM renderer
otherwise (`installWebgl`, `:93-107`).

## Keystroke wire contract and the Esc rule

`protocol.rs::parse_client_frame` (`:38-51`): a **binary** WebSocket frame is raw keystroke
bytes, already xterm-encoded — sent as-is to the PTY. A **text** frame must be JSON
`{"resize":{"cols":N,"rows":N}}`, clamped to 1..=500 columns / 1..=200 rows
(`protocol.rs::clamp`, :54-59); anything else (wrong shape, out-of-range before clamping
changes it) is silently dropped, not an error.

`TerminalView`'s `onKeyDown` (`terminal-view.tsx:87-91`) is bound in the **bubble** phase, so
xterm's own listener on its hidden helper textarea has already run and turned the keystroke
into a binary frame before this handler sees it. In `control` mode, if the event originated
inside the terminal host, it calls `stopPropagation()` unconditionally — this is what sends
Escape to the live agent instead of letting it bubble up to the dialog's own
`onEscapeKeyDown` (which would otherwise close the dialog), and incidentally also keeps Tab /
Shift+Tab out of the dialog's focus trap. In `view` mode nothing is stopped: Escape bubbles
normally and closes the dialog, and the footer hint shows `Esc back to details`
(`terminal-view.tsx:340-377`).
