# runtime-and-dashboard-server / w5-terminal-bridge: terminal liveness, attach protocol, PTY descriptors

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md` sections D1 and CR-60; `doc/plans/briefs/review-remediation/validation/web.md` sections D2 and D3 (server halves only).

## Findings covered
- D1: a terminal peer that is connected but never reads holds its slot forever (the output flush stalls with no clock).
- CR-60 (a) no WebSocket ping and no inbound-silence deadline; (b) the original PTY slave is not close-on-exec (still true only outside Linux; a base commit opens the Linux pair close-on-exec from creation); (c) a failed `TIOCSCTTY` is ignored; (d) `Resize` is applied in `Mode::View`. Not covered (documented bounds): (e).
- D2 (server half): a failed target resolution or PTY spawn closes with no code (browser sees 1005), and the browser cannot tell "socket open" from "terminal attached".
- D3 (server half): a full terminal pool answers an authenticated client with an HTTP 503 before the handshake, which the browser reports as a bad cookie.
The browser side (`web/src/api/terminal.ts`, `terminal-notice.tsx`) is another stage and is built against the wire protocol pinned below.

## Wire protocol (pinned; the browser stage depends on it verbatim)
1. After the PTY child is attached, the server sends exactly one WebSocket text frame whose payload is exactly `{"ready":true}`, before any terminal output (binary frame).
2. Close code `4010` (`CLOSE_ATTACH_FAILED`) with reason `could not resolve the stage session` when target resolution fails, and reason `could not start the tmux attach client` when the PTY or tmux client cannot start. Existing refusals keep 4004, 4008, 4009 and their reasons.
3. Close code `4011` (`CLOSE_BUSY`) with reason `terminal limit reached` when an authenticated client finds every terminal slot taken: the handshake completes (HTTP 101) and the server then sends this close frame. Never an HTTP 503. Authentication, host, origin and path checks still run first, with their HTTP failures.
4. A stopping server (`running` false at admission) also completes the handshake and closes with the existing `CLOSE_SERVER_STOPPING` (1001), reason `server stopping`.
5. The server pings on an interval and closes a peer that stays silent past a deadline (bare close, no frame); an output flush that makes no progress for the stall timeout closes with 4008 reason `output stalled`.
6. `Resize` frames are ignored in view mode. The PTY starts at 120 columns by 36 rows and stays there for viewers.

## Files
Own (write), all under `loom/src/commands/status/web/terminal/`:
- `bridge.rs`, `bridge/liveness.rs` (new), `bridge/finish.rs` (new), `bridge/poll.rs` (new), `pty.rs`, `upgrade.rs`, `protocol.rs`
- tests: `tests_bridge.rs`, `tests_bridge/backpressure.rs`, `tests_bridge/liveness.rs` (new), `tests_pty.rs`, `tests_route.rs`, `tests_upgrade.rs`, `tests_upgrade/admission.rs`, `tests_upgrade/shutdown.rs`, `tests_upgrade/attach.rs` (new)
Read only: `resolve.rs`, `mod.rs` (no change needed: `upgrade.rs` reaches `protocol` items through `super::protocol::`), `commands/status/web/tests.rs` helpers.
Frozen (contract tests, never edit): `liveness_contract_tests.rs`, `busy_contract_tests.rs`, `ready_contract_tests.rs` in the same directory.
Not yours: `commands/status/web/tests/socket.rs` and everything under `web_server/` (worker `w4-web-authority` edits them; you need none of them).

## Changes

### 1. D1 and CR-60 (a, d): `bridge.rs` and new child modules
`bridge.rs` is 380 lines and `run_with` and `pump_socket` are 49 lines each: every function must stay at most 50 lines and the file well under 400. Structure:
1. `bridge/finish.rs`: move `enum Stop` and `fn finish` there (`pub(super)`), add variant `OutputStalled` mapped to `CloseFrame { code: CLOSE_REFUSED, reason: "output stalled" }`. `Stop::Stalled` keeps 4008 `input stalled` (an existing test asserts it).
2. `bridge/poll.rs`: move `struct Ready`, `fn poll_ready`, `fn socket_gone` and `POLL_TIMEOUT_MS` there unchanged (`pub(super)`), keeping their comments.
3. `bridge/liveness.rs`:
   - `#[derive(Clone, Copy)] pub(in crate::commands::status::web::terminal) struct Timing { pub(in crate::commands::status::web::terminal) stall: Duration, keepalive: Duration, silence: Duration }` (all three fields with the same visibility) and `pub(in ..terminal) const DEFAULT: Timing` as an associated const: `stall: super::GATE_STALL_TIMEOUT`, `keepalive: 15 s`, `silence: 60 s`.
   - `pub(super) struct Liveness { timing, gated_since: Option<Instant>, output_since: Option<Instant>, last_heard: Instant, last_ping: Instant }` with `new(timing, now)`, and (all take an explicit `now: Instant` so unit tests need no sleeping):
     - `gate_stalled(&mut self, now, gate_shut: bool) -> bool`: the existing `gate_stalled` logic (clock starts when the gate is first found shut, cleared when it opens, true once `timing.stall` elapsed). Delete the old free fn in `bridge.rs`.
     - `output_stalled(&mut self, now, stalled: bool) -> bool`: `stalled` false clears the clock and returns false; true starts it (`get_or_insert`) and returns whether `timing.stall` has elapsed.
     - `heard(&mut self, now)`: `last_heard = now`.
     - `ping_due(&mut self, now) -> bool`: true at most once per `timing.keepalive` (sets `last_ping = now` when it returns true).
     - `silent(&mut self, now, gate_shut: bool) -> bool`: while the gate is shut the peer is not being read, so set `last_heard = now` and return false (the deadline is paused); otherwise `now - last_heard >= timing.silence`.
   - Inline unit tests for each method with synthetic instants (stall clock starts/clears/fires; a flush that makes progress resets; ping once per interval; silence paused under a shut gate and fires after reopening plus `silence`).
