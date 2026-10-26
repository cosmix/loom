# plan-server-core / integrator: context, gate, router, routes, CLI, run loop

Tier: opus (Claude). Wave 2: starts after codex units C1-C16 and the Claude `authoring` and
`ws-hub` workers have returned. `server_info/mod.rs` (C17) and `loom/tests/plan_server_cli.rs`
(C18) are codex units written after you from the sections below; you declare
`pub mod server_info;` and register `server_info::routes()` in `routes.rs`, and `PlanServeArgs` in
`cli/types_ops.rs` is yours (below). Read `doc/plans/briefs/plan-server/common.md` first, then the other four briefs of this
stage (`doc/plans/briefs/plan-server/plan-server-core/*.md`) for the shapes they produced, then
the code they wrote. Spec: `doc/product/section_12.md` "Access to the server" (all four
subsections), "People and authoring", "Request limits"; `doc/product/section_02.md` "Access",
"Lifetime", "Without `.loom/work`".

## Files you own

`loom/src/plan_server/mod.rs` (take it over: module list, `DEFAULT_PORT`, `start`, `run`),
`loom/src/plan_server/{context.rs, extensions.rs, gate.rs, gate_tests.rs, router.rs, routes.rs, request.rs, watch.rs}`,
`loom/src/plan_server/ws/dispatch.rs`, `loom/src/plan_server/tests/{mod.rs, socket.rs, support.rs}`,
`loom/src/commands/plan/serve.rs`, `loom/src/commands/plan/mod.rs` (one `pub mod serve;` line),
`loom/src/cli/types_ops.rs` (`PlanServeArgs`), `loom/src/cli/types.rs` (the `Commands::Plan` variant only),
`loom/src/cli/dispatch.rs` (`dispatch_plan` and the one `Commands::Plan` arm only).

## Context (`context.rs`, `extensions.rs`)

```text
pub struct PlanContext { root, runtime: RuntimePaths, project: RwLock<Project>, authoring: Authoring, presence: Presence,
                         hub: PlanHub, shutdown: Arc<Shutdown>, read_only: ReadOnly, bind: BindInfo, token: String,
                         batch_hooks: RwLock<Vec<Box<dyn Fn(&PlanContext, &Batch) + Send + Sync>>>, extensions: Extensions }
pub struct BindInfo { pub local: SocketAddr, pub remote: bool, pub urls: Vec<String> }
impl PlanContext {
    pub fn root(&self) -> &Path; pub fn runtime(&self) -> &RuntimePaths;
    pub fn project(&self) -> RwLockReadGuard<'_, Project>;
    pub fn authoring(&self) -> &Authoring; pub fn presence(&self) -> &Presence; pub fn hub(&self) -> &PlanHub;
    pub fn shutdown(&self) -> &Shutdown; pub fn read_only(&self) -> &ReadOnly; pub fn bind(&self) -> &BindInfo;
    pub fn is_stopping(&self) -> bool;
    pub fn require_authoring(&self, browser: &BrowserId, take: Take) -> Result<(), Refusal>;   // wraps Authoring::require with stopping and names
    pub fn on_batch(&self, f: Box<dyn Fn(&PlanContext, &Batch) + Send + Sync>);
    pub fn own_writes(&self) -> &OwnWrites;   // the watcher's OwnWrites; plan 4's SummaryEngine registers its writes here
    pub fn extensions(&self) -> &Extensions;
}
pub struct Extensions;  // RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>: insert<T>(Arc<T>), get<T>() -> Option<Arc<T>>
```

`Project`, `Batch`, `Watcher`, `RuntimePaths`, `runtime_paths` are plan 1's
(`loom::planning::{project::Project, watch::{Batch, Watcher, OwnWrites}, runtime::{RuntimePaths, server_runtime_paths}}`).
Read their committed code before use; do not reimplement any of it.

## watch.rs

