# Macos Socket And Pty Tests

> macOS setsockopt EINVAL after peer close; PTY 1 KiB reads in pacing tests

## Socket Tests That Unwrap setsockopt After the Peer Closed

**What happened:** `subscriber_contract_tests::eof_with_a_live_daemon_reconnects` failed on the macOS nightly lane with EINVAL at `stream.set_read_timeout(..).unwrap()`.
**Why:** macOS `setsockopt` returns EINVAL once the peer has closed the stream; the fake daemon drops the connection right after the subscription, so the call raced the close. Linux accepts the call.
**Prevention:** a test whose fake peer closes right after a handshake treats `set_read_timeout`/`set_write_timeout` failure as harmless (`.ok()`), as production `TuiApp::connect_and_subscribe` does; the following read reports EOF.
**Fix:** `commands/status/ui/tui/subscriber_contract_tests.rs::first_subscription` ignores the error.

## Slow-Reader Tests Paced in Messages Instead of Bytes

**What happened:** `tests_bridge::liveness::bridge_survives_a_client_that_reads_slowly` hit its 60 s budget on macOS every run while passing on Linux.
**Why:** the test read one WebSocket message per 250 ms tick. A macOS PTY read returns 1 KiB against up to 16 KiB on Linux, so the 300 KiB transfer was about 300 messages (75 s) on macOS and 19 on Linux.
**Prevention:** pace a slow reader by bytes per tick, never by messages or frames; frame size follows the platform's PTY read size.
**Fix:** `commands/status/web/terminal/tests_bridge/liveness.rs::take_tick` reads up to 16 KiB per tick.
