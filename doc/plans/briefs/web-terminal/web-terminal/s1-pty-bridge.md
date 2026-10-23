# S1 — PTY child and WebSocket bridge (codex `gpt-5.6-sol`, effort xhigh)

You own the process side of the browser terminal: a PTY-hosted child process, and the loop that
pumps bytes between that PTY and one already-accepted WebSocket. You do NOT touch routing,
authentication, the CLI, or the wire model — S2 owns those and will call the API you expose.

## Files

| Own (write) | Read only |
| --- | --- |
| `loom/Cargo.toml` (ONLY via `cargo add`, see task 1) | `loom/src/commands/status/web/ws.rs` (the existing pump: timeouts, `MAX_INBOUND_BYTES`, `send_pending`) |
| `loom/src/commands/status/web/terminal/mod.rs` (declarations only, see below) | `loom/src/commands/status/web/tests.rs` (`skip_without_loopback`, `start`, `workspace` helpers) |
| `loom/src/commands/status/web/terminal/pty.rs` | |
| `loom/src/commands/status/web/terminal/bridge.rs` | `loom/src/commands/status/web/tests/socket.rs` (`connect_websocket` at :175-190 — the tungstenite client pattern) |
| `loom/src/commands/status/web/terminal/protocol.rs` | `loom/src/verify/criteria/confine.rs:117` (the crate's one `pre_exec` precedent) |
| `loom/src/commands/status/web/terminal/tests_pty.rs` | `loom/src/process/mod.rs` (`run_bounded`, `is_process_alive`) |
| `loom/src/commands/status/web/terminal/tests_bridge.rs` | |

You also own `loom/src/commands/status/web/terminal/mod.rs`, which holds ONLY a module docstring,
the declarations, and one re-export — nothing else ever goes in it:

```rust
//! Browser terminals: a PTY-hosted `tmux attach-session` client per WebSocket.
mod bridge;
mod protocol;
mod pty;
mod resolve;
mod token;
mod upgrade;
#[cfg(test)]
mod tests_bridge;
#[cfg(test)]
mod tests_pty;
#[cfg(test)]
mod tests_route;
pub(super) use upgrade::handle_upgrade;
```

`resolve.rs`, `token.rs`, `upgrade.rs` and `tests_route.rs` are S2's files and may not exist yet
when you run your one check; if they are missing, skip the check and say so in your report — the
orchestrator verifies the integrated tree. S2 adds `mod terminal;` to `web/mod.rs`.

## Task 1 — enable the nix features (first, once)

From `loom/`: `cargo add nix --features term,poll`. `nix::pty` lives behind the `term` feature —
nix 0.31 has NO `pty` feature (`nix-0.31.3/src/lib.rs:168-170`: `feature! { #![feature = "term"]
pub mod pty; }`), and `cargo add` aborts without touching the manifest if you name one. `nix` is
already a dependency (`loom/Cargo.toml:26`, `["process", "fs", "signal"]`, `default-features =
false`); the features MERGE and the result is exactly one line:
`nix = { version = "0.31", features = ["process", "fs", "signal", "term", "poll"], default-features = false }`.
Confirm with `rg -n '^nix = ' loom/Cargo.toml` that it is still ONE inline line carrying
`default-features = false` (three acceptance regexes anchor on the `^nix =` prefix). Never hand-edit the
manifest.

Verify the API names against the INSTALLED crate source, not memory: `fd -t d 'nix-0.31'
~/.cargo/registry/src` then read `pty.rs` (`openpty`, `OpenptyResult { master: OwnedFd, slave:
OwnedFd }`, `Winsize`) and `poll.rs` (`poll`, `PollFd`, `PollFlags`, `PollTimeout`).

## Task 2 — `terminal/protocol.rs` (pure, no I/O)

```rust
/// Whether the browser may type into the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode { View, Control }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WindowSize { pub cols: u16, pub rows: u16 }

/// One message from the browser.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ClientFrame { Input(Vec<u8>), Resize(WindowSize) }

/// Binary frames are raw keystrokes; text frames are JSON `{"resize":{"cols":N,"rows":N}}`.
/// Anything else (malformed JSON, unknown keys, zero or absurd sizes) returns `None`.
pub(super) fn parse_client_frame(message: &tungstenite::Message) -> Option<ClientFrame>

/// Clamp to what a PTY can hold: 1..=500 columns, 1..=200 rows.
pub(super) fn clamp(size: WindowSize) -> WindowSize

/// Close codes the browser switches on. 1000 = the agent's tmux client exited normally.
pub(super) const CLOSE_ENDED: u16 = 1000;
pub(super) const CLOSE_SERVER_STOPPING: u16 = 1001;
/// No stage with this id exists. Permanent; the page never retries.
pub(super) const CLOSE_UNKNOWN_STAGE: u16 = 4004;
/// The stage exists but can never be attached in this shape (native backend,
/// unrenderable identifier). Permanent; the page never retries.
pub(super) const CLOSE_REFUSED: u16 = 4008;
/// The stage exists and may become attachable (no live session yet, still
/// spawning, endpoint not ready). The page retries every 2 s.
pub(super) const CLOSE_NOT_YET: u16 = 4009;
```

Three classes, not two: a two-code scheme forces one wrong answer in both directions — a
native-backend session is permanent and would be polled forever, a stage between two sessions is
transient and would never be retried. S2 maps `Refusal` onto these and pins the mapping with
`refusal_close_code_table`; F2 mirrors it in `terminal.ts`.

Use `serde_json` (already a dependency) with a small `#[derive(Deserialize)]` struct carrying
`#[serde(deny_unknown_fields)]` for the resize message. Tests for this file live in
`tests_bridge.rs` (table: binary → Input; valid resize; resize with extra key → None; text that is
not JSON → None; Ping/Pong/Close → None).

## Task 3 — `terminal/pty.rs`

```rust
/// A child process whose controlling terminal is a PTY we own.
pub(super) struct PtyChild { master: OwnedFd, child: std::process::Child }

impl PtyChild {
    /// Open a PTY pair sized `size`, give the child the slave as stdin/stdout/stderr and as its
    /// controlling terminal (setsid + TIOCSCTTY in `pre_exec`), spawn, close our copy of the slave.
    pub(super) fn spawn(command: &mut Command, size: WindowSize) -> std::io::Result<Self>
    pub(super) fn master(&self) -> BorrowedFd<'_>
    /// TIOCSWINSZ on the master; the child receives SIGWINCH.
    pub(super) fn resize(&self, size: WindowSize) -> std::io::Result<()>
    /// Read from the master. `Ok(None)` is "not ready yet" (`EAGAIN`/`EWOULDBLOCK` on the
    /// non-blocking master); `Ok(Some(0))` and `EIO` both mean the child side closed — map EIO
    /// to `Ok(Some(0))`.
    pub(super) fn read(&self, buf: &mut [u8]) -> std::io::Result<Option<usize>>
    /// ONE non-blocking write; returns how many bytes went out (`Ok(0)` on `EAGAIN`). There is
    /// deliberately no `write_all`: see the non-blocking note below.
    pub(super) fn write_some(&self, bytes: &[u8]) -> std::io::Result<usize>
    /// Close the master (the child gets SIGHUP), then kill-if-still-running and reap. Never blocks
    /// longer than 2 s: poll `try_wait` in 50 ms steps, then `kill()` + `wait()`.
    pub(super) fn shutdown(self)
}
```

Mechanics:

- `nix::pty::openpty(Some(&winsize), None)`; `Winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0,
  ws_ypixel: 0 }`.
- stdin/stdout/stderr: `Stdio::from(slave.try_clone()?)` three times (use `OwnedFd::try_clone`).
- `unsafe { command.pre_exec(|| { nix::unistd::setsid().map_err(io::Error::from)?;
  libc::ioctl(0, libc::TIOCSCTTY as _, 0); Ok(()) }) }` — `setsid` through nix, its `Errno`
  mapped to `io::Error` and PROPAGATED out of the closure (the crate's one `pre_exec` precedent,
  `verify/criteria/confine.rs:112-119`, does this with `setpgid`); a failed `setsid` makes
  `TIOCSCTTY` fail with `EPERM`, the child has no controlling terminal, and SIGWINCH never
  reaches tmux — resize silently stops working. The ioctl's own return is ignored (it fails
  harmlessly if the fd is already the controlling terminal on some platforms). `libc` is already a dependency.
  `TIOCSCTTY`'s type differs between Linux (`c_ulong`) and macOS (`c_uint`); cast to the type
  `libc::ioctl` expects on the target with `as _` as `fs/safe_fs.rs` does for its own ioctl-style
  calls. The crate builds on both.