`pub fn spawn_watcher(ctx: Arc<PlanContext>, running: Arc<AtomicBool>) -> JoinHandle<()>`:
`Watcher::new(root, OwnWrites::default()).spawn(Duration::from_secs(1), stop, |batch| { ... })`;
per batch: skip `initial` (the context loaded the project at start), write-lock and
`Project::refresh(&batch)` (log an error with `tracing::warn`, keep serving), then run every
`on_batch` hook in order, then broadcast `PlanMessage::Checklist` when the checklist changed.
The `OwnWrites` handed to `Watcher::new` is the one `ctx.own_writes()` returns (a clone of one
shared value), so plan 4's engine registrations reach the watcher.

## Gate (`gate.rs`) and router (`router.rs`)

`PlanRouter { ctx: Arc<PlanContext>, routes: Vec<RouteSpec> }` implements `web_server::Router`.

- Static: `GET`/`HEAD` of `/assets/<key>` -> `web_server::assets::lookup`; `/` and every path not
  under `/api/`, `/assets/`, `/ws` -> `assets::page("plan.html")`, or 503 with
  `"planning assets are not embedded; build web/dist and rebuild loom"` when absent. No cookie
  needed. `GET /?token=<t>`: right token -> 302 to `/` with two `Set-Cookie` headers
  (`loom_plan_<port>=<token>` and, when the request has no valid browser cookie,
  `loom_plan_browser_<port>=<BrowserId::mint()>`), each `Path=/; HttpOnly; SameSite=Strict`, no
  Max-Age; wrong token 403. `?token=` on any other path is ignored (served as the shell, no cookie).
  Reuse `web_server::bootstrap`'s cookie bootstrap; extend it to set two cookies if it sets one.
- `/api/*` and `/ws`: run section 12's checks in order (common brief), as one function
  `pub fn admit(head: &RequestHead, local: SocketAddr, policy: &AccessPolicy, ctx: &PlanContext, spec: Option<&RouteSpec>) -> Result<Admitted, Refusal401or403>`.
  `Origin` must equal `http://{Host header}` byte for byte for POST and WS in every posture: use
  `AccessPolicy::origin_allowed(local, head, OriginRequirement::Required)` only if its loopback branch
  is exact; otherwise write the exact comparison here (the dashboard's loopback rule accepts any
  loopback alias, which section 12 forbids for this server).
  A valid access cookie but no or invalid browser cookie: HTTP -> mint one and attach `Set-Cookie`
  to this response; `/ws` -> 401 (the app calls `GET /api/server` first).
- Unknown `/api/*` path after the cookie check -> 404 JSON.
- Every response adds `Referrer-Policy: no-referrer` through `extra_headers`.
- `/ws`: after admission, `PlanContext` creates `ConnCtx { id: next ConnId, browser, public_id: presence.public_id(&browser) }`,
  takes a `Lane::WebSocket` slot, runs `ws::conn::run_connection` with `MAX_INBOUND_BYTES`,
  `ws::dispatch::dispatch` as `on_message`, and `Refused { command: "frame", reason }` for bad frames.
- Hub hooks installed at start: on open -> `Presence::opened`, `Authoring::connection_opened`, and the
  `Hello` frame; on close -> the reverse. Authoring events and presence changes broadcast
  `PlanMessage::Authoring` / `PlanMessage::Presence`.

## Routes (`routes.rs`, `request.rs`)

```text
pub enum Method { Get, Post }
pub enum BodyKind { None, Json, Text { max: usize } }   // Json max = web_server::http::MAX_BODY_BYTES (8 KiB)
pub enum Role { Viewer, Holder, HolderOrTake }
pub struct RouteSpec { pub method: Method, pub path: &'static str, pub body: BodyKind, pub role: Role, pub handler: fn(&ApiRequest<'_>) -> ApiResponse }
pub fn all_routes() -> Vec<RouteSpec>;         // concatenates each module's routes(); one line per module
pub fn install_all(ctx: &Arc<PlanContext>);    // one line per module that has install()
```

