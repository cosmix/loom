# Configurable dashboard listener and remote access policy

Implement `loom status --web [PORT] --host HOST`. The default remains
`127.0.0.1`; accept IPv4/IPv6 literals and `localhost` (normalize the latter to
127.0.0.1 without DNS). Reject hostnames, URLs, addresses containing ports,
whitespace, and malformed values during CLI parsing. `--host` requires `--web`.
Use an optional parsed host in clap so its default cannot accidentally make
every `status` invocation require `--web`; resolve the omitted default at dispatch.

## Grounded starting points

- `cli/types_status_web.rs::StatusWebArgs` owns web flags; `cli/dispatch.rs:322`
  calls `status::web::execute(port, web_args.terminals)`.
- `web/mod.rs::bind_listener` binds 127.0.0.1; `None` walks from 7373 on
  `AddrInUse` only, `Some(n)` binds exactly, and zero requests an ephemeral port.
  `serve(TcpListener, PathBuf, Arc<AtomicBool>, ServeOptions)` is also public.
- `ServeOptions` currently has only `terminal_token: Option<String>`; presence
  controls both terminal authentication and the published `terminals` capability.
- `connection::handle` checks Host before bootstrap, HTTP routing, or WS upgrade.
  `http::{host_allowed,origin_allowed,origin_allowed_strict}` hardcode loopback.
  Origin gates also exist in status WS, status GET, and config GET/POST.
- `terminal/upgrade.rs::admit` independently rechecks Host, lane enabled, exact
  Origin/Host, cookie, path, capacity and shutdown before creating a PTY.
  Keep that independent gate. Token helpers already use 32 random bytes and a
  port-scoped `HttpOnly; SameSite=Strict; Path=/` cookie; reuse them.
- Settings POST is an existing filesystem write surface even without terminals.
  Remote access therefore needs authentication for the entire dashboard.

Paths above are relative to `loom/src/commands/status/` except `cli/` paths,
which are relative to `loom/src/`.

## Chosen contracts

Introduce small leaf modules `web/listener.rs`, `web/access.rs`, and
`web/auth.rs`. Keep files <=400 lines and functions <=50 lines. Dispatch is
397 lines today: replace its call, do not append a sizeable branch. Connection
is 381 lines: extract bootstrap/admission helpers as part of the change.

`listener::bind_listener(host: IpAddr, port: Option<u16>) -> Result<TcpListener>`
and `bind_first_available_port(host: IpAddr, start: u16)` preserve the existing
atomic bind algorithm and error kinds. Errors identify the attempted socket
address. Use SocketAddr formatting for IPv6 brackets. No port probing followed
by a separate bind, DNS resolution, interface enumeration, or new dependency.

`execute(port: Option<u16>, terminals: bool, host: IpAddr)` is the single CLI
adapter. Resolve the actual socket after binding. For wildcard bindings print
the listening address plus a usable loopback bootstrap URL with the actual port,
and explain replacing the loopback address with the machine's reachable IP.
Do not imply `0.0.0.0` or `::` is a remote browser destination. For a concrete
address print that address. Remote mode output states HTTP is unencrypted and
the token grants dashboard/settings access; terminal access remains opt-in.

Preserve `ServeOptions.terminal_token` for callers and add
`dashboard_token: Option<String>`. Introduce one resolved process auth object:
terminal-only input supplies it too; when both are present require equality;
reject tokens unless they contain exactly 64 lowercase hexadecimal characters.
Update the owned e2e fixture's current `TOKEN = "e2e-token"` to that format.
CLI mints one token when the bound listener
is non-loopback OR terminals are enabled, populating terminal_token only for
`--terminals`. `Broadcaster::spawn(..., terminals)` must continue deriving its
capability from terminal opt-in, never from dashboard authentication presence.

`serve_with` resolves immutable access policy from `listener.local_addr()` and
options BEFORE spawning broadcaster or accept work. Reject a non-loopback
listener without valid process authentication, including wildcard listeners
passed directly to public `serve`; do not trust a CLI-only check or a caller's
claim that the listener is loopback. A wildcard listener stays remote-policy
even when a particular client connects through loopback.

