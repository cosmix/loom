# W1 — Rust server, asset embedding, CLI wiring, smoke script

Tier: codex `gpt-5.6-terra`, effort `xhigh`. Runs in parallel with W2 and W3 after W0 returned.
Do not run `git` at all. Do not touch `.loom/`. Do not edit `loom/maintainability-baseline.txt`
(the orchestrator reconciles it).

Read `doc/plans/PLAN-web-dashboard.md` § "Design decisions" first. W0 already added the
`tungstenite` and `httparse` dependencies, `pub mod web;` in `status.rs`, and
`loom/src/commands/status/web/model.rs` (`WebSnapshot`, `collect_snapshot`, `SnapshotSource`,
`DaemonState`). Start from those.

## Files you own (write)

- `loom/src/commands/status/web/mod.rs` — REWRITE W0's skeleton
- `loom/src/commands/status/web/http.rs`, `ws.rs`, `broadcast.rs`, `connection.rs`, `assets.rs`, `tests.rs` — new
- `loom/src/commands/status/ui/tui/mod.rs` — change `mod daemon_client;` (line 15) to `pub(crate) mod daemon_client;`; nothing else
- `loom/src/process/sandbox_probe.rs` — add ONE probe fn (below)
- `loom/src/cli/types.rs` — add ONE field to `Commands::Status` (below); the file is at 391 lines, it may reach 399
- `loom/src/cli/dispatch.rs` — change the `Commands::Status` arm (lines 229-233)
- `loom/build.rs` — add the web asset table
- `scripts/smoke-web-dashboard.sh` — new, executable

Read-only: `web/model.rs` (W0), `loom/src/commands/status/ui/tui/daemon_client.rs` (`connect`,
`subscribe`, `is_socket_disconnected`), `loom/src/commands/status/ui/tui/app.rs:117-125`
(`connect_and_subscribe`) and `:199-245` (`receive_response`, `is_read_timeout` at `:367`),
`loom/src/daemon/protocol.rs:251-272` (`Response`), `loom/src/daemon/wire.rs:90-95`
(`read_message`/`write_message`), `loom/src/fs/work_dir.rs:195,247,320,453` (`WorkDir::new`,
`initialize`, `load`, `root`), `loom/src/commands/status/data/collector.rs:352`
(`collect_status_data(&WorkDir)`), `loom/src/assets/mod.rs:1-8` (the `include!` pattern),
`loom/src/commands/status.rs:148-176` (how `execute` opens the work dir), `loom/src/process/sandbox_probe.rs`.

Size rule: every file under 400 lines, every function under 50 lines. The split below is sized
for that; if a module grows past it, split again rather than record debt.

## 1. CLI

`loom/src/cli/types.rs`, inside `Status { .. }` after the `verbose` field, exactly this (a blank
line, a doc line, the attribute, the field):

```rust

        /// Serve the live dashboard as a web page on 127.0.0.1 (PORT defaults to 7373; 0 picks a free port)
        #[arg(long, value_name = "PORT", num_args = 0..=1, default_missing_value = "7373", conflicts_with_all = ["live", "compact"])]
        web: Option<u16>,
```

`loom/src/cli/dispatch.rs:229-233` becomes:

```rust
        Commands::Status {
            live,
            compact,
            verbose,
            web,
        } => match web {
            Some(port) => status::web::execute(port),
            None => status::execute(live, compact, verbose),
        },
```

`status.rs` is NOT edited (it is at 399 lines after W0). The branch lives in the dispatch arm for
that reason.

## 2. `web/mod.rs`

```rust
//! `loom status --web`: a minimal HTTP + WebSocket server on 127.0.0.1 that serves the
//! embedded dashboard (`web/dist`) and streams `WebSnapshot` frames.

mod assets;
mod broadcast;
mod connection;
mod http;
pub mod model;
mod ws;
#[cfg(test)]
mod tests;

/// Default port for `loom status --web` without a value.
pub const DEFAULT_PORT: u16 = 7373;

/// Entry point for `loom status --web [PORT]`. Binds 127.0.0.1 only, prints the URL, serves until the process is killed.
pub fn execute(port: u16) -> anyhow::Result<()>
/// Serve on an already-bound listener. `base` is the project root (the directory holding `.loom/work`).
/// Returns when `shutdown` becomes true (checked every 50 ms between accepts). Public for tests.
pub fn serve(listener: std::net::TcpListener, base: std::path::PathBuf, shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>) -> anyhow::Result<()>
```

