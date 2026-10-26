# plan-server-core / ws-hub: the duplex WebSocket, the hub, shutdown

Tier: opus (concurrency). Wave 1, parallel with `authoring` and `runtime-startup`, after
`foundation`. Read `doc/plans/briefs/feat-planner-02-server/common.md` first.

## Files you own

`loom/src/plan_server/ws/{mod.rs, hub.rs, conn.rs, tests.rs}`, `loom/src/plan_server/shutdown.rs`,
`loom/src/plan_server/shutdown_tests.rs`. The orchestrator has declared these modules in
`plan_server/mod.rs`; never edit it. A build failure naming a sibling's module is transient.
Read-only: `plan_server::ids`, `plan_server::wire::message`, `loom/src/web_server/{ws.rs, hub.rs, limits.rs}`.

## ws/hub.rs: `PlanHub`

```text
pub struct PlanHub { .. }   // Mutex<HashMap<ConnId, Entry { browser: BrowserId, public_id: PublicId, tx: mpsc::SyncSender<Arc<String>> }>>
impl PlanHub {
    pub fn new() -> Self;
    pub fn register(&self, conn: &ConnCtx) -> mpsc::Receiver<Arc<String>>;   // queue depth 64
    pub fn unregister(&self, id: ConnId);
    pub fn broadcast(&self, msg: &PlanMessage);
    pub fn send_to_conn(&self, id: ConnId, msg: &PlanMessage);
    pub fn send_to_browser(&self, browser: &BrowserId, msg: &PlanMessage);
    pub fn send_to_holder(&self, holder: Option<&BrowserId>, msg: &PlanMessage);
    pub fn on_open(&self, f: Box<dyn Fn(&ConnCtx) -> Vec<PlanMessage> + Send + Sync>);
    pub fn on_close(&self, f: Box<dyn Fn(&ConnCtx) + Send + Sync>);
    pub fn open_frames(&self, conn: &ConnCtx) -> Vec<Arc<String>>;   // runs on_open hooks in order
    pub fn closed(&self, conn: &ConnCtx);                             // runs on_close hooks
    pub fn connections_of(&self, browser: &BrowserId) -> usize;
}
```

`send_to_holder` and `send_to_browser` reach every open tab of that browser; when it has none (the
holder is away) the message is dropped, never queued: a returning tab gets current state from the
`on_open` hooks. Serialize a message once per call, then `try_send` the `Arc<String>` to each target; a full
queue drops that connection (unregister it and let its loop close the socket), as
`Broadcaster::publish` drops slow subscribers. Never hold the map lock while serializing or
while running a hook.

## ws/conn.rs: the per-connection loop

```text
pub fn run_connection(
    stream: TcpStream,
    conn: ConnCtx,
    hub: &PlanHub,
    max_inbound: usize,
    running: &AtomicBool,
    on_message: &dyn Fn(&ConnCtx, ClientMessage),
    on_bad_frame: &dyn Fn(&ConnCtx, &str),
) -> io::Result<()>;
```

- Accept with `tungstenite::accept_with_config`, `max_message_size` and `max_frame_size` =
  `max_inbound` (256 KiB from the caller), the pattern of `web_server/ws.rs::handle_with_keepalive`
  (socket read timeout 250 ms, keepalive ping every 10 s).
- Register with the hub, send `open_frames(conn)` first, then loop: drain the receiver to the
  socket; read one frame with the timeout; a text frame parses as `ClientMessage`
  (`serde_json::from_str`); a parse failure calls `on_bad_frame(conn, reason)` (the integrator
  answers with `Refused`); binary frames are refused the same way; a close or an error ends the loop.
  Exit when `running` is false.
- Always unregister and call `hub.closed(conn)` on exit (a guard struct, so a panic path also runs it).
- A message over the limit closes the connection (tungstenite reports `Capacity`); log at
  `tracing::debug`.

## ws/mod.rs

Re-exports and `pub const MAX_INBOUND_BYTES: usize = 256 * 1024;`. The upgrade gate (Origin,
cookie) is the integrator's (`plan_server/router.rs`); this module starts after admission.

## shutdown.rs

```text
pub struct ShutdownHook { pub name: &'static str, pub drain: Box<dyn FnOnce() + Send>, pub abort: Box<dyn FnOnce() + Send> }
pub struct Shutdown { .. }
impl Shutdown {
    pub fn new() -> Self;
    pub fn register(&self, hook: ShutdownHook);
    pub fn is_stopping(&self) -> bool;
    pub fn begin(&self, report: &dyn Fn(&str)) -> JoinHandle<()>;   // first signal
    pub fn abort_all(&self);                                       // second signal
}
pub fn install_signal_handler(shutdown: Arc<Shutdown>, on_first: Box<dyn Fn() + Send + Sync>, exit: fn(i32) -> !);
```

- `begin`: sets stopping, takes every registered hook, runs each `drain` on its own named thread,
  calls `report("waiting on: a, b")` once with the names still running and again each time one
  finishes ("finished: a"), and returns a handle that joins when all drains returned. A hook
  registered after `begin` runs its `drain` at once.
- `abort_all`: runs every not-yet-run `abort` (from hooks whose drain is still running too), in
  registration order, each at most once.
- `install_signal_handler`: one `ctrlc::set_handler` (the crate allows one per process; the
  `termination` feature already covers SIGTERM and SIGHUP). First call: `on_first()` then
  `begin(..)` printing through `plan_server::lifecycle::line` is the integrator's choice, so take
  the callback; second call: `abort_all()` then `exit(130)`. `exit` is a parameter so tests do not
  end the test process.

## Tests

`ws/tests.rs`: real loopback socket (skip when loopback bind fails, as
`commands/status/web/tests.rs::skip_without_loopback` does): open-frames arrive first and in hook
order; `send_to_browser` reaches both tabs of a browser and no other; a 300 KiB text frame closes
the connection; an unparseable frame calls `on_bad_frame` and keeps the connection; a full queue
drops only the slow connection. `shutdown_tests.rs`: drains run concurrently (two hooks that each
wait on a barrier both finish); `report` names the running hooks; `abort_all` runs each abort
once; a late registration drains at once.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::ws` and
`plan_server::shutdown` pass. Report any signature you had to change, with the reason.