- Resize: `libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &winsize as *const _)` in `unsafe`,
  mapping `-1` to `io::Error::last_os_error()`.
- `read`/`write_some` go through `nix::unistd::read`/`write` on the borrowed fd or through a
  `std::fs::File` built from a duplicated fd — either is fine; keep the master `OwnedFd` as the
  single owner.
- The master is NON-BLOCKING: set `O_NONBLOCK` on it immediately after `openpty`
  (`nix::fcntl::{fcntl, FcntlArg, OFlag}`), and let `poll` decide when each direction is ready.
  There must be NO unbounded blocking write anywhere in this module. A blocking `write_all` to a
  PTY whose child has stopped reading blocks forever once the input queue fills; the bridge then
  stops observing `running`, never releases its terminal slot, and defeats the server's 3 s
  shutdown drain. `connection.rs:17-34` and `ws.rs:32-39` already treat an unbounded blocked
  write as the shutdown hazard it is.

## Task 4 — `terminal/bridge.rs`

```rust
/// Pump bytes between an accepted WebSocket and a PTY child until either side ends.
/// Consumes both. Returns when the socket is closed and the child is reaped.
pub(super) fn run(
    socket: tungstenite::WebSocket<TcpStream>,
    child: PtyChild,
    mode: Mode,
    running: &AtomicBool,
)
```

