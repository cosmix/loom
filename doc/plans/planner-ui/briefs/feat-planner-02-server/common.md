# feat-planner-02-server: shared context for every worker

Plan: `doc/plans/PLAN-feat-planner-02-server.md`. It implements product sections 02 and 12 (access,
people and authoring) of `doc/product/`. Read only the spec passages your brief names.

## Rules that bind every worker

- Rust crate is `loom/` (`loom/src`, `loom/tests`). Run cargo with `--manifest-path loom/Cargo.toml`
  from the worktree root, or from `loom/`.
- 400 lines per file, 50 per function, 300 per impl/class. The exact ledger
  `loom/maintainability-baseline.txt` fails on growth AND shrinkage of a recorded row; new code goes
  in new modules. `cargo test --manifest-path loom/Cargo.toml --test maintainability` proves it.
- No placeholders, stubs, `todo!()`, or empty bodies. No backwards-compatibility shims (loom is
  unreleased): when a path moves, every importer moves with it.
- Dependencies only through `cargo add` / `bun add`. This plan adds none: `fs2`, `nix`, `libc`,
  `which`, `ctrlc`, `tungstenite`, `httparse`, `hex`, `sha2`, `serde_json` are already in
  `loom/Cargo.toml`.
- Every host-side open of a path a session can write (`doc/`, the runtime directory) goes through
  `fs::safe_read` / `fs::safe_fs` with no-follow opens, never `std::fs`
  (`doc/loom/knowledge/patterns/security-sandbox-and-hooks.md`).
- Terminal lines that print a name, path or label pass it through
  `context::untrusted::inline_safe` (`loom/src/context/untrusted.rs:123`).
- Never write a code comment, commit message or doc line that mentions an AI system.
- Record every mistake, decision and surprise with `loom memory note` / `loom memory decision`
  at once. Never Claude Code auto-memory. Never `git add`, `git commit`, `loom stage complete`.

## Interfaces fixed by this plan (other plans build on them; do not rename)

The cross-plan contract is the "server-files" section of the plan's own prose ("Interfaces
later plans consume"). Shapes there are binding: module paths, type names, function signatures,
route paths, cookie names, wire enum names. If the code forces a change, stop and report it to
the stage orchestrator instead of renaming.

## Section 12 order of checks (planning server)

For every request: (1) `Host` equals the accepted socket's address (`AccessPolicy::host_allowed`);
(2) for every `POST` and every WebSocket upgrade, `Origin` present and byte-equal to
`http://<Host>`, on loopback too; (3) on every `/api/*` path and `/ws`, the access cookie
`loom_plan_<port>` compared with `token::matches` (constant time), else 401 with
`{"error":"unauthenticated"}`; (4) `POST` content type (`application/json`, or
`text/plain; charset=utf-8` for a text route) and `Content-Length` within the route's limit,
refused before reading; (5) the route's role. Only the static build (`/`, SPA paths, `/assets/*`)
is served without the cookie. `GET /?token=<token>` with the right token sets both cookies and
redirects to `/`; a wrong token is 403; `?token=` on any other path does nothing. No
`Access-Control-Allow-*` header anywhere. Every response carries the dashboard's security headers
and CSP plus `Referrer-Policy: no-referrer`.

## Limits (section 12, "Request limits")

64 connections, of which 48 WebSockets (`limits.rs`); JSON body 8 KiB (`MAX_BODY_BYTES`);
WebSocket inbound message or frame 256 KiB (dashboard keeps 64 KiB); request path parameter
1024 bytes.
