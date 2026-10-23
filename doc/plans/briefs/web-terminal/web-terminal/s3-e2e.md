# S3 — tmux end-to-end test and terminal smoke script (codex `gpt-5.6-terra`, effort xhigh)

Wave 2: you start after S1 and S2 are in the tree. You prove the whole server path against a REAL
tmux server, and give CI a shell-level probe of the gate. Read S1's and S2's code first
(`loom/src/commands/status/web/terminal/`, `mod.rs::ServeOptions`, `tests.rs::start_with`).

## Files

| Own (write) | Read only |
| --- | --- |
| `loom/tests/e2e/web_terminal.rs` | `loom/tests/e2e/tmux_backend.rs` (`skip_unless_tmux_can_bind` :201-220, `TmuxTmpDirGuard` :225-248, `TmuxServerGuard` :254-263, all `pub(crate)`) |
| `loom/tests/e2e/mod.rs` (an alphabetised list of `pub mod X;` at :6-19 — add `pub mod web_terminal;` after `pub mod uncommitted_changes;`) | `loom/tests/e2e/tmux_reconcile.rs` (`start_inner_server` :89-109 — replicate its `new-session` invocation) |
| `scripts/smoke-web-terminal.sh` | `loom/tests/e2e/helpers/session_io.rs` (`create_session_file(work_dir, &session)` :10) |
| `.github/workflows/ci.yml` (task 3) | `loom/src/commands/status/web/tests/socket.rs` (`connect_websocket` :175-190) |
| | `scripts/smoke-web-dashboard.sh` (the shape to copy) |

`tests/` can only reach `pub` items of the crate: `commands::status::web::{serve, ServeOptions}`,
`models::session::{Session, SessionStatus, SessionBackendKind, SessionType}`,
`Session::derive_tracking_key`, `orchestrator::terminal::tmux::socket_name`,
`orchestrator::terminal::tmux::socket::socket_path_for` (`pub`, `socket.rs:33`, what
`TmuxServerGuard` needs). Confirm each with `loom map --find-all <symbol>` before writing.

## Task 1 — `loom/tests/e2e/web_terminal.rs`

Every test is `#[serial]` (`serial_test::serial`, as `tmux_backend.rs:313` does): the tmux tmpdir
guard mutates process env. Guards, in order, each returning early with its SKIP line:
`skip_unless_tmux_can_bind(dir, test_name)`, then a loopback probe (`TcpListener::bind("127.0.0.1:0")`,
print `SKIP <name>: cannot bind loopback` on failure), then `which tmux`. The plan's stage sandbox
sets `allow_all_unix_sockets: true` and its acceptance runs this file with
`LOOM_E2E_REQUIRE_TMUX=1` (`tmux_backend.rs:205-208` turns the skip into a panic), so in-stage
these tests RUN FOR REAL; a skip there is a stage failure, which is intended.

Fixture: `TmuxTmpDirGuard::new()`; a tempdir with `.loom/work` initialised (`WorkDir::new(base)?
.initialize()?`); a `Session` (`Session::new()`, then set `stage_id = Some("web-term-stage")`,
`tracking_key = Session::derive_tracking_key("web-term-stage", SessionType::Stage)`, `backend =
SessionBackendKind::Tmux`, `status = SessionStatus::Running`, `pid =
Some(std::process::id())`); `create_session_file`.

**Two fixture facts the resolver actually depends on — neither is `session.pid`.**

1. *A stage file must exist.* The resolver reads the stage inventory
   (`verify::transitions::load_stage`, `pub`) so that 4004 means "no such stage"; with no stage
   file, `web-term-stage` IS an unknown stage and every test here closes 4004 instead of
   attaching. Create one with `verify::transitions::create_stage` (`pub`) for id
   `web-term-stage` before starting the server.
