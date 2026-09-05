# Web Dashboard

> `loom status --web [PORT]` — a read-only HTTP/WebSocket server (port 7373 default, `127.0.0.1` only) that serves an embedded React SPA and streams the same `StatusData` payload the live TUI renders. Module: `loom/src/commands/status/web/`; frontend: `web/`.

## Server

Hand-rolled HTTP/1.1 + WebSocket server, no framework: `loom/src/commands/status/web/mod.rs` (entry, `WorkDir::new(".")?.load()?` — needs `.loom/work` in the CWD or it exits before binding), `loom/src/commands/status/web/connection.rs` (routing, Host-header DNS-rebinding gate, CSP/security headers, `WRITE_TIMEOUT=5s` for whole-bundle HTTP writes vs the WebSocket lane's 250ms, `MAX_CONNECTIONS=64`/`MAX_WEBSOCKETS=48` sub-cap with an RAII slot guard and `DRAIN_DEADLINE=300ms` wall-clock bound on `drain_pending` for the accept loop), `loom/src/commands/status/web/broadcast.rs` (daemon-fed publish/subscribe), `loom/src/commands/status/web/ws.rs` (frame send/receive), `loom/src/commands/status/web/model.rs` (wire types, pinned by `loom/src/commands/status/web/model_tests.rs`).

It reuses existing TUI/status code rather than reimplementing it: `render::attention_entries`, `render::failure_label`, `scheduling_report::alerts`, `tick::read`, `data::collect_status_data`, `daemon_client::{connect,subscribe,is_socket_disconnected}`, `DaemonServer::check_status`.

## Broadcaster fallback

Daemon lane first; a `Response::Error` degrades to the file-poll lane with the message carried into `snapshot.notice`. A received frame resets the failure counter so a healthy daemon never falls back. After `FILE_POLL_COUNT` polls the loop retries the daemon (~10s reconnect cycle). Lock order is `last_body -> subscribers -> latest` in `publish`, and `subscribe` takes `subscribers` before reading `latest` (a suffix of that order), so no deadlock; dedup compares the body ignoring only `generated_at`, so a daemon-to-files handover always publishes once.

## Fixture contract

`loom/src/commands/status/web/model_tests.rs` pins the Rust model's exact serde output against `web/src/api/fixtures/statuses.json`, which the TS `zod` schema (`web/src/api/schema.ts`) and tests consume. Changing the wire model means updating the fixture and schema in the same change, or the TS tests and the zod parse both fail. The one gap: the fixture test hardcodes the 13 `StageStatus` variants instead of deriving them from an exhaustive match, so a 14th variant compiles Rust-side and only fails at runtime on the TS side (zod rejects the whole snapshot, page renders blank).

## Committed dist + build.rs embedding

`web/dist` is a committed build output, embedded via `include_bytes!` at compile time: `loom/build/assets.rs`'s `generate_web_assets` (called from `loom/build.rs`) walks `web/dist` and writes a generated Rust source file under Cargo's `OUT_DIR` (not checked in) that the crate `include!`s. It compares generated content before writing (`write_if_changed`) — writing unconditionally would make the file's fresh mtime trigger a full crate rebuild on every `cargo build`. Rebuild and commit `dist/` with every change to `web/src/` (`cd web && bun install && bun run build`).

## CI/scripts

Any script invoking `loom status --web` needs its own `.loom/work` in the CWD — `WorkDir::new(".")` bails before binding otherwise, and a clean CI checkout never has one since `.loom/work` is gitignored (`scripts/smoke-web-dashboard.sh` creates a scratch workspace for this reason).
