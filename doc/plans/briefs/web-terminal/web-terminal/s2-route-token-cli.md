# S2 — terminal route, token gate, CLI flag, wire model (codex `gpt-5.6-terra`, effort xhigh)

You own everything between an incoming TCP connection and S1's bridge: the `--terminals` flag,
the per-run token and its cookie, the `/ws/terminal/<stage>/<mode>` upgrade with its gate order,
stage-to-session resolution, the `terminals` field on the snapshot, and the server tests for all
of it. S1 owns `terminal/pty.rs`, `terminal/bridge.rs`, `terminal/protocol.rs` and their tests;
you call them, you do not edit them. S1's API (copy exactly):

```rust
// terminal/protocol.rs (S1)
pub(super) enum Mode { View, Control }
pub(super) struct WindowSize { pub cols: u16, pub rows: u16 }
pub(super) const CLOSE_UNKNOWN_STAGE: u16 = 4004;  // permanent: no such stage
pub(super) const CLOSE_REFUSED: u16 = 4008;        // permanent: never attachable
pub(super) const CLOSE_NOT_YET: u16 = 4009;        // retryable: not attachable yet
// terminal/pty.rs (S1)
impl PtyChild { pub(super) fn spawn(command: &mut Command, size: WindowSize) -> io::Result<Self> }
// terminal/bridge.rs (S1)
pub(super) fn run(socket: WebSocket<TcpStream>, child: PtyChild, mode: Mode, running: &AtomicBool)
```

If S1 is not in the tree yet when you start, write against those signatures; the orchestrator
integrates.

## Files

| Own (write) | Read only |
| --- | --- |
| `loom/src/commands/status/web/terminal/upgrade.rs` (S1 owns `terminal/mod.rs`, which declares your modules and re-exports `upgrade::handle_upgrade`; never edit it) | `loom/src/commands/status/web/terminal/{mod,pty,bridge,protocol}.rs` |
| `loom/src/commands/status/web/terminal/resolve.rs` | `loom/src/orchestrator/terminal/tmux/viewer.rs` (`live_tmux_sessions` :193-220, `tmux_session_name` :185-187, `endpoint_ready` :288-299, all `pub(crate)`) |
| `loom/src/commands/status/web/terminal/token.rs` | `loom/src/orchestrator/terminal/tmux/mod.rs` (`socket_name` :163-165, `pub`) |
| `loom/src/commands/status/web/terminal/tests_route.rs` | `loom/src/commands/attach/mod.rs` (`attach_direct` :154-193, `exec_tmux` :197-202: `env_remove("TMUX")`; `pick_newest` :80-86) |
| `loom/src/commands/status/web/connection.rs` | `loom/src/commands/status/data/collector.rs` (`load_all_sessions` :26-59, re-exported as `commands::status::data::load_all_sessions`; `session_for_stage` :82-97 private and NOT the rule you want — see task 6) |
| `loom/src/commands/status/web/http.rs` | `loom/src/fs/tmux_tmpdir.rs` (`adopt_recorded_tmux_tmpdir` :147-161) |
| `loom/src/commands/status/web/limits.rs` | `loom/src/daemon/server/tokens.rs:36-47` (private `generate_token_hex` — replicate, do not import) |
| `loom/src/commands/status/web/mod.rs` (add `mod terminal;` next to the other `mod` lines) | `loom/src/models/session/types.rs` (`Session`, `SessionStatus`, `SessionBackendKind`) |
| `loom/src/commands/status/web/broadcast.rs` | |
| `loom/src/commands/status/web/model.rs` | |
| `loom/src/commands/status/web/model_tests.rs` | |
| `loom/src/commands/status/web/tests.rs`, `tests/socket.rs`, `tests/pure.rs`, `tests/errors.rs` | `loom/src/commands/status/web/tests/ports.rs` (do NOT edit; keep the `#[path = "tests/ports.rs"] mod ports;` line in `tests.rs`) |
| `loom/src/cli/types_status_web.rs`, `loom/src/cli/dispatch.rs` (the one `web::execute` arm at :233-235) | |
| `loom/src/commands/attach/mod.rs` — VISIBILITY ONLY: change `fn matches_for_stage` and `fn pick_newest` to `pub(crate) fn`. Nothing else in that file. | `loom/src/verify/transitions/mod.rs` (`load_stage`, `pub` at :17) |
| `loom/maintainability-baseline.txt` — ONLY line 67, `function src/cli/dispatch.rs dispatch 128`, and only if your `dispatch.rs` edit changes that function's line count | |
| `web/src/api/fixtures/snapshot.json` (ONLY to add `"terminals": false`) | |
| `scripts/smoke-web-dashboard.sh` (one added check, task 9) | |