2. *A PID-identity file must exist.* The resolver's population is `viewer::live_tmux_sessions`,
   whose liveness is `TmuxBackend::is_session_alive` → `native::pid_only_is_alive` → the PID
   identity FILE; `pid_guard.rs:20-23` says outright that `session.pid` is never an identity
   fallback. Neither `write_pid_entry` nor `pid_file_path` is re-exported publicly
   (`native/mod.rs:38-40` exports only `read_pid_entry`, `wrapper_script_path`,
   `cleanup_stage_files`, `discover_claude_pid`), so write the bytes directly:

   ```rust
   let pid_key = format!("{}-{}", session.tracking_key, session.id); // window_title_and_pid_key's shape
   let pids = work_dir.root().join("pids");
   std::fs::create_dir_all(&pids)?;
   std::fs::write(pids.join(format!("{pid_key}.pid")), format!("{}\n", std::process::id()))?;
   ```

   One line, no start time: `read_pid_entry` parses `start_time` as absent,
   `verify_process_identity` returns `Unverifiable`, and `pid_only_is_alive` accepts
   `Unverifiable` as alive (`pid_guard.rs:39-44`). Without this file the session never enters
   `live_tmux_sessions` and every test gets 4009 instead of a screen.

Start the inner
server on socket `socket_name(&session)` with session name `session.tracking_key`:
`tmux -L <socket> new-session -d -s <key> -x 80 -y 24 sh` (the shell is the "agent"), wrapped in
`TmuxServerGuard` for teardown. Start the web server: `serve(listener, base, running,
ServeOptions { terminal_token: Some("e2e-token".into()) })` on a thread.

Tests (one `#[test]` each, or one test with three phases if the fixture cost is high — the skip
guard makes either fine):

- `control_mode_round_trips_a_command`: WebSocket to `/ws/terminal/web-term-stage/control` with
  `Origin: http://127.0.0.1:<port>` and `Cookie: loom_dashboard_<port>=e2e-token` — the cookie
  name carries the bound port, because cookies have no port scope and a fixed name lets two
  dashboards clobber each other's token (plan decisions table); format it from the port you
  already have. Send `{"resize":{"cols":80,"rows":24}}` then a binary frame carrying
  `printf 'hi-%s-web\n' from\r`; read binary frames until the concatenated bytes contain
  `hi-from-web` (10 s cap). Close.
  The literal `hi-from-web` MUST NOT appear in what you type. A PTY echoes the command line back,
  so typing `printf hi-from-web` and then searching the output for `hi-from-web` passes even when
  the shell never executes anything — it would prove only that the echo path works, which
  `view_mode_receives_the_screen_but_never_types` already proves. Splitting the marker across
  `printf`'s format and its argument means the string can only appear once the child actually
  RAN. (An equally good alternative: have the command write a file under the tempdir and poll its
  contents.)
- `view_mode_receives_the_screen_but_never_types`: `/view`; send binary `printf never-typed\r`;
  read frames for 2 s; assert bytes were received (the screen) and none contain `never-typed`.
- `detaching_reaps_the_tmux_client`: after closing the control socket, poll `tmux -L <socket>
  list-clients` for up to 3 s until it prints nothing.
- `server_stop_closes_with_1001_and_reaps`: with a control socket open and live (a first binary
  frame received), set `running` false WITHOUT closing the socket; assert the client reads a
  `Close` with code 1001 within 3 s, the server thread joins within 5 s (S2's `serve` drains
  bridges before returning), and `tmux -L <socket> list-clients` prints nothing afterwards.

Set `running` false and join the server thread at the end; the guards tear down tmux. These four
names are acceptance lines in the plan; a renamed test fails the stage.

## Task 2 — `scripts/smoke-web-terminal.sh`

Copy the structure of `scripts/smoke-web-dashboard.sh` (scratch workspace via `mktemp`, background
`loom status --web 0 --terminals`, port AND token extracted from the printed URL, trap on EXIT).
Assert with `curl -s -o /dev/null -w '%{http_code}'`:

- `GET /?token=<token>` → `302`, and `-D -` shows `set-cookie: loom_dashboard_<port>=`
  (case-insensitive) — assert the PORT-SCOPED name, not a bare `loom_dashboard=`.
- `GET /?token=wrong` → `403`.
- `GET /ws/terminal/x/view` with `Upgrade: websocket`, `Connection: Upgrade`,
  `Sec-WebSocket-Version: 13`, `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==`,
  `Origin: http://127.0.0.1:<port>` and NO cookie → `401`.