4. `bridge.rs`: `mod finish; mod liveness; mod poll;` with `use`/`pub(super) use liveness::Timing;`. `run` calls `run_with(.., Timing::DEFAULT)`; `run_with(socket, child, mode, running, timing: Timing)` becomes `configure`, then `let stop = pump_loop(&mut socket, &child, mode, running, timing); finish(socket, child, stop);`. `fn pump_loop(..) -> Stop` holds the old loop with `Liveness::new(timing, Instant::now())`, returning `Stop` instead of `break`ing:
   - `gate_shut = pending.len() >= MAX_PENDING_INPUT`; `live.gate_stalled(now, gate_shut)` -> `Stop::Stalled`.
   - `pump_pty` now returns `Result<bool, Stop>`: `Ok(true)` when this pass's flush or send hit a stall (`WouldBlock`/`TimedOut`/`Interrupted`), `Ok(false)` when it completed or had nothing to do, `Err(stop)` as the old `Some(stop)`. Add `fn outbound(result: tungstenite::Result<()>) -> Result<bool, Stop>` built on the existing `outbound_stop` and `stalled` (do not change `outbound_stop`; the unit test `a_signal_interrupting_a_transfer_is_retried_not_read_as_a_dead_peer` keeps passing unchanged).
   - After `pump_pty`: `live.output_stalled(now, stalled)` -> `Stop::OutputStalled`. While a flush is stalled no new PTY bytes are read, so the queue holds about one frame and a client that drains 16 KiB per stall period survives.
   - `pump_socket` gains `live: &mut Liveness` and calls `live.heard(Instant::now())` for every `Ok(message)` (a `Pong` or `Ping` included, though `parse_client_frame` returns `None` for them). Extract the `match parse_client_frame(..)` into `fn apply_client_frame(child, mode, pending, message)` so `pump_socket` stays under 50 lines. In it, `ClientFrame::Resize` applies only `if mode == Mode::Control`; in view mode it joins the ignored arm (CR-60 d).
   - At the end of each pass call `fn keepalive(socket, live, stalled, gate_shut) -> Option<Stop>`: `live.silent(now, gate_shut)` -> `Some(Stop::Peer)`; else, when `!stalled && live.ping_due(now)`, `outbound_stop(socket.send(Message::Ping(Default::default())))` (never queue a ping behind a stalled frame).