`execute`: `let work_dir = WorkDir::new(".")?; work_dir.load()?;` (mirrors `status::execute`),
`TcpListener::bind(("127.0.0.1", port))` with `.with_context(|| format!("failed to bind 127.0.0.1:{port}"))`,
then `println!("loom dashboard: http://127.0.0.1:{}/  (Ctrl-C to stop)", listener.local_addr()?.port())`
— the smoke script and the tests read this line, keep the format. If `assets::WEB_ASSETS.is_empty()`
print `eprintln!("warning: dashboard assets are not embedded in this binary; run`cd web && bun install && bun run build`, then rebuild loom")`.
Then `serve(listener, PathBuf::from("."), Arc::new(AtomicBool::new(false)))`.

`serve`: `listener.set_nonblocking(true)?`; `let broadcaster = broadcast::Broadcaster::spawn(base.clone(), shutdown.clone());`
loop: `accept()` → on `Ok((stream, _))`: `stream.set_nonblocking(false)?`, clone the broadcaster
handle and base, `thread::spawn(move || connection::handle(stream, &broadcaster, &base))`; on
`WouldBlock`: if `shutdown` is set, return `Ok(())`, else sleep 50 ms; on another error: log with
`tracing::warn!` and continue.

## 3. `web/http.rs` — request head parsing and responses (httparse)

```rust
/// The parsed request head: enough to route.
pub struct RequestHead { pub method: String, pub path: String /* query stripped */, pub upgrade_websocket: bool, pub origin: Option<String> }
/// Largest request head accepted; longer heads get 431.
pub const MAX_HEAD_BYTES: usize = 16 * 1024;
/// Parse a complete head (`\r\n\r\n` present). `Ok(None)` when the buffer is still partial.
pub fn parse_head(buf: &[u8]) -> anyhow::Result<Option<RequestHead>>
/// Read from the stream until the head is complete or MAX_HEAD_BYTES is exceeded (5 s read timeout).
pub fn read_head(stream: &mut TcpStream) -> anyhow::Result<RequestHead>
/// Whether an `Origin` header value is acceptable: absent, or a URL whose host is `127.0.0.1` or `localhost` (any port, http or https).
pub fn origin_allowed(origin: Option<&str>) -> bool
/// Write a full response and flush. Adds Content-Length, Content-Type, Connection: close, and the fixed security headers.
pub fn write_response(stream: &mut TcpStream, status: u16, reason: &str, content_type: &str, body: &[u8]) -> std::io::Result<()>
```

`parse_head` uses `httparse::Request::new(&mut [httparse::EMPTY_HEADER; 32])` and `.parse(buf)`;
`Status::Partial` → `Ok(None)`; `Status::Complete(_)` → build the head: `method`, `path` (cut at
the first `?`), `upgrade_websocket` = an `Upgrade` header equal (case-insensitive) to `websocket`,
`origin` = the `Origin` header as UTF-8. Reject a path that does not start with `/`.

Fixed headers on every response: `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`,
`X-Frame-Options: DENY`,
`Content-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' ws: wss:`,
`Connection: close`.

## 4. `web/assets.rs` — the embedded `web/dist`

```rust
/// (path relative to web/dist, bytes)
pub type WebAsset = (&'static str, &'static [u8]);
include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));   // defines `pub const WEB_ASSETS: &[WebAsset]`
/// Look up an asset by request path (`/assets/index.js` -> key `assets/index.js`; `/` -> `index.html`).
pub fn lookup(path: &str) -> Option<(&'static [u8], &'static str)>   // (bytes, mime)
/// The SPA entry document, if embedded.
pub fn index_html() -> Option<&'static [u8]>
fn mime_for(key: &str) -> &'static str
```

MIME by extension: `html` → `text/html; charset=utf-8`, `js` → `text/javascript; charset=utf-8`,
`css` → `text/css; charset=utf-8`, `svg` → `image/svg+xml`, `woff2` → `font/woff2`, `woff` →
`font/woff`, `png` → `image/png`, `ico` → `image/x-icon`, `json` → `application/json`, `txt` →
`text/plain; charset=utf-8`, `map` → `application/json`, anything else `application/octet-stream`.
`lookup` is a linear scan of `WEB_ASSETS` (about forty entries).

## 5. `loom/build.rs` — write `$OUT_DIR/web_assets.rs`

