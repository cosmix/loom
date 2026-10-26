# web-server-extract / extract: move the dashboard's server parts into `loom::web_server`

Tier: opus (cross-cutting refactor). Read `doc/plans/briefs/plan-server/common.md` first.

## Goal

The dashboard (`loom status --web`) and the coming planning server (`loom plan`) differ only in
the router they pass to one accept loop. After this worker, `loom/src/web_server/` holds the
shared layer, `loom/src/commands/status/web/` holds only dashboard code, and the dashboard behaves
byte-for-byte as before.

## Read first (anchor by symbol; lines are from b0f91c00, re-confirm)

- `loom/src/commands/status/web/mod.rs`: `execute`, `serve`, `serve_with` (accept loop,
  `drain_connections`), `print_startup` and helpers, `DEFAULT_PORT` (:110), `ServeOptions`,
  `TerminalLane`, the `ctrlc::set_handler` call (:161).
- `unserved.rs` `spawn_connection` (calls `connection::handle` at :45), `connection.rs` `handle`
  (:79), `gate` (:136), `route` (:335), `handle_route` (:223), `Route` (:59), `index_response`.
- `bootstrap.rs` `route_upgrade`, `bootstrap_terminal_token`; `ws.rs` `handle`,
  `handle_with_keepalive`; `broadcast.rs` `Broadcaster` (`Inner`, `subscribe`, `publish`,
  `latest`, producer `run`); `access.rs` `AccessPolicy::resolve`; `auth.rs` `Auth::new`;
  `terminal/token.rs` (`mint`, `matches`, `cookie_name`, `cookie_token`, `query_token`);
  `limits.rs`; `listener.rs`; `http.rs` (`RequestHead`, `response_head`, CSP const);
  `head.rs`; `interfaces.rs`; `assets.rs`.
- Importers outside the module: `loom/src/cli/dispatch.rs:322`
  (`status::web::execute(...)`), `loom/tests/e2e/web_terminal.rs:18`
  (`use loom::commands::status::web::{serve, ServeOptions};`), `loom/tests/web_host.rs` (binary).

## What moves to `loom/src/web_server/` (add `pub mod web_server;` to `loom/src/lib.rs`)

`git mv` each file so history follows: `listener.rs`, `access.rs` + `access_tests.rs`, `auth.rs`,
`terminal/token.rs` -> `web_server/token.rs`, `head.rs`, `http.rs`, `ws.rs`, `limits.rs`,
`unserved.rs`, `interfaces.rs`, `assets.rs`. `connection.rs` and `bootstrap.rs` split: the generic
part (timeouts, head peek, Host check, the `?token=` cookie bootstrap parameterised by cookie
specs, `fail`, `respond`, `drain_pending`, `reject_unavailable`) moves; the dashboard's `Route`,
`route()`, `handle_route`, `serve_api`, config and history arms, and the `/ws` and terminal
upgrade arms stay in `commands/status/web/` as the dashboard router.

## Shapes to produce (binding; later plans call them)

- `pub trait Router: Send + Sync + 'static { fn upgrade(&self, stream: TcpStream, peeked: &RequestHead, conn: &ConnInfo<'_>) -> Option<TcpStream>; fn handle(&self, stream: &mut TcpStream, head: &RequestHead, body_prefix: Vec<u8>, conn: &ConnInfo<'_>) -> io::Result<()>; }`
- `pub struct ConnInfo<'a> { pub local: SocketAddr, pub policy: &'a AccessPolicy, pub limits: &'a Arc<Limits>, pub running: &'a AtomicBool }`
- `pub fn serve(listener: TcpListener, router: Arc<dyn Router>, running: Arc<AtomicBool>, config: ServeConfig) -> Result<()>`
  with `pub struct ServeConfig { pub policy: AccessPolicy, pub limits: Arc<Limits>, pub thread_name: &'static str }`
  in `web_server/serve.rs`. Order inside each connection thread is today's: timeouts, peek head,
  `host_allowed` (403 "host not allowed") BEFORE the router sees anything, then
  `router.upgrade`, then `http::read_head` (400 on failure), then `router.handle`. Connection
  slots, the 503 when full, the 3 s drain on stop: unchanged.