## Task 1 — CLI

`loom/src/cli/types_status_web.rs`: add to `StatusWebArgs`

```rust
/// Let the dashboard open live stage terminals (tmux backend only). Prints a tokenized URL.
#[arg(long, requires = "web")]
pub terminals: bool,
```

`web` is already `Option<Option<u16>>` (`--web` alone is `Some(None)`, meaning "scan upward from
`DEFAULT_PORT`"; `clap`'s `requires` reads presence, which `tests/pure.rs:247-253` already pins),
so `requires = "web"` works with a bare `--web`. `loom/src/cli/dispatch.rs:233-235`:
`Some(port) => status::web::execute(port, web_args.terminals)` — `port` is already `Option<u16>`
there; add only the second argument. Add to `tests/pure.rs` next to
`the_cli_preserves_omitted_and_explicit_dashboard_ports` (:236-266 — do NOT modify that test; it
pins the `Option<Option<u16>>` parse): `terminals_requires_web` (parsing
`["loom","status","--terminals"]` is an `Err` whose message mentions `required`) and
`terminals_parses_with_web` (`["loom","status","--web","--terminals"]` parses with the flag set).

## Task 2 — token (`terminal/token.rs`)

```rust
/// 32 bytes from /dev/urandom as 64 hex chars (same shape as the daemon's tokens).
pub(super) fn mint() -> std::io::Result<String>
/// Constant-time equality over bytes; `None` never matches.
pub(super) fn matches(presented: Option<&str>, expected: &str) -> bool
/// The cookie name for a dashboard bound to `port`: `loom_dashboard_<port>`.
pub(super) fn cookie_name(port: u16) -> String
/// The value of cookie `name` out of a raw `Cookie` header (`a=b; loom_dashboard_7373=X; c=d`).
pub(super) fn cookie_token<'a>(cookie_header: Option<&'a str>, name: &str) -> Option<&'a str>
/// The `token` value out of a raw query string (`token=X&y=z`); no percent-decoding (hex only).
pub(super) fn query_token(query: Option<&str>) -> Option<&str>
```

**The cookie name carries the bound port and there is no fixed `COOKIE_NAME` constant.** Cookies
have NO port scope: `Domain`/`Path` are the only axes, so every dashboard on 127.0.0.1 shares one
cookie jar. The CLI deliberately supports several dashboards at once — `--web` without a value
scans upward from `DEFAULT_PORT` (`bind_first_available_loopback_port`, `web/mod.rs:69-83`) — so a
fixed name means opening instance B's tokenized URL OVERWRITES instance A's cookie; A's own
same-origin page then presents B's token and gets 401 on every terminal from then on. The exact
`Origin == Host` gate closes the cross-port control attack but does nothing about this collision.
`serve` derives the name once from `listener.local_addr()?.port()` and threads it down with the
token; no other module may spell `loom_dashboard` itself. Tests:
`cookie_name_is_scoped_to_the_bound_port` (pure: `cookie_name(7373) == "loom_dashboard_7373"`) and
`two_servers_do_not_share_a_cookie_name` (start two servers via `start_with`, bootstrap a token on
each, and assert the `Set-Cookie` names differ and that instance A still accepts its own cookie
after B's bootstrap — one jar, two names).

Constant time: copy the SHAPE of `daemon/server/tokens.rs:101 ct_eq` (private, so replicate
rather than import) — fold `a.len() == b.len()` and the XOR of every byte pair into one
accumulator; do not short-circuit. `mint` mirrors `tokens.rs:36-47 generate_token_hex` (32 bytes
from `/dev/urandom`); `hex = "0.4.3"` is already a dependency, so `hex::encode` beats a
`format!` loop.

## Task 3 — request head (`http.rs`)

`RequestHead` (http.rs:13-20) gains `pub query: Option<String>` (the part after `?`, from the
`split_once('?')` at :57-60 that currently discards it) and `pub cookie: Option<String>` (raw
`Cookie` header via the existing `header_value`, :29-43; note `header_value` `bail!`s on a
duplicated header and `head.rs:51-54` turns that into a 400, so two `Cookie:` lines never reach
the token gate — expected, not a bug). Add a NEW helper (it does not exist yet)
`pub(super) fn write_redirect(stream: &mut TcpStream, location: &str, set_cookie: Option<&str>) ->
io::Result<()>` that writes `HTTP/1.1 302 Found` with `Location`, the same fixed security headers
`response_head` (:161-165) emits, `Content-Length: 0`, and `Set-Cookie` when given. It is reached
from `handle` with the request head still UNCONSUMED in the socket (`handle` only peeks,
`connection.rs:82`; `:141-146` explains why writing over an unread request makes Linux answer
RST and the browser never sees the response), so it must drain first exactly as `fail` does
(`connection.rs:180-181`): either make `drain_pending` `pub(super)` and call it before writing, or
put the redirect writer in `connection.rs` next to `fail`. Keep `response_head` for the other
responses; factor the shared header block into one helper so the existing test
`every_http_response_carries_the_security_headers` keeps passing for the redirect too (extend
that test to cover the redirect).

## Task 4 — server options and startup (`mod.rs`)

```rust
/// What `loom status --web` was started with.
#[derive(Debug, Clone, Default)]
pub struct ServeOptions {
    /// `Some(token)` when `--terminals` was given: the cookie value every terminal upgrade must present.
    pub terminal_token: Option<String>,
}
pub fn execute(port: Option<u16>, terminals: bool) -> Result<()>
pub fn serve(listener: TcpListener, base: PathBuf, running: Arc<AtomicBool>, options: ServeOptions) -> Result<()>

/// The terminal lane's resolved configuration, built inside `serve` from `options` and the
/// listener's own port. This — not `ServeOptions` — is what `connection::handle` receives, so
/// the cookie name can never disagree with the port the server actually bound.
pub(super) struct TerminalLane {
    pub(super) token: String,
    pub(super) cookie_name: String,  // token::cookie_name(actual_port)
}
```

`execute` ALREADY takes `Option<u16>` (`None` = scan upward from `DEFAULT_PORT`) and binds through
`bind_listener` / `bind_first_available_loopback_port` (`web/mod.rs:62-84`): do NOT touch those,
nor the tests in `web/tests/ports.rs` that import them. Only the `terminals` parameter is new. The
startup line prints `actual_port` (`listener.local_addr()?.port()`), never the requested one, and
so must the tokenized URL: when `terminals`, `token::mint()` and print
`loom dashboard: http://127.0.0.1:{actual_port}/?token={token}  (terminals enabled; Ctrl-C to stop)`;
otherwise the existing line. When `terminals` is set — and only then — call
`crate::fs::tmux_tmpdir::adopt_recorded_tmux_tmpdir(work_dir.root())` ONCE here, before `serve`, on
the main thread. It mutates the process environment (`fs/tmux_tmpdir.rs:155-159`), so the
flagless dashboard must not reach it: without `--terminals` this server stays byte-for-byte what
it is today. Never call it from a connection thread. The argument is the STATE dir — `work_dir`
is the `WorkDir::new(".")` already bound at the top of `execute` (`web/mod.rs:43`), and
`WorkDir::root()` is `.loom/work` (`fs/work_dir.rs:453`; cf. `sessions_dir()` at `:396`). The
function reads `<arg>/tmux-tmpdir` and probes `<arg>/orchestrator.pid` (`:130`, `:148`), so
passing `Path::new(".")` or the repo root makes the call a silent permanent no-op. It returns
`TmuxTmpdirAdoption`; log the `Adopted` arm the way `attach::execute` prints it
(`commands/attach/mod.rs:49-56`). Adoption happens only while the daemon is live — that is
crate-wide policy (`loom attach` does the same; `:139-146`), so say in the docstring that a
daemon-less dashboard keeps the ambient `TMUX_TMPDIR`. Put the call behind a small
`fn adopt_for(work_dir: &WorkDir) -> TmuxTmpdirAdoption` so a test can drive it (task 8).

`serve` does not join connection threads today (`web/mod.rs:95-133`: spawn, forget, return the
moment `running` is false), so Ctrl-C would strand one `tmux attach-session` client per open
tab. After the accept loop exits, wait up to 3 s (50 ms polls) for the **`Lane::Connection`**
counter to reach zero before `serve` returns, so every bridge observes `running == false`, sends
1001, and runs `child.shutdown()`.

Drain the CONNECTION lane, not the terminal lane. A connection can sit in `read_head` for up to
`HEAD_TIMEOUT` (5 s, `head.rs:10`) before anything knows it is a terminal, while its terminal
count is still zero — so a terminal-lane drain can observe zero, let `serve` return, and have
that thread spawn a `tmux attach-session` child afterwards. The connection slot is held for the
whole thread lifetime (`web/mod.rs:105-151`), so waiting on it covers the undecided ones. The 3 s
bound can still expire under a stalled head; what makes THAT safe is the acquire-then-recheck
rule in task 5, not the drain. `serve` threads the options into `connection::handle` (add a
`&ServeOptions` parameter) and into `Broadcaster::spawn`. Rewrite the module docstring's
"Access control" section: the snapshot lanes are unchanged; the terminal lane is opt-in, requires
the `Origin` header AND the cookie, and the token is printed once at startup — anyone who can read
the operator's terminal or `?token=` URL can type into the agents.

## Task 5 — routing and the upgrade (`connection.rs`, `limits.rs`, `terminal/upgrade.rs`)

`limits.rs`: add `Lane::Terminal` and `pub(super) const MAX_TERMINALS: usize = 8;` (one per open
browser tab; each holds a PTY, a tmux client and a thread). The match arms live in
`Lane::capacity` (:34-40) and `Lane::counter` (:42-47), NOT in `Slot::acquire` (:74, which is
lane-generic); `Limits` (:50-54) gains a third `terminals: AtomicUsize` field. A terminal
connection holds the `Lane::Connection` slot the accept loop already took plus a `Lane::Terminal`
slot, never a `Lane::WebSocket` one, so open terminals cannot starve `/ws` subscriptions.

`connection.rs::route` (:302-316): add `path.starts_with("/ws/")` to the `Missing` arm so a plain
GET of a terminal path is a 404, not the SPA. Add the token bootstrap: in `handle`, after the Host
check and before routing, if `path == "/"`, the method is `GET` or `HEAD` (the method check
otherwise happens later, at `connection.rs:100`), and `query_token(head.query)` is `Some(t)`: with
`options.terminal_token` `Some(expected)` → `matches` ⇒ `write_redirect("/",
Some("<cookie_name>=<t>; Path=/; HttpOnly; SameSite=Strict"))` where `<cookie_name>` is
`lane.cookie_name` (i.e. `loom_dashboard_<bound port>`), else 403 `token not accepted`;
with terminals disabled → fall through to the SPA (ignore the parameter).

Terminal upgrade, next to the existing `/ws` branch (:91): when `peeked.upgrade_websocket` and
`peeked.path` starts with `/ws/terminal/`, call `terminal::handle_upgrade(stream, &peeked, base,
options, running, limits)`. Gate order inside it, each step answering with `connection::fail`
(make it `pub(super)`; it is already) BEFORE any WebSocket accept:

1. terminals disabled → 404 `not found` (do not reveal the lane).
2. `Origin` header ABSENT, or not `http`/`https`, or its authority (the part after `://`, up to
   the first `/`, `?` or `#`) not byte-equal to the request's `Host` header → 403 `origin not
   allowed`. Do NOT reuse `http::origin_allowed` (`http.rs:133-147`): it accepts an absent Origin
   and ANY loopback port. `SameSite` is keyed on the registrable domain and, for an IP literal,
   ignores the port, so a page served by any other local server (`http://127.0.0.1:9999`) is
   same-site to the dashboard, the browser attaches the `loom_dashboard_<port>` cookie to that page's
   WebSocket, and a loopback-only check would let it take control of the agents. `Host` has
   already passed `host_allowed` (loopback) at `connection.rs:87`, so equality with it is the
   whole check. Write `pub(super) fn origin_matches_host(origin: Option<&str>, host: Option<&str>)
   -> bool` in `upgrade.rs` (pure; unit-tested in `tests_route.rs`: absent → false, other scheme
   → false, same host other port → false, `localhost:P` vs `127.0.0.1:P` → false, equal → true).
3. cookie token missing or wrong → 401 `dashboard token required`.
4. path shape `/ws/terminal/<stage>/<mode>` where `<mode>` is `view` or `control` and `<stage>` is
   a plain identifier (`[A-Za-z0-9_-]+`; reuse `viewer::is_plain_identifier`, `pub(crate)` at
   `viewer.rs:168`) → else 404.
5. `Slot::acquire(limits, Lane::Terminal)` → `None` ⇒ `fail(stream, 503, "Service Unavailable",
   b"terminal limit reached")`. Not `reject_overloaded` (`connection.rs:167-177`): that one
   belongs to the accept loop and re-sets the write timeout for it.
6. **Then, holding that slot, re-read `running`**: `if !running.load(Ordering::SeqCst) { return
   fail(stream, 503, "Service Unavailable", b"server stopping"); }` — the slot drops on return.
   The order matters and is not interchangeable. `Slot` releases in `Drop` (`limits.rs:88-95`),
   so slot occupancy alone cannot tell a shutting-down server that a connection is ABOUT to
   become a terminal, and a connection accepted microseconds before Ctrl-C can spend up to 5 s in
   `read_head` (`head.rs:10`) before it classifies. Acquire-then-recheck is race-free in both
   directions: if the increment lands before the drain's read, the drain waits for it; if it
   lands after, `running` is already false and this gate refuses. Checking `running` BEFORE
   acquiring reopens the hole.

Then `tungstenite::accept_with_config` exactly as `ws.rs:24-31`, then resolve (task 6): a refusal
becomes `socket.close(Some(CloseFrame { code: CloseCode::Library(refusal.close_code()), reason:
refusal.reason().into() }))` followed by one flush and return; success builds the tmux command and hands
off to `bridge::run`. tungstenite 0.30 spellings (verified against the installed crate):
`4000..=4999` maps to `CloseCode::Library`, `Bad` fails `is_allowed()`; `CloseFrame.reason` is
`Utf8Bytes`, so `.into()` a `&str`; build messages with `Message::text(..)`/`Message::binary(..)`,
never the tuple variants with a `String`/`Vec<u8>`.

tmux client command: `Command::new("tmux")` with args `["-L", &target.socket, "-T", "256,RGB",
"attach-session", "-t", &target.tmux_session]` and, for `Mode::View`, `"-f", "read-only"`
inserted before `-t` (`-T` and `-f` need tmux ≥ 3.2; `attach_direct` at `attach/mod.rs:154-193`
passes neither, so this is the first `-T` in the tree — the host has 3.6a, CI installs Ubuntu's).
`env_remove("TMUX")` (as `attach/mod.rs:197-202`), `env("TERM", "xterm-256color")`. Initial
`WindowSize { cols: 120, rows: 36 }`; the browser sends its real size in its first frame. Expose
`pub(super) fn attach_args(target: &Target, mode: Mode) -> Vec<String>` in `resolve.rs` and pin it
with `attach_args_view_mode_is_read_only` in `tests_route.rs`: view's args contain `["-f",
"read-only"]` adjacently before `-t`, control's do not contain `read-only` at all.

## Task 6 — resolution (`terminal/resolve.rs`, pure)

```rust
pub(super) struct Target { pub socket: String, pub tmux_session: String }
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Refusal { UnknownStage, NativeBackend, Unrenderable, NoSession, NotReady }
impl Refusal { pub(super) fn close_code(&self) -> u16; pub(super) fn reason(&self) -> &'static str }
pub(super) fn resolve(
    stage_id: &str,
    known_stage: bool,
    all_sessions: &[Session],   // every session file: used ONLY to explain a miss
    live: &[Session],           // viewer::live_tmux_sessions output
    ready: impl Fn(&Session, &str) -> bool,
) -> Result<Target, Refusal>
```

**Do not restate attach's rule — call it.** `resolve` takes `live` already filtered by
`viewer::live_tmux_sessions` and selects with `crate::commands::attach::{matches_for_stage,
pick_newest}`, which you promote from private to `pub(crate)` in `attach/mod.rs` (two keywords,
no other change). One definition serves both callers; a second copy is how the dashboard and
`loom attach` would silently disagree about which session a stage id means.

What `live_tmux_sessions` (`viewer.rs:189-249` → `load_live_tmux_session` :226-249) already
excludes, so `resolve` must NOT re-check any of it: `backend != Tmux`; every status except
`Running | Spawning` — **`Paused` is NOT attachable**, the earlier "any non-terminal status"
wording was simply wrong; `!TmuxBackend::is_session_alive(session)`; and sessions with no
derivable `tmux_session_name`. Liveness there is `native::pid_only_is_alive` — PID-file identity
plus start time — and **never `session.pid`**: `pid_guard.rs:20-23` states outright that
`session.pid` is informational and never an identity fallback, and using it reintroduces PID-reuse
and missing-identity false positives.

The rules, in order:

1. `!known_stage` → `UnknownStage` (**4004**, reason `no stage with that id`).
2. `pick_newest(&matches_for_stage(live, stage_id))`:
   - `Some(session)`: `let Some(name) = tmux_session_name(session) else { return
     Err(Refusal::Unrenderable) };` (discovery already guaranteed a name, so this arm never
     fires in production — it exists so `resolve` stays total for a hand-built test slice);
     `let socket = socket_name(session)`. If
     `!is_plain_identifier(&socket) || !is_plain_identifier(&name)` → `Unrenderable` (**4008**,
     reason `this session's identifier cannot name a tmux session`). Validate BOTH, exactly as
     `attachable_panes` does (`viewer.rs:118-132`): `tmux_session_name(..).is_none()` is not
     validation — it only means no name could be derived at all, and a corrupt non-empty tracking
     key sails straight past it. Then `!ready(session, &name)` → `NotReady` (**4009**, reason
     `still spawning, or just ended`). Otherwise `Ok(Target { socket, tmux_session: name })`.
   - `None`: look in `all_sessions` for the newest non-terminal session naming `stage_id`. If one
     exists with `backend != Tmux` → `NativeBackend` (**4008**, reason `this stage's session runs
     in a native terminal window; terminals need loom run --backend tmux`). Otherwise
     `NoSession` (**4009**, reason `no live session for this stage yet`).

The reason strings above are the literal, browser-worded ones; `attach/wait.rs`'s
`EndpointFailure`/`describe_failure` are private and end in "re-run `loom attach` in a moment", so
do not reach for them.

Close codes are three classes because the page reacts differently to each: 4004 and 4008 are
permanent (the page shows the reason and stops), 4009 is retryable (the page shows `waiting` and
reconnects every 2 s). A two-code scheme is wrong in both directions — a native-backend session
would be polled forever, a stage between two sessions would never be retried.

Production caller, in `upgrade.rs`:

```rust
let work_dir = WorkDir::new(base)?;
let known = crate::verify::transitions::load_stage(stage_id, work_dir.root()).is_ok();
let all = crate::commands::status::data::load_all_sessions(&work_dir)?;
let live = viewer::live_tmux_sessions(work_dir.root())?;
resolve(stage_id, known, &all, &live, endpoint_ready)
```

`load_stage` is `pub` (`verify/transitions/mod.rs:17`) and resolves the numeric-prefixed stage
filename itself. `load_all_sessions` is re-exported at `commands::status::data`
(`data/mod.rs:7`); the `collector` module itself is private, so do not name it in a path. Loading the inventory is what makes 4004 mean what it says: without it, "no
matching session" and "no such stage" are the same answer, and a known stage between two sessions
would be refused permanently — the exact case a terminal gets opened for.

Accepted divergence, worth a `loom memory note`: the `>_` glyph's gate is
`StageSummary.session_alive`, which the collector computes from the RAW pid
(`collector.rs:146`), so a stage can show a glyph and then answer 4009. That is the honest
outcome — the page renders it as `waiting` — and the fix is never to weaken the resolver to match
the collector.

Tests (`tests_route.rs`) drive `resolve` with hand-built `Session` slices and a `ready` closure,
named exactly: `resolve_unknown_stage_refuses_4004`,
`resolve_known_stage_without_session_refuses_4009`, `resolve_native_backend_refuses_4008`,
`resolve_rejects_a_corrupt_identifier` (a tracking key containing a space reaches `live` and is
refused 4008), `resolve_not_ready_refuses_4009`,
`resolve_returns_the_target_for_a_live_tmux_session`, `resolve_newest_live_session_wins` (a newer
`Running` beats an older one), `resolve_paused_session_is_not_attachable` (a `Paused` session
never reaches `live`, so the stage answers 4009 — assert the refusal, and assert
`live_tmux_sessions` is the reason by keeping it out of the `live` slice), and
`refusal_close_code_table` (a table pinning all five variants to 4004/4008/4008/4009/4009 and to
their exact reason strings — F2's `terminal.test.ts` carries the mirror of this table).

## Task 7 — wire model

`model.rs::WebSnapshot`: add, after `source`,

```rust
/// Whether this server accepts browser terminal attachments (`loom status --web --terminals`).
pub terminals: bool,
```

`broadcast.rs`: `Broadcaster::spawn(base, running, terminals: bool)` stores it; `snapshot_frame`
and `fresh_file_snapshot` gain a `terminals: bool` parameter and set the field; update every
caller (`run`, `poll_files_once`, `classify_response` if it builds frames, and the tests in
`tests/pure.rs`). `model_tests.rs::fixture_snapshot` (:84-102) sets `terminals: false`;
`web/src/api/fixtures/snapshot.json` gains `"terminals": false` (the whole-document test
`fixture_matches_serde_output` compares `serde_json::Value`s, so key position is free; put it
after `"source"`). F2 adds the same field to the zod schema; the fixture value is the contract.

## Task 8 — server tests (`tests/socket.rs`, `tests.rs`)

Split `serve` so tests can see the admission counters: `serve` keeps its public signature and
becomes three lines — build `Limits`, build `TerminalLane` from `options` and the listener port,
call `pub(super) fn serve_with(listener, base, running, options, limits: Arc<Limits>) -> Result<()>`
which holds today's body. Update `tests.rs::start` to take `ServeOptions` (keep a `start(base)`
wrapper with defaults so the existing tests are untouched; add `start_with(base, options)` calling
`serve_with`) and have `start_with` hand the test back the port, the `Arc<AtomicBool>` running
flag and the `Arc<Limits>`. Add `pub(super) fn terminal_count(&self) -> usize` to `Limits` for the
assertions below. New tests, each guarded with
`skip_without_loopback`:

- `terminal_upgrade_when_disabled_is_404`
- `terminal_upgrade_without_origin_is_403` and `terminal_upgrade_with_foreign_origin_is_403`
- `terminal_upgrade_without_token_is_401` and `terminal_upgrade_with_wrong_token_is_401`
- `terminal_unknown_stage_closes_4004`: full handshake with the EXACT listener origin —
  `Origin: http://127.0.0.1:<port>`, port included — and the cookie named
  `token::cookie_name(port)` (build the request with `into_client_request()` then
  `headers_mut().insert(..)`), then read one message and assert a `Close` with code 4004.
  A port-less `Origin: http://127.0.0.1` is byte-UNEQUAL to the `Host` header tungstenite sends
  (`127.0.0.1:<port>`), so it is refused 403 BEFORE the accept and the test can never observe a
  close code at all. Take the port from `listener.local_addr()`; never hardcode it.
- `terminal_upgrade_with_same_site_other_port_origin_is_403`: cookie present, `Origin:
  http://127.0.0.1:<port+1>` (a loopback origin whose port differs from `Host`) → 403.
- `token_bootstrap_sets_cookie_and_redirects` (302, `Location: /`, `Set-Cookie` contains
  `HttpOnly`, `SameSite=Strict` AND `Path=/`) and `token_bootstrap_with_wrong_token_is_403`.
- `plain_get_of_a_terminal_path_is_404`.
- `terminal_upgrade_after_stop_is_503`: with a server started normally, set its `running` flag
  false and then send one fully-valid upgrade (right Origin, right cookie, real stage path);
  assert 503 with body `server stopping`. This is gate 6 and it is the whole reason a late
  connection cannot start a tmux client behind the drain.
- `a_late_upgrade_after_shutdown_spawns_no_child`: accept-side race test. Open a TCP connection
  to the server but send NOTHING for 200 ms; meanwhile set `running` false and join the `serve`
  thread (assert it returns within 4 s); only then write the upgrade head; assert the response is
  503 or the socket is closed, and that `limits.terminal_count()` is still zero — no PTY and no
  `tmux attach-session` child was ever created.
- `cookie_name_is_scoped_to_the_bound_port` and `two_servers_do_not_share_a_cookie_name` as
  described in task 2.
- `terminal_slots_are_capped`: a refused upgrade CANNOT hold a slot — a 4004 writes one small
  close frame and returns, and `Slot` releases in `Drop` (`limits.rs:88-95`), so leaving the
  client unread parks nothing and the ninth request would be accepted. Pre-occupy the lane
  instead, through a test-visible seam: expose `pub(super) fn acquire_terminal_slot(limits:
  &Arc<Limits>) -> Option<Slot>` in `limits.rs`, have the upgrade call it, and in the test hold
  `MAX_TERMINALS` `Slot`s in a `Vec` against the SAME `Arc<Limits>` `start_with` handed back,
  then assert one real upgrade gets 503 with body `terminal limit reached`, and that dropping the
  vec lets the next one through again. No PTY, no tmux, no parked sockets.
- `snapshot_carries_terminals_flag`: `/api/status` body has `"terminals":true` under
  `start_with(.., terminal_token: Some(..))` and `false` under `start`.
- `adopt_is_called_with_the_state_dir` (`tests/pure.rs`): a PID file is NOT what
  `adopt_recorded_tmux_tmpdir` consults. It asks `DaemonServer::is_running`, which is
  `check_status`, which treats the **flock on `orchestrator.lock` as authoritative** — `Held` is
  the only running answer, and a free or absent lock is `NotRunning`
  (`daemon/server/core.rs:115-158`, `daemon/server/lock.rs:24-49`). A live-looking
  `orchestrator.pid` alone therefore yields `DaemonNotRunning` and the test can never see
  `Adopted`. Build the fixture so a daemon really looks alive: create
  `<tmp>/.loom/work/orchestrator.lock`, open it, and hold `libc::flock(fd, LOCK_EX | LOCK_NB)`
  for the duration of the call — flock is per open-file-description, so a second fd on the same
  file, even in this process, gets `EWOULDBLOCK` and `inspect_lock` reports `Held`. Write
  `<tmp>/.loom/work/tmux-tmpdir` next to it, then drive `adopt_for(&WorkDir)` (task 4) and assert
  the `Adopted` arm; then assert `DaemonNotRunning` when the same fixture is addressed by its repo
  root (no lock file there). Drop the guard fd at the end and restore any `TMUX_TMPDIR` the call
  set (`#[serial]`).

Most test names in tasks 1, 2, 6 and 8 are acceptance lines in the plan (each is grepped out of
`cargo test -- --list`); a renamed test fails the
stage.

## Task 9 — smoke script

`scripts/smoke-web-dashboard.sh` runs without `--terminals`; add one check after the 404 asset
probe: `GET /ws/terminal/x/view` (plain, no upgrade) expects `404`. (S3's
`scripts/smoke-web-terminal.sh` covers the `--terminals` side; it derives the cookie name as
`loom_dashboard_<port>` from the port it already parses out of the printed URL.)

## Constraints

- File ≤ 400 lines and function ≤ 50 lines (`cargo test --test maintainability` is in the gate).
  `connection.rs` is 316 lines today: put the terminal gate in `terminal/upgrade.rs`, not inline.
  `loom/maintainability-baseline.txt:67` pins `function src/cli/dispatch.rs dispatch 128` and the
  gate fails if that function's measured length moves EITHER way (`tests/maintainability/baseline.rs:182-198`);
  if your `dispatch.rs` edit changes it, update that one line to the new count — you own that line
  and nothing else in the file. If a file you own crosses 400 lines (`tests/pure.rs` is 360,
  `broadcast.rs` 299), split it rather than adding a baseline entry.
- Every refused upgrade returns before `accept`; every accepted one ends in `bridge::run`.
- No `TODO`/`FIXME` strings anywhere.
- Never run `git`. The orchestrator commits.
- `loom memory note` for anything that surprised you; `loom memory decision` for any choice not
  fixed above.

## Done means

`cargo test --lib commands::status::web::` passes locally (run ONCE), the test names in tasks 1,
6 and 8 appear in `-- --list`, `loom status --help` shows `--terminals`, and `rg -q '"terminals"'
web/src/api/fixtures/snapshot.json` succeeds.