Pass resolved policy through accept/spawn into routing; use each accepted
stream's `local_addr()` for Host validation. Remote Host must be a strict numeric
socket authority whose IP and port equal that actual local endpoint. Permit
`localhost:<actual-port>` only when the accepted local endpoint is loopback.
Reject missing/duplicate Host, userinfo, paths, malformed ports/brackets,
arbitrary DNS names, and wildcard destination authorities. IPv6 authority
parsing must compare parsed addresses; normalize IPv4-mapped addresses when
the OS reports a dual-stack accepted endpoint. Do not add proxy headers or a
user supplied allowlist. Preserve the current loopback-only Host compatibility
for default mode; tightening that separate behavior is outside this feature.

For remote mode, any supplied Origin must be an exact HTTP origin for the
validated request authority (no path/query/fragment/userinfo; reject `null`,
wrong scheme, host, and port). Missing Origin is allowed for ordinary GET/HEAD
navigation and assets; require Origin for settings POST and both WS lanes.
Keep existing loopback origin semantics, including terminal exact-origin
admission. Retain config CSRF verification, JSON/framing/body limits and gate
order: auth and Origin precede body reads and mutation.

The only unauthenticated remote success is GET/HEAD `/?token=<valid-token>`:
validate Host and any supplied Origin first, exchange token for the existing
port cookie, then redirect to `/`. Invalid bootstrap token returns 403; absent
or wrong cookie returns 401 on every other route, including assets, SPA
fallback, status/config GET, config POST, `/ws`, and terminal WS. Query tokens
on other paths never authenticate. Avoid token logging beyond the deliberate
startup URL; preserve security headers and token-stripping redirect. A remote
cookie alone must not enable terminal capability. Loopback without terminals
remains usable without a cookie; loopback terminals retain their current
dashboard usability and terminal cookie requirement.

Use one shared policy API for HTTP/config/status WS and terminal admission,
for example `AccessPolicy::host_allowed(local, head)` and
`origin_allowed(head, require_origin)`. Thread policy into config request gates
instead of weakening the old global loopback helpers. Terminal admission also
checks policy directly before PTY allocation. Do not remove a repeated gate
without proving the leaf entry point is no longer independently callable.

## Required reads and write ownership

Read source graph outlines first, then these focused sources:
`loom/src/cli/{types_status_web,dispatch}.rs`;
`loom/src/commands/status/web/{mod,connection,http,head,tests}.rs`;
`web/config_api/{request,csrf}.rs`; `web/terminal/{mod,upgrade,token,tests_upgrade}.rs`;
`web/tests/{ports,pure,socket,config_api}.rs`;
`loom/tests/e2e/web_terminal.rs`; `scripts/smoke-web-dashboard.sh` and
`scripts/smoke-web-terminal.sh`; README dashboard/settings/terminal sections.
Here the abbreviated `web/` paths share the command-status prefix above.
Run `rg -n 'ServeOptions|TerminalLane|handle_upgrade|bind_listener|bind_first_available_loopback_port|host_allowed|origin_allowed' loom/src loom/tests`
to enumerate every initializer and caller before changing signatures.

Exact implementation write set (all web paths below use the full
`loom/src/commands/status/web/` prefix):

- One Terra web worker: `web/mod.rs`, new `web/listener.rs`,
  `web/connection.rs`, `web/http.rs`, new `web/access.rs`, new `web/auth.rs`,
  `web/config_api/request.rs`, `web/terminal/upgrade.rs`,
  `web/terminal/token.rs`, `web/terminal/mod.rs`, new `web/auth_tests.rs`,
  `web/tests.rs`, `web/tests/ports.rs`, `web/tests/pure.rs`,
  new `web/tests/remote.rs`, `web/terminal/tests_upgrade.rs`,
  `web/terminal/tests_upgrade/shutdown.rs`, and
  `loom/tests/e2e/web_terminal.rs`. This worker owns all module declarations,
  including registering Luna's `web/access_tests.rs`.