5. `finish` is unchanged apart from the new variant.

### 2. CR-60 (b, c): `pty.rs`
`pty.rs::open_pty(size)` opens the pair and `PtyChild::spawn` calls it. The Linux variant already opens both ends close-on-exec in the opening call (`posix_openpt` with `O_CLOEXEC`, and `open_slave`, which takes the slave from the kernel with `TIOCGPTPEER`); do not change it. The non-Linux variant calls `openpty` and sets `FD_CLOEXEC` on the master only, so there the original slave is inherited by the child between `openpty` and `drop(slave)` in `spawn`: set `FD_CLOEXEC` on `pair.slave` too, in that function; failure is an error. Its doc comment already says a fork between `openpty` and the `fcntl` stays possible there (no atomic variant of `openpty`); extend it to name both ends. The slave clones from `try_clone` are close-on-exec, and `dup2` into 0/1/2 clears the flag on the child's stdio as intended. In the `pre_exec` closure of `PtyChild::spawn` return `Err(io::Error::last_os_error())` when `libc::ioctl(0, libc::TIOCSCTTY as _, 0)` is `-1`; `spawn` then fails and `upgrade.rs` reports it as the 4010 spawn failure.

### 3. D2 and D3: `protocol.rs` and `upgrade.rs`
1. `protocol.rs`: add `pub(super) const CLOSE_ATTACH_FAILED: u16 = 4010;`, `pub(super) const CLOSE_BUSY: u16 = 4011;` and `pub(super) const READY_FRAME: &str = r#"{"ready":true}"#;`, each with a doc line naming its meaning and that `web/src/api/terminal.ts` matches them literally.
2. `upgrade.rs`: add `pub(super) const RESOLVE_FAILED: &str = "could not resolve the stage session";` and `pub(super) const SPAWN_FAILED: &str = "could not start the tmux attach client";`.
3. `enum Admission<'a> { Attach { stage_id: &'a str, control: bool, slot: Slot }, Close { code: u16, reason: &'static str } }`. `admit` returns `Option<Admission<'a>>` (`None` still means an HTTP failure was written: host, 404 lane, origin, cookie, path). After the path check: `!running` -> `Close { CLOSE_SERVER_STOPPING, "server stopping" }` (checked before the slot, so a stopping server takes none); no slot -> `Close { CLOSE_BUSY, "terminal limit reached" }`; else `Attach`.
4. `handle_upgrade`: after `admit`, always `tungstenite::accept_with_config` (existing config and warning on failure), then `match admission { Close { code, reason } => close_with(&mut socket, code, reason), Attach { stage_id, control, slot: _slot } => start_bridge(..) }` (the slot guard lives to the end of the arm).
5. Replace `close_refusal(socket, Refusal)` with `fn close_with(socket: &mut WebSocket<TcpStream>, code: u16, reason: &str)` (same body: `CloseFrame { code: CloseCode::Library(code), reason: reason.into() }`, `socket.close(Some(frame))`, `flush`); `pub(super)` so tests reach it. Refusals call it with `refusal.close_code()` and `refusal.reason()`.
6. `start_bridge`: a resolution `Err` logs as today and `close_with(.., CLOSE_ATTACH_FAILED, RESOLVE_FAILED)`; a failed `PtyChild::spawn` logs and `close_with(.., CLOSE_ATTACH_FAILED, SPAWN_FAILED)`. Replace the final `bridge::run(..)` by `run_attached(socket, child, mode(control), running)`.
7. `pub(super) fn run_attached(mut socket: WebSocket<TcpStream>, child: PtyChild, mode: Mode, running: &AtomicBool)`: `socket.send(Message::text(READY_FRAME))`; on error `child.shutdown()` and return; otherwise `bridge::run(socket, child, mode, running)`. This is the seam the frozen `ready_contract_tests.rs` calls.