Loop design (single thread, no second thread, no `mio`):

1. Socket timeouts: `set_read_timeout(Some(5 ms))`, `set_write_timeout(Some(250 ms))` — the
   read timeout is deliberately short because step 3 loops `read()` until it times out.
2. `poll` over the master and the TCP fd, `PollTimeout::from(50u16)`. The master is registered
   `POLLIN`, plus `POLLOUT` **only while `pending` is non-empty** (step 4a); the TCP fd is
   registered `POLLIN`:
   `poll(&mut [PollFd::new(master, if pending.is_empty() { POLLIN } else { POLLIN | POLLOUT }), PollFd::new(tcp, POLLIN)], PollTimeout::from(50u16))`
   (`impl From<u16> for PollTimeout` exists, `nix-0.31.3/src/poll_timeout.rs:101`). `EINTR` →
   continue. BORROWS: `PollFd::new(fd: BorrowedFd<'fd>, ..)` (`poll.rs:48`) holds the borrow for
   the array's lifetime, and `socket.get_ref()` is `&Stream` while step 3 needs `&mut socket` for
   `send` — so capture the TCP fd ONCE outside the loop as a `RawFd` (`socket.get_ref().as_raw_fd()`),
   re-derive it each iteration with `BorrowedFd::borrow_raw`, and build the `PollFd` array inside
   a block that ends before any use of `socket` or `child`.