- A read-only Sol design/review worker checks this access-policy contract and
  existing call paths before Terra edits. Sol writes no files; Terra resolves
  its findings within the owned set before implementation and hands off final
  function signatures to the orchestrator and Luna.
- Luna owns only new `web/access_tests.rs`, after Terra's APIs land. Luna
  implements the pure authority/origin matrix below; Terra owns socket,
  bootstrap, port, token and existing regression tests.
- Shared CLI Terra worker, outside this brief's write boundary, owns
  `loom/src/cli/types_status_web.rs`, `loom/src/cli/dispatch.rs` and CLI parse
  tests. Integrate this serially after the web, memory and retrieval workers
  finish. Every CLI file is read-only for the web worker.

User-facing documentation is owned by final distillation; README is read-only
for this worker. Supply the settled CLI behavior and remote HTTP/token caveat
in the handoff for a dedicated user guide.

Shared CLI handoff: add optional `host: Option<IpAddr>` to StatusWebArgs with
`#[arg(long, requires = "web", value_parser = parse_web_host)]`; the CLI owner
provides a small parser accepting literal IPs and the exact alias `localhost`
(127.0.0.1), rejecting all other forms. Call the settled public signature
`web::execute(port, web_args.terminals, web_args.host.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)))`.
The web worker implements that signature; the shared CLI owner wires it and
owns parsing/help regressions. No additional global/user/project config key.

`head.rs` is a read dependency. `http.rs::header_value` already rejects
duplicate Host and `parse_head_with_len` constructs RequestHead; preserve that
parser invariant, including its ambiguous-header regressions. CLI parsing tests
belong to the shared CLI brief, not `web/tests/remote.rs`. Sol reviews first,
Terra implements web, then Luna adds the pure matrix after API handoff; the
shared CLI integration is serial after independent core workers. Workers never
run git, builds, tests, formatters, linters or other verification; the
orchestrator runs the acceptance commands. No frontend/dist changes are
expected: frontend URLs already derive from browser location.

## Acceptance evidence

The owned `terminal/tests_upgrade/shutdown.rs` constructs TerminalLane and calls
`terminal::handle_upgrade` directly. Thread the new policy into it and preserve
its no-child-after-shutdown assertion.

Required coverage includes default parsing and host-requires-web; IPv4,
localhost and IPv6 parsing; invalid names/URLs/port-bearing input; occupied
automatic port advance versus exact-port failure; zero actual-port reporting;
wildcard/public-serve fail-closed behavior; concrete and wildcard endpoint Host
validation including different ports, DNS rebinding, IPv6/mapped IPv6; remote
bootstrap redirect/cookie flags; unauthenticated refusal of every route;
authenticated status/config/WS success; wrong/missing/foreign Origin and valid
cookie plus invalid CSRF rejection before write; disabled terminal 404 and
unchanged terminal admission/close behavior; multiple dashboards' distinct
port cookies; default loopback no-cookie usability.

Use real loopback sockets bound through a wildcard listener to exercise remote
policy on CI without relying on an external network or credentials. Unit-test
non-loopback authority cases using constructed local SocketAddr values; exercise
IPv6 where available and report a platform skip explicitly. Existing fixtures
must keep WorkDir/temp directories isolated and join server threads; do not
spawn orphan dashboards. Tests should distinguish authentication refusal from
later route/session refusal, including the terminal protocol's post-upgrade
close path. No implementation worker runs repository verification.

Orchestrator acceptance commands from repository root:

```sh
cargo test --manifest-path loom/Cargo.toml --lib commands::status::web -- --test-threads=1
cargo test --manifest-path loom/Cargo.toml --test e2e web_terminal -- --test-threads=1
cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings
```

Final integration uses the repository's full required checks once, plus a
scratch-workspace CLI smoke showing `--web 0 --host 0.0.0.0` prints the chosen
port and tokenized URL, refuses no-cookie GET, bootstraps, and serves status
with the cookie. Do not expose a real user's live configuration for this smoke.