`path` is exact unless it ends in `/` (prefix). The gate reads `Content-Length` and refuses over
the limit (413) before reading; `request.rs` reads the body with `web_server::http::read_body`.
`ApiRequest { ctx, head, browser: &BrowserId, public_id: &PublicId /* presence.public_id(browser) */, query: QueryParams, body }` with `json<T>()`, `text()`;
`QueryParams::parse(query)` percent-decodes (`%XX` and `+`), refuses a value over 1024 bytes (400).
`ApiResponse::{json(status, &T), error(status, code, message), text(status, mime, body)}`; error
bodies are `{"error": code, "message": message}`. Role `Holder`/`HolderOrTake` call
`ctx.require_authoring` and answer 403 with the `Refusal` text.

## server_info/mod.rs

`routes()`: `GET /api/server` -> `ServerInfo`; `GET /api/share` -> `ShareLink` (url = first bind
URL; `bind` from `BindInfo.remote`; `tunnel` on loopback = `startup::tunnel_hint(port, &hostname)` with `nix::unistd::gethostname`). `install(ctx)`: nothing beyond the hub hooks above.

## ws/dispatch.rs

`pub fn dispatch(ctx: &PlanContext, conn: &ConnCtx, msg: ClientMessage)` runs on the
connection's own thread (document this): a handler must return quickly and hand long work to its
own thread. A frame that does not parse as `ClientMessage` never reaches `dispatch`; the router
answers it with `Refused { command: "frame", reason }` automatically. `SetName` ->
`presence.set_name` + broadcast presence; `Reading` -> `presence.set_reading` + broadcast. One arm
per variant; later plans add arms.

## mod.rs: `start` and `run`

