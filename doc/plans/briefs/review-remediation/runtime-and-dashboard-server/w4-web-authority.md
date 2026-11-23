# runtime-and-dashboard-server / w4-web-authority: loopback Host and Origin policy, accept-loop rejection

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/security.md` section CR-04; `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md` section CR-59.

## Findings covered
- CR-04 (narrowed): on a loopback bind `AccessPolicy::origin_allowed` ignores the port and scheme of `Origin`, and `/ws` accepts an absent `Origin`, so a page on `http://localhost:3000` can open `ws://127.0.0.1:<dashboard>/ws` and read the live status stream. The loopback `Host` parser (`http::origin_host`) also accepts `[::1].evil.com` and `127.0.0.1:80@evil.com`.
- CR-59: the accept loop answers a connection refused at the cap with a blocking drain (50 ms reads, up to 300 ms) followed by a write, so one trickling client stalls every other accept.

## Files
Own (write), under `loom/src/`:
- `web_server/access.rs`, `web_server/access_tests.rs`, `web_server/http.rs`, `web_server/connection.rs`, `web_server/unserved.rs`
- `commands/status/web/router.rs`, `commands/status/web/mod.rs` (doc comment only), `commands/status/web/config_api/csrf.rs` (doc comment only)
- `commands/status/web/tests.rs`, `commands/status/web/tests/socket.rs`, `commands/status/web/tests/socket_edges.rs` (new), `commands/status/web/tests/config_api.rs`, `commands/status/web/tests/pure.rs`
Read only: `plan_server/gate.rs` (it calls `AccessPolicy::host_allowed`), `commands/status/web/terminal/upgrade.rs` (its own exact `origin_matches_host` stays), `web_server/hub.rs` (not yours).
Frozen (contract tests, never edit): `web_server/access_contract_tests.rs`.

## Changes

### 1. CR-04: same-origin sockets on a loopback bind, strict `Host` grammar
Requirement that shapes this change: a loopback dashboard is reached through SSH tunnels that map any local port (`ssh -L 8080:127.0.0.1:7373 host`), and that must keep working. The server therefore never compares a loopback `Host` or `Origin` with its own listening port. It compares `Origin` with the request's own `Host`: a browser page served through the tunnel sends `Host: localhost:8080` and `Origin: http://localhost:8080`, which agree; a page on another local port (`http://localhost:3000`) that opens the socket sends its own `Origin` next to the dashboard's `Host`, which do not. The planner (`plan_server/gate.rs::origin_matches`) and the web terminal (`terminal/upgrade.rs::origin_matches_host`) already use this rule. The remote posture (a non-loopback bind) is not changed by this worker in any way.

`web_server/access.rs` has the strict parser the remote posture uses: `parse_authority`, `remote_origin_authority`, `authority_matches`. The dashboard and the planner both call `AccessPolicy::host_allowed(local, head)`, so one `Host` fix covers both; `plan_server` has no separate parser.
1. `AccessPolicy::host_allowed`, loopback branch: `head.host.as_deref().is_some_and(loopback_host)` with a new private `fn loopback_host(host: &str) -> bool`:
   - `parse_authority(host)` is `Some((Authority::Localhost, _))`: true;
   - `Some((Authority::Literal(ip), _))`: true only when `ip` is `127.0.0.1` or `::1` exactly (`Ipv4Addr::LOCALHOST`, `Ipv6Addr::LOCALHOST`; import what is missing);
   - `None`: true only when `host` equals, ASCII case-insensitively, one of `localhost`, `127.0.0.1`, `[::1]` (a port-less `Host`: a tunnel on port 80, and existing tests send a bare `Host: localhost`).
   The port of a loopback `Host` is never compared (`Host` is the DNS-rebinding gate; the port is whatever the tunnel mapped; the existing test `default_loopback_compatibility_is_unchanged` pins `127.0.0.1:41599` as accepted). The remote branch is unchanged.