- same with `Cookie: loom_dashboard_<port>=<token>` and `Origin: http://evil.example` → `403`.
- same with `Cookie: loom_dashboard_<port>=<token>` and `Origin: http://127.0.0.1:<port+1>` (loopback,
  wrong port) → `403`.
- same with cookie and `Origin: http://127.0.0.1:<port>` (byte-equal to `Host`) → `101`. Verify
  EMPIRICALLY that `curl -s -o /dev/null -w '%{http_code}' --max-time 2` prints `101` and not
  `000` on this curl build (exit code 28 is expected); if it prints `000`, assert on
  `curl -si … --max-time 2 | rg -q "^HTTP/1.1 101"` instead and say so in your report.
- the 4004 close code, end to end: `curl` cannot read a close frame, so use a `python3` snippet
  (python3 is on the host and on `ubuntu-latest`; `websocat` is not installed and must not be
  required): open a raw socket, send the RFC 6455 handshake (`Upgrade`, `Connection`,
  `Sec-WebSocket-Version: 13`, a base64 key, the port-scoped cookie, the loopback Origin equal to
  `Host`) to `/ws/terminal/definitely-not-a-stage/view`, read the first frame after the `101`, and
  assert opcode `0x8` with close code `4004`. Fail the script if the code differs. 4004 now means
  "no stage with that id" specifically — the server checks the stage inventory — so an id matching
  no stage file is exactly the input that produces it. (A stage that exists but has no live
  session answers 4009; do not assert 4004 for that case.)
- `GET /api/status` → body contains `"terminals":true` (`jq -e '.terminals == true'`).
- `GET /` → the served HTML references `assets/index.js`, AND some `assets/*.js` other than
  `index.js` served by the binary contains `xterm-screen` (fetch `/assets/<name>.js` for each name
  listed in `web/dist/assets/`) — this is what proves the binary embeds the NEW bundle: a binary
  built before `bun run build` serves the pre-terminal dist and passes every other line above.

Print `smoke-web-terminal: ok` as the last line on success. `chmod +x`. The script must pass with
`scripts/smoke-web-terminal.sh loom/target/debug/loom` from the repo root. The CI
`smoke-web-dashboard` job installs ripgrep only, not tmux (`ci.yml:286-317`): nothing in this
script may depend on tmux (all its probes stop before `resolve`).

## Task 3 — CI

In `.github/workflows/ci.yml`, the `smoke-web-dashboard` job (`:286-317`) runs the dashboard
smoke script at `:316-317`; add one step after it running `scripts/smoke-web-terminal.sh` with
the same binary path. In the `Test (${{ matrix.name }})` job (`:55-92`), add
`LOOM_E2E_REQUIRE_TMUX: "1"` to the `Run tests` step (`:90-92`) as an `env:` ONLY for the `linux`
matrix leg (the `macos-build` leg is `--no-run` and installs no tmux; use
`${{ matrix.name == 'linux' && '1' || '' }}`) so a silent tmux skip fails CI instead of passing.
Change nothing else in the workflow.

## Constraints

- File ≤ 400 lines, function ≤ 50 lines (the maintainability gate scans `tests/` too).
- Every tmux invocation bounded (`wait-timeout` is a dependency; or a manual `try_wait` loop).
- Print the SKIP line to STDERR (`eprintln!`) exactly in the shape `tmux_backend.rs:211-217` uses
  (`SKIP <test_name>: ...`) so a sandboxed run reads as a skip, not a pass; the plan's acceptance
  greps `2>&1` for a line starting with `SKIP`.
- No `TODO`/`FIXME`. Never run `git`.

## Done means

`cargo test --test e2e web_terminal` compiles and either passes (on a host with tmux and AF_UNIX)
or prints the SKIP lines; `scripts/smoke-web-terminal.sh loom/target/debug/loom` passes on the
host (it needs a listening port, so run it once from the worktree root; if the sandbox denies the
bind, say so in your report instead of retrying).