## Interfaces shared with other workers
None across workers. Within this worker's files: `bridge::Timing { stall, keepalive, silence }` and `Timing::DEFAULT` (visible in `terminal`), `bridge::run_with(socket, child, mode, running, timing: Timing)`, `upgrade::run_attached`, `upgrade::close_with`, the three protocol constants. The frozen contract tests call `bridge::run_with`, `bridge::Timing`, `upgrade::run_attached`, and the existing `tests_upgrade::{terminal_options, connect_terminal}` (keep those helpers unchanged).

## Tests
Harness: `tests_bridge.rs` has `Fixture { socket, running, server }`, `start_bridge(script, mode)`, `start_bridge_with(script, mode, socket_bytes, stall_timeout)`, `wait_for_binary`, `wait_for_binary_progressing`, `wait_for_close`, `wait_for_close_frame`, `join_within`, `skip_bridge_test`.
1. `tests_bridge.rs`: change the 4th parameter of `start_bridge_with` from `stall_timeout: Duration` to `timing: Timing` and pass it to `run_with`; `start_bridge` passes `Timing::DEFAULT`; add `mod liveness;`. `tests_bridge/backpressure.rs`: replace the import of `GATE_STALL_TIMEOUT` by `Timing`, pass `Timing::DEFAULT` at the three call sites that passed `GATE_STALL_TIMEOUT` and `Timing { stall: Duration::from_secs(1), ..Timing::DEFAULT }` at the one that passed one second. Assertions unchanged.
2. `tests_bridge/liveness.rs` (new):
   - `bridge_disconnects_a_peer_that_never_reads` (D1): script `yes`, `Mode::Control`, `socket_bytes = Some(8 * 1024)`, `Timing { stall: 1 s, ..DEFAULT }`, the client never reads; `join_within(fixture.server, 10 s)`. Fails today (never ends).
   - `bridge_survives_a_client_that_reads_slowly` (control, passes before and after): script printing about 300 KiB then `END`, stall 1.5 s, 8 KiB buffers; the client reads one message every 250 ms for at least 4 s (more than twice the stall time) and still reaches `END`.
   - `bridge_closes_a_silent_peer` (CR-60 a): script `sleep 30`, `Timing { keepalive: 200 ms, silence: 1 s, ..DEFAULT }`, the client never reads (so it never answers a ping); `join_within(server, 8 s)`. Fails today.
   - `bridge_keeps_a_peer_that_answers_pings`: same timing, the client calls `socket.read()` in a loop for 3 s (tungstenite answers pings on the next read); the server thread is still running afterwards; then close the client and join.
   - `a_viewer_resize_is_ignored` (CR-60 d): `Mode::View`, script `sleep 1; stty size; sleep 30`, send text `{"resize":{"cols":100,"rows":30}}`, `wait_for_binary(.., b"24 80", 10 s)` is true. Positive control in the same test: a second `Mode::Control` fixture with the same script prints `30 100`. Fails today for the view fixture.
3. `tests_pty.rs`: `slave_and_master_do_not_leak_into_the_child` (`#[cfg(target_os = "linux")]`): spawn `sh -c 'for f in /proc/$$/fd/*; do readlink "$f"; done'` through `PtyChild::spawn`, read the output, assert exactly three lines contain `/dev/pts/` and none contains `ptmx`. It passes before and after on Linux, where the pair is close-on-exec from creation; it guards that end to end. The existing `pty_slave_is_close_on_exec_and_peers_the_master` gates its close-on-exec assertion with `#[cfg(target_os = "linux")]`: remove that gate, so the non-Linux fix is asserted where it applies (macOS CI). Keep `pty_master_is_close_on_exec` and `pty_child_shutdown_reaps_the_child` (it asserts the `SIGHUP` exit, never a time bound) unchanged. CR-60 (c) has no regression test (the ioctl cannot be made to fail from a test).
4. `tests_upgrade/attach.rs` (new, `mod attach;` in `tests_upgrade.rs`; loopback listener, `tungstenite::accept` on a server thread, `tungstenite::client` in the test, `skip_without_loopback` and `skip_without_pty` guards):
   - `the_ready_frame_precedes_terminal_output`: the server thread spawns a `PtyChild` running `sh -c 'printf OUT; sleep 30'` and calls `run_attached(.., Mode::View, &running)`; the first client message is `Message::Text` equal to `{"ready":true}`, a later binary message contains `OUT`, no binary message arrives before the text one; set `running` false and join.
   - `close_with_sends_the_typed_close_frame`: `close_with(&mut socket, CLOSE_ATTACH_FAILED, RESOLVE_FAILED)`; the client reads `Close(Some(frame))` with `CloseCode::Library(4010)` and the exact reason; same for `SPAWN_FAILED`.