Add `const WEB_DIST_ROOT: &str = "web/dist";` next to the other roots (NOT inside `ASSET_ROOTS`:
those are UTF-8 validated and installed; this one is binary and embedded only). In `main()` after
`generate_embedded_assets(&repo_root)` call `generate_web_assets(&repo_root)`:

- `let dist = repo_root.join(WEB_DIST_ROOT);`
- `emit_if_exists(&dist)` (a directory path: cargo rescans it recursively).
- If `dist.join("index.html").exists()`: collect `walk_files(&dist)` MINUS the `validate_utf8`
  call (write a `walk_files_binary` sibling that does not validate; do not weaken `walk_files`),
  sort by `asset_key`, and write
  `pub const WEB_ASSETS: &[WebAsset] = &[\n    ("index.html", include_bytes!("<abs>")),\n ...];\n`
  using the existing `asset_key`, `absolute_path` and `rust_literal` helpers.
- Else write `pub const WEB_ASSETS: &[WebAsset] = &[];\n` and
  `println!("cargo:warning=web/dist is missing;`loom status --web`will answer 503. Build it with: cd web && bun install && bun run build");`.
- Write to `PathBuf::from(env OUT_DIR).join("web_assets.rs")`, panicking on failure like
  `generate_embedded_assets` does.

Keep every function under 50 lines. `build.rs` is not scanned by the maintainability test, but
match its style.

## 6. `web/broadcast.rs` — one daemon subscription per server

```rust
/// Fans `WebSnapshot` JSON frames out to every WebSocket client.
#[derive(Clone)]
pub struct Broadcaster { inner: Arc<Inner> }
struct Inner { latest: Mutex<Option<Arc<String>>>, subscribers: Mutex<Vec<mpsc::Sender<Arc<String>>>> }
impl Broadcaster {
    /// Start the background producer thread. `base` is the project root.
    pub fn spawn(base: PathBuf, shutdown: Arc<AtomicBool>) -> Self
    /// Subscribe; the latest frame (if any) is queued on the new receiver immediately.
    pub fn subscribe(&self) -> mpsc::Receiver<Arc<String>>
    /// The last published frame.
    pub fn latest(&self) -> Option<Arc<String>>
    fn publish(&self, json: String)     // store as latest, send to all, drop senders that fail
}
```

Producer thread (`fn run(this: Broadcaster, base: PathBuf, shutdown: Arc<AtomicBool>)`):

```text
let work_dir = WorkDir::new(&base) (then load()); let work_path = work_dir.root().to_path_buf();
loop until shutdown:
  match daemon_session(&work_path) {            // connect + subscribe, then set_read_timeout(1 s)
    Ok(stream) => forward_daemon(&this, stream, &work_path, &shutdown),   // returns on disconnect/error
    Err(_)     => poll_files(&this, &work_dir, &work_path, &shutdown),     // 5 polls, 2 s apart, then return
  }
```

`daemon_session` reuses `daemon_client::connect(&work_path.join("orchestrator.sock"))` and
`daemon_client::subscribe(&mut stream)` — the same calls as `app.rs:117-125`.

`forward_daemon`: loop `read_message::<Response, _>(&mut stream)`: `Ok(Response::StatusUpdate { data })`
→ `publish(serde_json::to_string(&collect_snapshot(work_path, data, SnapshotSource::Daemon))?)`;
`Ok(_)` → continue; `Err(e)` when `is_socket_disconnected(&e)` → return; `Err(e)` when the error
chain holds an `io::Error` of kind `WouldBlock` or `TimedOut` → continue (write your own
`is_read_timeout` in this module: `app.rs:367` is private); any other `Err` → return. Check
`shutdown` each iteration.

`poll_files`: up to 5 times: `collect_status_data(work_dir)` → `collect_snapshot(work_path, data, SnapshotSource::Files)`
→ serialize → publish only if it differs from `latest()`; sleep 2 s (in 100 ms slices, checking
`shutdown`). A collector error is logged with `tracing::warn!` and the loop continues.

`publish`: compare with `latest`; if equal, return (no send); else store and send an `Arc` clone
to every sender, retaining only the senders whose `send` succeeded.

## 7. `web/ws.rs` — one thread per WebSocket client

```rust
/// Complete the handshake on a stream whose head was only peeked, then stream frames until the client leaves.
pub fn handle(stream: TcpStream, rx: mpsc::Receiver<Arc<String>>)
```

`tungstenite::accept(stream)` (call it fully qualified exactly like that; a wiring check greps
for `tungstenite::accept(` in this file) → on `Err(_)` log and return. THEN
`ws.get_mut().set_read_timeout(Some(Duration::from_millis(250)))` (never before the handshake: a
timeout during it surfaces as `HandshakeError::Interrupted`). Loop:

