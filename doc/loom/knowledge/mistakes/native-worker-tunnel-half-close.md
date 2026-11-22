# Native Worker Tunnel Half Close

> Tunnel splice cut replies when a client half-closed after its request

## The Tunnel Copier Dropped Replies After a Client Half-Close

**What happened:** `native_worker/proxy/tunnel.rs::splice` shut down both sockets as soon as either direction reached end of stream. A client that half-closes after its request, as `printf ping | nc -w 5` does, had its tunnel cut before the reply was relayed back. The macOS kernel test `native_macos_the_proxy_tunnel_round_trips_and_other_sockets_are_denied` failed six times in seven runs. No Seatbelt denial was involved.

**Why:** the copier treated a clean end of stream as teardown and never passed it on as a half-close. The kernel test that exercises it had never run (`mistakes/cargo-test-harness-layout.md`).

**Prevention:** a byte relay passes a clean end of stream on with a write-side shutdown, and closes both sides only on an error or when the far side ends. Test a relay with a client that half-closes and then reads.

**Fix:** `splice` treats `a` as the side that opened the connection. A clean end of `a` is passed on with `Socket::close_write`, and the reply from `b` still flows. Any error, or an end of `b`, closes both sockets. A half-closed tunnel waits only on `b`, which the proxy bounds with its head and idle timeouts. The regression test is `bridge_tests.rs::a_half_closed_client_still_receives_the_reply`.