2. `AccessPolicy::origin_allowed`, loopback branch (replace the two `http::` calls; keep the `if !self.remote` split and leave the remote arm as it is): an absent `Origin` is `!require`; a present one is allowed only when `head.host` passes `loopback_host` and a new private `fn same_origin(origin: &str, host: &str) -> bool` holds: `origin` is `http://` or `https://` followed by exactly `host`, compared ASCII case-insensitively, with nothing after it (so a path, query, fragment, userinfo, `null` or any other scheme fails). `local` is not consulted on this branch. `https` stays accepted because the current loopback rule accepts it and a TLS-terminating forwarder that preserves `Host` must not start failing.
3. Delete from `web_server/http.rs`: `LOOPBACK_HOSTS`, `origin_host`, `is_loopback_host`, `host_allowed`, `origin_allowed_strict`, `origin_allowed` and their doc comments (lines ~168-231). `access.rs` then imports only `RequestHead` from `http`. Nothing else in `loom/src` or `loom/tests` calls them (checked with `rg`; the only other callers are two tests in `commands/status/web/tests/pure.rs`, handled below).
4. `commands/status/web/router.rs`, `handle_websocket_upgrade` (lines ~53-63): always `OriginRequirement::Required`; remove the loopback/remote split and its comment. A browser sends `Origin` on every WebSocket upgrade, through a tunnel too.
5. Docs that name the old behaviour: the module comment of `web_server/access.rs` (the sentence that loopback keeps the relaxed behaviour), the `Host`/`Origin` paragraph near the top of `commands/status/web/mod.rs`, and `config_api/csrf.rs` (`http::origin_allowed_strict` becomes `AccessPolicy::origin_allowed` with `OriginRequirement::Required`). State the new rule: on a loopback bind `Host` must be a loopback name in the strict grammar with any port, a present `Origin` must be `http://` or `https://` plus that same `Host`, and `/ws` and writes require `Origin`; on any other bind both must name the accepted socket exactly, as before.
6. In the `access.rs` module comment, state why the loopback branch compares `Origin` with `Host` and not with the listening port: a forwarded local port (an SSH tunnel) differs from the listening port, and tunnelled access is a requirement.

### 2. CR-59: reject at the cap without waiting
`web_server/connection.rs`: `reject_unavailable` (line ~71) currently sets a write timeout and calls `fail`, which runs the blocking `drain_pending`. Replace its body so it never blocks the accept loop:
1. `stream.set_nonblocking(true)`; read what has already arrived, in 4 KiB chunks until `WouldBlock`, end of stream, an error, or `MAX_DRAIN_BYTES` (no timeout, no sleeping); `stream.set_nonblocking(false)`; `stream.set_write_timeout(Some(REJECT_WRITE_TIMEOUT))` with a new `const REJECT_WRITE_TIMEOUT: Duration = Duration::from_millis(250)`; `http::write_response(.. 503, "Service Unavailable", "text/plain; charset=utf-8", body, true)`; `stream.shutdown(Shutdown::Write)`. Return quietly on any step that fails.
2. Keep `drain_pending` (blocking, 300 ms) and `fail` for the connection-thread callers (`plan_server/request.rs:124`, `commands/status/web/config_api/request.rs:115`, `web_server/http.rs:327`). Rewrite the comments at the `DRAIN_DEADLINE` constant and above `reject_unavailable` that say the drain "runs on the accept loop".
3. Both callers are in `web_server/unserved.rs` (`spawn_connection` cap path, `answer_unserved` thread-spawn-failure path); neither changes. Trade-off (state it in the doc comment): a request that arrives after the single read pass may leave unread bytes, turning the close into an RST that can discard the 503; the message is advisory.
`web_server/unserved.rs` has an inline test `an_unserved_connection_is_answered_with_a_503`; it must keep passing.

## Interfaces shared with other workers
None. `w5-terminal-bridge` does not use any file of this worker (its terminal tests build their own sockets and `Origin` headers; the helpers `terminal_options`, `terminal_request`, `connect_terminal` it keeps are untouched here).

## Tests
Fixtures: `commands/status/web/tests.rs` has `workspace()`, `start(base)`, `start_with(..)`, `stop(running)`, `request(port, raw)`, `body(&response)`, `skip_without_loopback(name)`; `web_server/access_tests.rs` has `head(host, origin)`, `loopback_policy()` (local `127.0.0.1:7373`), `remote_policy()`.
1. `web_server/access_tests.rs` (193 lines; add at most about 60):
   - `loopback_origin_must_equal_the_request_host`: with `local = 127.0.0.1:7373` and `OriginRequirement::Required`, as (`Host`, `Origin`) pairs. True: (`127.0.0.1:7373`, `http://127.0.0.1:7373`); (`localhost:7373`, `http://localhost:7373`); (`localhost:7373`, `http://LocalHost:7373`); (`localhost:8080`, `http://localhost:8080`) (a tunnel on another local port); (`localhost`, `http://localhost`); (`[::1]:9000`, `http://[::1]:9000`); (`localhost:8443`, `https://localhost:8443`). False: (`127.0.0.1:7373`, `http://localhost:3000`); (`127.0.0.1:7373`, `http://127.0.0.1:5173`); (`localhost:8080`, `http://localhost:3000`); (`127.0.0.1:7373`, `http://localhost:7373`) (a different origin to a browser); (`localhost:7373`, `http://evil.example`); (`localhost:7373`, `null`); (`localhost:7373`, `http://localhost:7373/x`); (`localhost:7373`, `ftp://localhost:7373`); (`evil.example:7373`, `http://evil.example:7373`) (`Host` is not loopback); an absent `Host` with any `Origin`. With `Optional` an absent `Origin` is true. Fails today (cross-port origins pass).
   - `loopback_host_uses_the_strict_authority_grammar`: true for `127.0.0.1`, `LOCALHOST`, `[::1]`, `127.0.0.1:41599`, `[::1]:7373`, `localhost:7373`; false for `[::1].evil.com`, `[::1].evil.com:7373`, `[::1`, `127.0.0.1:80@evil.com`, `evil.example`, `127.0.0.1.evil.example`, `loom.test:7373`, `::1`, and an absent `Host`. Fails today (`[::1].evil.com`, `127.0.0.1:80@evil.com` pass).
