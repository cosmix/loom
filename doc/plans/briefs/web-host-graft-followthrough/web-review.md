# Web and CLI pressure review

Author disposition: both findings below were incorporated. The exact shutdown
test is now W-owned, token format is fixed at 64 lowercase hex characters, and
the E2E fixture migration is explicit. Findings retain the reviewed draft evidence.

Reviewed `PLAN-web-host-graft-followthrough.md` and web/CLI/Luna briefs against
current source; rechecked the changing plan before this report. Review scope is
the web/CLI seam. No runtime verification or implementation edits performed.

## Blocks execution: direct terminal admission test lacks a write owner

Anchor: web brief, Required reads and write ownership / Chosen contracts.
The brief explicitly requires passing resolved access policy into terminal
admission and retaining that leaf's independent gate. The ownership search
omits a direct leaf consumer:
`loom/src/commands/status/web/terminal/tests_upgrade/shutdown.rs:62` calls
`web::terminal::handle_upgrade(server, &head, &base, Some(&lane), &running, &limits)`
directly. At :56 it constructs `TerminalLane` directly too. It bypasses
connection.rs deliberately to prove shutdown refuses before spawning a child.
The web worker owns only the parent `terminal/tests_upgrade.rs`, not this child.
The current enumerating rg excludes both TerminalLane and handle_upgrade.

Add the exact shutdown.rs path to W's write set and include
`TerminalLane|handle_upgrade` in the consumer search. Preserve the test's 503
shutdown assertion after supplying its resolved loopback policy. The stage's
web glob is sufficient globally, but the stricter worker table is not.

## Under-specified: process-token format and fixture migration

Anchor: web brief, Chosen contracts.
The new public-serve preflight says reject malformed tokens without defining
format. Current production tokens are 64 lowercase hex characters from 32 random
bytes (`loom/src/commands/status/web/terminal/token.rs:7-12`), but public test
callers currently accept arbitrary strings: the E2E constant is `"e2e-token"`
(`loom/tests/e2e/web_terminal.rs:35`) and is passed into ServeOptions at :231.
If strict validation follows the production format, that existing integration
fixture stops serving before reaching the terminal protocol assertions.

Set an explicit token contract, preferably exactly 64 lowercase ASCII hex
characters, and tell W to migrate the E2E constant accordingly. The E2E path is
already owned; no ownership expansion is needed. Add wrong length/non-hex
preflight cases alongside the empty and conflicting-token cases. Do not weaken
remote token validation merely to preserve a test fixture.

## Confirmed seams

- Cargo auto-discovers `loom/tests/web_host_graft.rs`: package edition 2021 at
  `loom/Cargo.toml:4` and no `autotests = false`. Explicit test tables at :52-62
  do not disable automatic discovery. No manifest change is necessary.
- Current dispatch call is in `loom/src/cli/dispatch.rs:322`; the new
  dispatch_commands.rs gate is grounded by the CLI worker's explicit extraction
  and ownership contract, not an assumed preexisting file.
- Both ServeOptions struct-literal consumers are owned: parent terminal test
  helper (`terminal/tests_upgrade.rs:14`) and E2E fixture (:230). Preserving
  terminal_token and adding dashboard_token still requires updating each literal.
- Config route exports are direct reexports
  (`loom/src/commands/status/web/config_api.rs:36`); changing request handler
  signatures does not require editing that export file.
- Full-route auth is correctly placed before HTTP routing and both upgrades;
  config GET/POST and status WS retain explicit policy consumers in the brief.
- Bootstrap keeps the port cookie format used by existing browser clients;
  current redirect/HTTP responses already emit Cache-Control: no-store
  (`loom/src/commands/status/web/http.rs:264`). No SPA/dist migration is required.
- Existing loopback config tests deliberately use Host localhost without a port
  and Origin 127.0.0.1:7373 on an ephemeral listener
  (`loom/src/commands/status/web/tests/config_api.rs:29-40`). The brief correctly
  preserves loopback compatibility instead of applying remote exact-origin
  policy globally.
- Secure-distilled v2 has no executable metadata and supplies no required web
  interface. State confinement overlaps CLI dispatch conceptually; the plan's
  explicit serial integration restriction is appropriate. Neither sibling owns
  this web authentication implementation.