```text
while let Ok(frame) = rx.try_recv() { ws.send(Message::text(frame.as_str()))?  -> on Err break }
match ws.read() {
  Ok(Message::Close(_)) => break,
  Ok(_) => {}                                           // pings are answered by tungstenite on the next read/write
  Err(Error::ConnectionClosed | Error::AlreadyClosed) => break,
  Err(Error::Io(e)) if matches!(e.kind(), WouldBlock | TimedOut) => {}
  Err(_) => break,
}
```

A `try_recv` returning `Disconnected` (the broadcaster is gone) breaks as well. Never call
`ws.close` from a `send` error path (the socket is already broken); just drop.

## 8. `web/connection.rs` — routing

```rust
/// Handle one accepted connection to completion.
pub fn handle(stream: TcpStream, broadcaster: &Broadcaster, base: &Path)
```

1. `stream.set_read_timeout(Some(5 s))`; `peek` up to 8 KiB into a buffer; `http::parse_head`
   on it. `Ok(None)` (partial) → fall through to step 3 with `read_head`.
2. If `head.path == "/ws"` and `head.upgrade_websocket`: if `!origin_allowed(head.origin)` →
   `write_response(403, "Forbidden", "text/plain; charset=utf-8", b"origin not allowed")` and
   return; else clear the read timeout (`set_read_timeout(None)`) and
   `ws::handle(stream, broadcaster.subscribe())` — the peeked bytes are still in the socket for
   tungstenite to read. Return.
3. `let head = http::read_head(&mut stream)` (consumes). Method not `GET`/`HEAD` → 405.
4. Route:
   - `/api/status` → origin check (403 as above); body = `broadcaster.latest()` or, when `None`,
     a fresh `collect_snapshot(work_path, collect_status_data(&WorkDir::new(base)?)?, Files)`;
     `application/json; charset=utf-8`, 200. A collector error → 500 with the error text.
   - `/` → `assets::index_html()` → 200 `text/html` or 503 `text/plain` with the "not embedded"
     sentence from `execute`.
   - `assets::lookup(path)` hit → 200 with its mime.
   - path starts with `/assets/` or `/api/` and missed → 404.
   - anything else (`/stages/...`, `/legend`, ...) → the SPA fallback: `index_html()` → 200, or
     503 when not embedded.
5. Errors writing the response are ignored (client gone).

## 9. `loom/src/process/sandbox_probe.rs` — one probe

```rust
/// Whether this process may bind a TCP listener on the loopback interface.
pub fn loopback_bindable() -> bool { std::net::TcpListener::bind("127.0.0.1:0").is_ok() }
```

Place it next to `unix_socket_bindable` and document it in the module docs list the same way.

## 10. `web/tests.rs`

`#[cfg(test)]`-only module. A helper `fn workspace() -> (tempfile::TempDir, PathBuf /*base*/)`
that creates a temp dir and `WorkDir::new(&base)?.initialize()?` (see `work_dir.rs:247`; if
`load()` requires a config file, write the minimal one `initialize` expects — read
`work_dir.rs:247-330` to see what `load` needs and produce it). A helper
`fn start(base: PathBuf) -> (u16, Arc<AtomicBool>)` that binds `127.0.0.1:0`, spawns `serve` on
a thread, and returns the port + shutdown flag; tests set the flag at the end.

Tests (REQUIRED NAMES marked):