2. `commands/status/web/tests/pure.rs`: delete `origin_allowed_accepts_loopback_and_rejects_foreign` and `host_allowed_accepts_loopback_and_rejects_everything_else` (lines ~87-125); they pin the deleted `http::` functions and their loopback cases move into the two tests above. Remove any import left unused.
3. `commands/status/web/tests/socket.rs` (393 lines; it must end under 400):
   - `connect_websocket` (line ~176): add `Origin: http://127.0.0.1:{port}` to the client request (`request.headers_mut().insert("Origin", ..)`), because `/ws` now requires it; the client's `Host` is `127.0.0.1:{port}`, so the two agree.
   - `handshake_survives_a_split_head` (line ~308): its `headers` literal sends `Host: localhost`, so it gets the line `Origin: http://localhost`.
   - Move `a_connection_past_the_cap_is_answered_503` (lines ~343-370) unchanged to the new file below, and drop the `MAX_CONNECTIONS` import here if it becomes unused.
4. `commands/status/web/tests/socket_edges.rs` (new; register it in `tests.rs` like the others: `#[path = "tests/socket_edges.rs"] mod socket_edges;`; use `super::{body, request, skip_without_loopback, start, stop, workspace}`):
   - `a_connection_past_the_cap_is_answered_503` (moved).
   - `websocket_from_another_localhost_port_is_refused`: raw `GET /ws` with `Host: 127.0.0.1:<port>`, `Upgrade: websocket`, `Connection: Upgrade`, `Sec-WebSocket-Version: 13`, `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==` and `Origin: http://localhost:3000` -> `HTTP/1.1 403`, body `origin not allowed`. Also the same request with no `Origin` -> 403. Fails today (101).
   - `websocket_through_a_forwarded_port_is_accepted`: the same raw upgrade with `Host: localhost:8080` and `Origin: http://localhost:8080` (neither names the server's port) -> the response starts with `HTTP/1.1 101`. This pins the tunnel requirement; it passes today and must keep passing.
   - `a_rejected_connection_does_not_wait_for_a_trickling_client`: hold `MAX_CONNECTIONS` silent connections as the moved test does; start 6 trickler threads that connect and then send one byte every 40 ms for 3 s (ignore write errors, they end when the server closes); sleep 100 ms; connect a probe, send a complete `GET / HTTP/1.1` request with `Host: 127.0.0.1`, set a 500 ms read timeout and `read_to_end`: the result is `Ok` and starts with `HTTP/1.1 503`. Join the tricklers, drop the held connections, `stop(running)`. Fails today (six sequential 300 ms drains delay the probe about 1.8 s). One trickler is not enough: a single 300 ms drain still fits under 500 ms.
5. `commands/status/web/tests/config_api.rs`: four literals send `Origin: http://127.0.0.1:7373` next to `Host: localhost` (`valid_headers()` at line ~36 and three inline ones at lines ~145, ~151, ~173). Under the same-origin rule they would turn into `origin not allowed` (and `a_write_without_a_csrf_token_is_refused` would stop seeing `csrf`). Change the four literals to `Origin: http://localhost`. No signature changes; assertions do not change.
6. Existing tests whose assertions change or move: the two deleted `pure.rs` tests; the moved cap test. Fixture-only edits: `connect_websocket`, `handshake_survives_a_split_head`, the `config_api.rs` Origin literals.
Contract tests to turn green: `web_server::access_contract_tests::*`.

## Proof
From `loom/`: `cargo check --lib`, `cargo test --lib web_server::`, `cargo test --lib commands::status::web::tests::`, `cargo test --test web_server_contracts`, `cargo test --test web_host`.

## Traps
- `OriginRequirement::Optional` must keep accepting an absent `Origin` on viewing routes (a same-origin `GET` carries none); only a present, mismatching one is refused.
- `authority_matches` compares with the listening port. Nothing on the loopback branch may call it or read `local.port()`: that is what would break a tunnel on another local port.
- Do not touch the remote arms of `host_allowed` and `origin_allowed`; remote and VPN access through a non-loopback bind must behave exactly as it does now.
- The planner uses the same policy: its tests send `Host: localhost` and `Host: 127.0.0.1:<port>`; both must stay accepted (`cargo test --lib plan_server::tests::socket` is a quick check).
- `socket.rs` is 393 lines: moving the cap test out first leaves room for the fixture edits.
- `reject_unavailable` runs on the accept thread: no sleeps, no blocking reads, a bounded write.
- Do not add a CORS header anywhere; the CSRF design depends on there being none.