5. `tests_upgrade/admission.rs`, `terminal_slots_are_capped` (line ~141): rewrite. With all `MAX_TERMINALS` slots held, `connect_terminal(port, &token).read()` is `Message::Close(Some(frame))` with `CloseCode::Library(4011)` and reason `terminal limit reached` (the handshake completes; today `connect_terminal` panics on the 503). After `drop(held)`, `connect_terminal(port, &token).read()` is a close with code 4004 (the stage `missing` does not exist), proving the slot is usable again. The old assertion of an HTTP 503 body is removed.
6. `tests_upgrade/shutdown.rs`: rename `terminal_upgrade_after_stop_is_503` to `terminal_upgrade_after_stop_closes_1001`. It drives `handle_upgrade` inline on a raw stream: read the response as bytes (`read_to_end`), assert it starts with `HTTP/1.1 101` and that the bytes after the header block (`\r\n\r\n`) are the server frame `[0x88, 2 + len, 0x03, 0xE9]` followed by `server stopping`. In `shut_down_and_assert_slot_freed` (line ~210) accept `response.starts_with("HTTP/1.1 101")` as a third alternative beside the empty and 503 cases; the slot assertion stays.
7. `tests_route.rs` (`close_codes_match_the_literal_wire_values_the_browser_depends_on`, line ~141): add `assert_eq!(CLOSE_ATTACH_FAILED, 4010); assert_eq!(CLOSE_BUSY, 4011);` and an assertion that `READY_FRAME == r#"{"ready":true}"#`; extend the `use` list.
8. Existing tests whose assertions change: `terminal_slots_are_capped` (rewritten), `terminal_upgrade_after_stop_is_503` (renamed, rewritten), `shut_down_and_assert_slot_freed` (one extra accepted response). Fixture-only: the `start_bridge_with` signature and its call sites.
Contract tests to turn green: `commands::status::web::terminal::liveness_contract_tests::*`, `busy_contract_tests::*`, `ready_contract_tests::*`.

## Proof
From `loom/`: `cargo check --lib`, `cargo test --lib commands::status::web::terminal::`, and `cargo test --lib commands::status::web::tests::remote` (it reuses `connect_terminal`).

## Traps
- `pump_pty` must keep returning before reading the PTY when the flush stalled; reading on would defeat tmux's own backpressure (see the existing comment) and hide the stall.
- The ping must not be sent while `stalled` is true, or a stuck peer accumulates pings behind a stuck frame.
- Pausing the silence deadline while the gate is shut is required: pongs are unread then, and a healthy busy browser would be dropped.
- The `Admission::Close` branch runs without a slot and without a PTY; do not call `resolve_target` or spawn anything there.
- `close_with` after `accept_with_config` writes the 101 first; sending the close frame on the bare stream (no handshake) is the HTTP 503 behaviour being removed.
- Give `Timing`, its fields and `DEFAULT` the visibility `pub(in crate::commands::status::web::terminal)`; `pub(super)` stops at `bridge`, so `tests_bridge` and the contract files could not build one.
- `bridge/finish.rs` keeps the `4008 input stalled` text; `tests_bridge/backpressure.rs` asserts it.