- `pub struct Hub` in `web_server/hub.rs`: the subscriber fan-out lifted out of `Broadcaster`'s
  `Inner` (`new(queue_depth)`, `subscribe() -> mpsc::Receiver<Arc<String>>` primed with the latest
  frame, `publish(String)` dropping a subscriber whose queue is full, `latest()`). The dashboard's
  "suppress unchanged body ignoring the timestamp" stays in `Broadcaster`, which now owns a `Hub`.
- `pub fn bind_listener(host: IpAddr, port: Option<u16>, default_port: u16) -> Result<TcpListener>`;
  `bind_first_available_port` becomes `pub`. The dashboard passes `DEFAULT_PORT` (7373).
- `AccessPolicy::new(local: SocketAddr, auth: Option<Auth>) -> Result<Self>` (fails closed on a
  non-loopback bind without auth, as `resolve` does); the dashboard computes its `Auth` from
  `ServeOptions` before calling it. `Auth::new(token: String, cookie_name: String)`; the
  dashboard passes `token::cookie_name(port)` (`loom_dashboard_<port>`).
- `Limits::new(max_inbound_bytes: usize) -> Arc<Limits>`; `MAX_INBOUND_BYTES` (64 KiB) stays the
  dashboard's value; `ws::handle` reads the limit from `Limits` instead of the const.
- `response_head` gains `extra_headers: &[(&str, &str)]`, threaded through `response_bytes`,
  `write_response`, `write_redirect` as a trailing argument (the dashboard passes `&[]`). CSP and
  every existing header unchanged.
- `assets.rs` keeps the one `include!` of `OUT_DIR/web_assets.rs`; add
  `pub fn page(name: &str) -> Option<&'static [u8]>` (exact key lookup, e.g. `"index.html"`), keep
  `lookup`, `index_html`, `mime_for`.
- Visibility: what the dashboard and the planning server both call is `pub(crate)` or `pub`;
  `token::*` become `pub(crate)`.
- Dashboard: `struct DashboardRouter { broadcaster, base, lane, ... }` in
  `commands/status/web/router.rs` implementing `Router`; `serve(listener, base, running, options)`
  and `ServeOptions` keep their public signatures (the e2e test imports them unchanged);
  `execute` keeps loading `WorkDir` before binding and keeps the `ctrlc` handler.
- Add `pub fn startup_lines(local: SocketAddr, remote: bool, terminals: bool, token: Option<&str>, addresses: &[IpAddr]) -> Vec<String>`
  in `commands/status/web/startup.rs` returning exactly the lines `print_startup` prints today
  (for a wildcard bind it calls the existing `wildcard_bootstrap_lines`, which keeps its name and
  signature, because `commands/status/web/tests/pure.rs:166` asserts on it); `print_startup` prints
  them. Text unchanged character for character.
- Tests: the dashboard test files keep their assertions; only `use` lines change. The one test
  file that moves is `access_tests.rs` (with `access.rs`, to `web_server/access_tests.rs`): git
  sees a deleted base test file, so completion raises `TI-edit-loom/src/commands/status/web/access_tests.rs`.
  That event is expected; report it to the orchestrator, who disputes it. Any other changed
  assertion is a defect.

## Traps

- `ctrlc` allows one handler per process; it stays in the dashboard's `execute`, never in
  `web_server`.
- The terminal lane (`terminal/`) depends on `WorkDir` and stays in the dashboard; only
  `token.rs` leaves it.
- `http::RequestHead::csrf_token` is read only by `config_api`; keep the field (moving it is
  churn), but no shared code may depend on it.
- Keep every file under 400 lines; `mod.rs` (352) must shrink, not grow.

## Done when

`cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo test --lib web_server::`,
`cargo test --lib commands::status::web::` (211 tests passed at b0f91c00 in the author's
checkout), `cargo test --test e2e web_terminal`, `cargo test --test web_host` all pass with the
module's tests edited only in their `use` paths. Report the final file list with line counts.