```text
pub struct ServerOptions { pub root: PathBuf, pub host: IpAddr, pub port: Option<u16>, pub read_only: ReadOnly }
pub struct RunningServer { pub local: SocketAddr, pub token: String, pub ctx: Arc<PlanContext>, .. }
// `start`, `ServerOptions`, `RunningServer` and `prereq::ReadOnly::from_problems` are `pub` and reachable
// from integration-test targets (`loom::plan_server::start`): plans 8 and 11's operator tests start a
// server with them, bootstrap with `GET /?token=<RunningServer.token>`, and send
// `Origin: http://127.0.0.1:<port>` on every write and WebSocket. Nothing they need is `#[cfg(test)]`.
impl RunningServer { pub fn stop(self) -> Result<()>; }
pub fn start(opts: ServerOptions) -> Result<RunningServer>;   // bind, mint token, load Project, build context, spawn watcher + 1 s tick (Authoring::tick) + web_server::serve on threads
pub fn run(args: &PlanServeArgs) -> Result<()>;
```

`start` runs `routes::install_all(&ctx)` after the context is built and BEFORE the accept loop
and the watcher start, so every later plan's `install` (startup recovery included) has run before
the first request. `start` touches neither the lock nor the runtime directory (tests call it on
temp repos). `run`:
`root::resolve_project(cwd)` -> `planning::runtime::server_runtime_paths(&root)` (never
`runtime_paths`: the server must not honour `LOOM_PLAN_STATE`, a variable only plan 7's session
launch sets; when the returned ignored value is `Some`, print to stderr
`warning: LOOM_PLAN_STATE is set in this shell and is ignored by the server`) -> create `dir` mode 0700 (refuse a
symlinked or non-directory `dir`) -> `lock::acquire(&paths.lock)` (`Running` -> print
`already_running_lines` to stderr, exit status 1 via an error the dispatcher maps) -> `start` ->
`ServerLock::publish(url, token)` -> print `startup_lines` -> `browser::decide` / `open(url)` ->
`shutdown::install_signal_handler` (first signal: broadcast `ServerStopping`, `Shutdown::begin`
reporting through `lifecycle::line`; the accept loop, every WebSocket and every HTTP route stay up
while the drains run, so the holder can still answer a session's prompts; routes and commands
check `ctx.is_stopping()` themselves; when the last drain returns, stop the server and return Ok) ->
block until stopped. `web_server::bind_listener(host, port, DEFAULT_PORT)`; the port in every
URL is the bound one; a remote bind prints the warning and sets `ServerInfo.remote`.

## CLI

`Commands::Plan { #[command(subcommand)] command: Option<PlanCommands>, #[command(flatten)] serve: PlanServeArgs }`
with `#[command(args_conflicts_with_subcommands = true)]`; `PlanServeArgs { #[arg(long)] port: Option<u16>, #[arg(long, value_parser = parse_web_host, default_value = "127.0.0.1")] host: IpAddr, #[arg(long)] no_open: bool }`
(`parse_web_host`, `loom/src/cli/types_status_web.rs:40`). `dispatch_plan(command, serve)`:
`None` -> `commands::plan::serve::execute(&serve)`. Keep the top-level `dispatch` match line
count unchanged (its function row in `loom/maintainability-baseline.txt` is exact).

## Tests (`plan_server/tests/`, `gate_tests.rs`)

Real loopback socket through `start` on a temp git repo with a minimal `doc/`: the bootstrap
sets both cookies; a wrong token is 403; `/api/server` without cookie is 401 and with it 200; a
second request without a browser cookie gets a new one; `POST` to a JSON route with
`text/plain` is refused 415 and with an 8 KiB + 1 `Content-Length` 413 before the body is sent;
`/ws` without Origin, with `http://localhost:<port>` against Host `127.0.0.1:<port>`, and without
cookie are each refused; a holder-only test route (declare it in the test with a test-only
`RouteSpec`, via a `start_with_routes` used only by tests) refuses a second browser; responses
carry `Referrer-Policy: no-referrer` and no `Access-Control-Allow-*` header; port fallback: with
7272 taken, `start(port: None)` binds 7273 (`#[serial]`); `port: Some(0)` binds an OS port; an
explicit busy port is an error naming the address (no fallback). Also: the token is 64 lowercase
hex and differs between two starts; both bootstrap cookies carry exactly `Path=/; HttpOnly;
SameSite=Strict` and no `Max-Age`/`Expires`; `Host: evil.example` on `/api/server` is 403;
a `POST` to a JSON test route with a valid cookie and `Origin: http://evil.example` is 403, with no
Origin 403, with the exact Origin 200; the browser cookie's value never appears in the `/api/server`
body, the `hello` frame or a `presence` frame (search the raw bytes); a query value of 1025 bytes is
400; `GET /api/share` on loopback answers the tokenized URL and the `tunnel` command, and with a
`0.0.0.0` bind `bind: "remote"` and no tunnel; a `0.0.0.0` bind sets `ServerInfo.remote`.

A context test: after `Shutdown::begin` with a hook whose drain waits on a channel, a
connected WebSocket still receives a broadcast and `GET /api/server` answers with `stopping: true`;
releasing the drain stops the server.

`loom/tests/plan_server_cli.rs` (binary, `env!("CARGO_BIN_EXE_loom")`, temp git repo, `LOOM_HOME`
in a temp dir; skip when loopback bind fails): a second `loom plan --no-open --port 0` while the
first runs exits 1 and prints `loom plan is already running for this project (pid N).` and
`Open http://...?token=...` on stderr; outside a git repo exits non-zero naming the directory;
`--port <a port held by the test>` exits non-zero with the bind error; the runtime directory
`$LOOM_HOME/plan/<key>/` is mode 0700 and `lock` 0600; after the run, `git status --porcelain` in the
repo is empty (nothing written into the project); with `LOOM_PLAN_STATE` pointing at another temp directory named for the project key, the server
still uses `$LOOM_HOME/plan/<key>/` (lock created there, nothing written to the other directory) and
prints the warning; SIGINT (`nix::sys::signal::kill`) makes the
process exit 0 within 5 s, and the URL line carries the bound port. Keep each file under 400 lines.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::`, `--test plan_server_contracts`,
`--lib cli::`, `--test maintainability`, clippy and fmt pass. Report every signature that differs
from the shapes above.