- `parse_head_reads_method_path_and_upgrade` — `GET /ws?x=1 HTTP/1.1\r\nHost: a\r\nUpgrade: websocket\r\nOrigin: http://127.0.0.1:7373\r\n\r\n` → method GET, path `/ws`, upgrade true, origin Some.
- `parse_head_returns_none_on_partial` and `read_head_rejects_oversized`.
- `origin_allowed_accepts_loopback_and_rejects_foreign` — None, `http://127.0.0.1:5173`, `http://localhost:7373`, `https://localhost` → true; `http://evil.example`, `http://127.0.0.1.evil.example`, `null` → false.
- `mime_for_known_extensions`.
- `route_prefers_assets_then_api_then_spa_fallback` — pure routing over the path rules in §8 (extract the routing decision into a small `enum Route` + `fn route(head) -> Route` in `connection.rs` so it is testable without sockets).
- `api_status_returns_snapshot_json` — `skip_unless(loopback_bindable(), ..)`; GET `/api/status` over a raw `TcpStream`; status 200; body parses with `serde_json::from_str::<WebSnapshot>`; `source == Files`; `daemon == NotRunning`.
- `api_status_rejects_foreign_origin` — same request with `Origin: http://evil.example` → 403.
- `index_serves_embedded_page` — `skip_unless(loopback_bindable() && !assets::WEB_ASSETS.is_empty(), ..)`; GET `/` → 200, `text/html`, body contains `<div id="root">`; GET `/stages/anything` → 200 same body; GET `/assets/nope.js` → 404.
- `index_reports_missing_assets` — `skip_unless(loopback_bindable() && assets::WEB_ASSETS.is_empty(), ..)`; GET `/` → 503.
- `websocket_delivers_a_snapshot` — REQUIRED NAME. `skip_unless(loopback_bindable(), ..)`. Connect with tungstenite's client: build the request with `"ws://127.0.0.1:{port}/ws".into_client_request()` (read `tungstenite-0.30.0/src/client.rs:194-260` for the `IntoClientRequest` impl available without the `url` feature; if `&str` needs it, build an `http::Request` by hand with `Host`, `Connection: Upgrade`, `Upgrade: websocket`, `Sec-WebSocket-Version: 13`, `Sec-WebSocket-Key`) and `tungstenite::client(request, TcpStream::connect(..)?)`; `read()` the first frame with a 5 s read timeout on the stream; it is `Message::Text` and parses as `WebSnapshot`; then `close(None)`.
- `websocket_rejects_foreign_origin` — raw handshake bytes with `Origin: http://evil.example` → the status line starts with `HTTP/1.1 403`.
- `broadcaster_publishes_file_snapshot_without_daemon` — `Broadcaster::spawn(base, flag)`, `subscribe()`, `recv_timeout(5 s)` yields a frame whose JSON has `"source":"files"`.

Use `crate::process::sandbox_probe::skip_unless` exactly like the existing callers (grep
`skip_unless(` under `loom/src` for the call shape).

## 11. `scripts/smoke-web-dashboard.sh`

```bash
#!/usr/bin/env bash
# Smoke test for `loom status --web`: starts the server on a free port and checks the
# embedded page, the JSON snapshot, the SPA fallback and a 404. Usage: smoke-web-dashboard.sh <loom-binary>
set -euo pipefail
bin=${1:?usage: $0 <loom-binary>}
out=$(mktemp "${TMPDIR:-/tmp}/loom-web-smoke.XXXXXX")
"$bin" status --web 0 >"$out" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true; rm -f "$out"' EXIT
port=""
for _ in $(seq 1 100); do
  port=$(rg -o 'http://127\.0\.0\.1:[0-9]+' "$out" | head -1 | rg -o '[0-9]+$' || true)
  [ -n "$port" ] && break
  sleep 0.1
done
[ -n "$port" ] || { echo "server did not print its URL:"; cat "$out"; exit 1; }
base="http://127.0.0.1:$port"
curl -fsS "$base/" | rg -q '<div id="root">'
curl -fsS "$base/api/status" | jq -e '.status.stages | type == "array"' >/dev/null
curl -fsS "$base/stages/anything" | rg -q '<div id="root">'
curl -fsS -o /dev/null -w '%{content_type}\n' "$base/assets/index.js" | rg -q '^text/javascript'
code=$(curl -s -o /dev/null -w '%{http_code}' "$base/assets/nope.js")
[ "$code" = "404" ] || { echo "expected 404 for a missing asset, got $code"; exit 1; }
code=$(curl -s -o /dev/null -w '%{http_code}' -H 'Origin: http://evil.example' "$base/api/status")
[ "$code" = "403" ] || { echo "expected 403 for a foreign origin, got $code"; exit 1; }
echo "smoke-web-dashboard: ok on port $port"
```

`chmod +x scripts/smoke-web-dashboard.sh`. It must run from the repository root of a checkout
whose `.loom/work` exists (a stage worktree has it).

## Done means

- Your one check: `cargo test --manifest-path loom/Cargo.toml --lib commands::status::web::`
  (W0's model tests plus yours). `index_serves_embedded_page` prints SKIP until the orchestrator
  builds `web/dist`; that is expected. Nothing else: the orchestrator runs fmt, clippy, doc, the
  maintainability gate and the smoke script.
- Report: files changed, the test summary line, any deviation from the module split, and any
  place the brief's signatures did not fit reality (say what you did instead).