3. If the PTY is readable: read up to 16 KiB; `Ok(None)` → not ready, fall through;
   `Ok(Some(0))` → send
   `Message::Close(Some(CloseFrame { code: CloseCode::Normal, reason: "ended".into() }))` and stop;
   else `socket.send(Message::binary(buf[..n].to_vec()))`, a send error → stop.
   If POLLHUP is set on the master without POLLIN, treat as `Ok(Some(0))`.
   tungstenite 0.30 spellings, verified against the installed crate: `CloseFrame { code, reason }`
   has `reason: Utf8Bytes` (`protocol/frame/frame.rs:24-29`), so `.into()` a `&str`; `4000..=4999`
   maps to `CloseCode::Library(n)` (`protocol/frame/coding.rs:255`) and `CloseCode::Bad(n)` fails
   `is_allowed()` — for 1001 use `CloseCode::Away`; `Message::Binary` holds `Bytes` and
   `Message::Text` holds `Utf8Bytes` (`protocol/message.rs:157-174`), so use the `Message::binary(..)`
   / `Message::text(..)` constructors (the crate's own idiom, `web/ws.rs:64`), never the tuple
   variants with a `Vec<u8>` or `&str`.
4. If the TCP side is readable: call `socket.read()` in a loop until it returns
   `Error::Io` with kind `WouldBlock` or `TimedOut` (tungstenite buffers several frames from one
   segment; only the first is visible to `poll`). Per message: `Close` or
   `ConnectionClosed`/`AlreadyClosed` → stop; `parse_client_frame` → `Input` is APPENDED TO `pending`
   ONLY in `Mode::Control` (in `View` it is dropped; the tmux client is also read-only, this is
   belt and braces); `Resize` → `child.resize(clamp(size))`; `None` → ignore. Any other
   `socket.read()` error → stop.
4a. `pending: VecDeque<u8>` is the browser-input queue, capped at
   `MAX_PENDING_INPUT: usize = 64 * 1024`: appending past the cap drops from the FRONT (oldest
   keystrokes) and logs once at `debug`, so a wedged child costs typed characters rather than the
   whole shutdown path. Whenever the master reports `POLLOUT`, drain with `write_some` in a loop
   until it returns `Ok(0)` or `pending` is empty, removing exactly what was written (partial
   writes are normal). A `write_some` error other than `EAGAIN`/`EINTR` → stop.
5. `!running.load(SeqCst)` → send `Close` with `CloseCode::Away` (1001) and reason `"server
   stopping"` and stop.
6. On stop: best-effort `socket.close(None)` + one `flush`, then `child.shutdown()`. `run` MUST
   return within ~3 s of `running` going false: S2's `serve` waits (3 s bound) for live bridges to
   drain before the process exits, and a bridge that lingers leaves a live `tmux attach-session`
   client on the stage's own server.

Inbound message and frame size caps: reuse the value of `ws.rs`'s `MAX_INBOUND_BYTES` (64 KiB) —
import it (make it `pub(super)` in `ws.rs`? NO: `ws.rs` is not yours; define your own
`const MAX_INBOUND_BYTES: usize = 64 * 1024;` in `bridge.rs` with a comment naming the twin).

## Task 5 — tests

`tests_pty.rs` (no network):

- `pty_child_echoes_input`: spawn `sh -c 'read line; printf "got:%s\n" "$line"'` at 80x24, write
  `hi\n`, read until the output contains `got:hi` (10 s cap), `shutdown`.
- `pty_child_resize_is_visible_to_stty`: spawn `sh -c 'read x; stty size'`, `resize(120x40)`,
  write `\n`, expect `40 120` in the output.
- `pty_child_shutdown_reaps_the_child`: spawn `sh -c 'sleep 30'`, shutdown, assert it returned in
  under 3 s and `is_process_alive(pid)` is false afterwards (grab the pid via `child.id()` before
  shutdown — expose `pub(super) fn pid(&self) -> u32`).
- `pty_child_resize_delivers_sigwinch`: spawn `sh -c 'trap "stty size" WINCH; read x'`,
  `resize(120x40)` WITHOUT writing anything first, expect `40 120` in the output within 5 s.
  (`stty size` after a `read` only reads `TIOCGWINSZ` off the fd and passes with or without a
  controlling terminal; this test is the one that proves `setsid` + `TIOCSCTTY` worked.)

`tests_bridge.rs` (loopback; guard every test with `crate::process::sandbox_probe::{loopback_bindable,
skip_unless}` — both `pub` (`sandbox_probe.rs:117,154`), the same probe `web/tests.rs`'s
`skip_without_loopback` wraps; import them, do not copy the shape):

- `bridge_relays_bytes_both_ways`: bind `127.0.0.1:0`; spawn a thread that `accept`s, runs
  `tungstenite::accept` and then `bridge::run(socket, PtyChild::spawn(sh -c 'read l; printf
  "echo:%s\n" "$l"'), Mode::Control, &running)`; on the test side `tungstenite::client` to
  `ws://127.0.0.1:port/`, send `Message::Text(r#"{"resize":{"cols":80,"rows":24}}"#)`, then
  `Message::Binary(b"ping\n")`, read binary frames until one contains `echo:ping`, then expect a
  `Close` with code 1000. 10 s cap on the read loop.
- `bridge_drops_input_in_view_mode`: same child; `Mode::View`; send `ping\n`; the child never
  gets a line, so no `echo:` frame arrives within 1 s; then close from the client and assert the
  server thread joins within 3 s (the child was reaped).
- `bridge_closes_1001_when_the_server_stops`: same fixture as `bridge_relays_bytes_both_ways`
  with a child that keeps running (`sh -c 'while read l; do printf "echo:%s\n" "$l"; done'`);
  after the first `echo:` frame arrives, set `running` to false; expect a `Close` with code 1001
  within 3 s, and the server thread to join within 3 s more (the child was reaped).
- `parse_client_frame_table` (pure) as listed in task 2.
- `pty_write_survives_a_child_that_stops_reading` (`tests_pty.rs`, no network): spawn
  `sh -c 'sleep 30'` — a child that never reads its stdin — then call `write_some` in a loop with
  a 1 MiB payload for at most 2 s and assert the loop RETURNS (it starts answering `Ok(0)` once
  the PTY input queue fills) rather than blocking. This is the test that proves the master is
  non-blocking; with a blocking master it hangs until the 300 s acceptance cap.
- `bridge_survives_a_flooded_child` (`tests_bridge.rs`): the `bridge_relays_bytes_both_ways`
  fixture with `sh -c 'sleep 30'` as the child; send 200 KiB of binary input in control mode,
  then set `running` false and assert the client sees a `Close` with code 1001 within 3 s and the
  server thread joins within 3 s more. Without the pending queue and `POLLOUT` this hangs.

Every test name above is an acceptance line in the plan; a renamed test fails the stage.

## Constraints

- File ≤ 400 lines, function ≤ 50 lines: `cargo test --test maintainability` enforces both and is
  in the stage gate. Split `bridge.rs` into helpers (`pump_pty`, `pump_socket`, `finish`) rather
  than one long loop body.
- No `unwrap` on socket or PTY results in non-test code; the thread must return, never panic
  (a panic here poisons nothing but leaks the child).
- No `TODO`/`FIXME` strings anywhere, including in comments (the stage's stub checker rejects them).
- Never run `git`. The orchestrator commits.
- Record surprises with `loom memory note "gotcha: ..."` — in particular anything the installed
  `nix 0.31` names differently from the signatures above.

## Done means

`cargo test --lib commands::status::web::terminal::` passes locally (run ONCE, this is your one
narrow check), every test name above appears in `cargo test --lib commands::status::web:: --
--list`, and `rg -n 'TODO|FIXME' loom/src/commands/status/web/terminal/` is empty.
