# Closed Pipe Panics

> println! on a closed stdout panics a server thread; drain piped stdout in tests

## A plan-server test closed the server's stdout and a background println! panicked

**What happened:** `runtime_is_private_ignores_session_state_and_sigint_stops_cleanly` failed on Linux CI with exit 1. The test's `start()` reader stopped draining stdout once it had the URL. The `loom-plan-load` thread then printed the read-only lines with `println!`. Those lines are non-empty only when prerequisites are missing, as on CI, so the print hit EPIPE and panicked. `loading::run` caught the panic as a failed load and stopped the server.

**Why:** `println!` panics on a closed pipe, and a panic on the load thread counts as a load failure. The test helper's early break made the race reachable.

**Prevention:** server startup output goes through `plan_server::lifecycle::print_lines` or `lifecycle::line`, which ignore write errors. A test that pipes a child's stdout keeps draining it until the child exits.

**Fix:** `lifecycle::print_lines` replaced the two `println!` loops in `plan_server/mod.rs`. `start()` keeps draining. The regression test is `lifecycle_tests::startup_lines_to_a_closed_pipe_do_not_panic`.

## A proxy test treated the 431 path's broken pipe as a send failure

**What happened:** `native_worker::proxy::tests::oversized_head_gets_431` failed on macOS CI with `send: BrokenPipe`. The proxy read up to its head limit, replied 431 and closed while the client was still writing its 9000-byte head.

**Why:** the `exchange` helper used `write_all(...).expect("send")`, which assumes the peer reads the whole request. A server that rejects early closes first, and whether the client is still writing is a timing race.

**Prevention:** a test client that sends more than the server is allowed to read accepts `BrokenPipe` on the send and asserts on the reply, which stays readable on a Unix socket.

**Fix:** `exchange` in `native_worker/proxy/tests.rs` accepts only `BrokenPipe` on the send.
