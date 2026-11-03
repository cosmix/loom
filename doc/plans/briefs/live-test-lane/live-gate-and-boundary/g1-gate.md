# live-gate-and-boundary G1: the egress gate and the live credential

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first; its "The gate" and
"Layout" sections are the spec. The frozen contracts are `tests/live_gate_contracts.rs` and
`tests/live_gate_contracts/**` (a fake upstream and a TLS client the contract session wrote);
read them before you start, never edit them. The stage description's PUBLIC SURFACE is
authoritative for every name below. Effort: this is the security boundary of the profile; spend
it on the failure paths.

## Files (yours alone)

`src/lib.rs` (one line: `pub mod live;` in alphabetical order), `src/live/mod.rs`,
`src/live/credential.rs`, `src/live/credential_tests.rs`,
`src/live/gate/{mod.rs, ca.rs, connect.rs, upstream.rs, rewrite.rs, proxy.rs, budget.rs, log.rs,
tests.rs}`. `src/live/mod.rs` starts with a `//!` paragraph naming the live profile and declares
`pub mod credential; pub mod gate;` plus `#[cfg(test)] mod credential_tests;`. Later stages add
modules to it.

The crates were added by confinement-hardening (tokio with `rt,net,io-util,time,sync,macros`,
tokio-rustls 0.26, rustls 0.23, hyper 1 with `http1,server,client`, hyper-util with `tokio`,
http-body-util, rustls-platform-verifier 0.7, rcgen 0.14 with `aws_lc_rs,pem`, base64 0.22).
Read their sources under `~/.cargo/registry/src/index.crates.io-*/` when an API is unclear;
never guess a signature.

## `credential.rs`

- `pub enum CredentialKind { ApiKey, OauthToken }`: serde `kebab-case` (`api-key`,
  `oauth-token`), `FromStr` and `Display` with the same spellings.
- `pub struct Credential { kind, value: String }` (fields private). `Credential::new(kind, raw:
  &str) -> Result<Self>` strips exactly one trailing `"\n"` or `"\r\n"`, then refuses an empty
  value, more than 4096 bytes, or any byte outside `0x21..=0x7e` (the error names the rule,
  never the value). `kind()`, `expose(&self) -> &str` (doc: only the gate and the output scrubber
  call it), and a manual `Debug` printing `Credential { kind: .., value: "<redacted>" }`.
  `pub fn header(&self) -> (hyper::header::HeaderName, hyper::header::HeaderValue)`:
  `x-api-key: <value>` or `authorization: Bearer <value>`, with `set_sensitive(true)`.
- `pub fn credential_path(work_dir: &Path) -> PathBuf`: `work_dir.join(LIVE_CREDENTIAL_FILE)`
  (`crate::fs::permissions::state_root::LIVE_CREDENTIAL_FILE`, added by
  confinement-hardening). `work_dir` is the state root (`.loom/work`) throughout.
- `pub fn store(work_dir: &Path, credential: &Credential) -> Result<()>`: refuse a `work_dir`
  that is a symlink or not a directory; write `{"kind","value","set_at"}` (chrono RFC 3339) to
  `live-credential.tmp-<pid>` opened `O_CREAT|O_EXCL|O_WRONLY|O_NOFOLLOW` mode 0600,
  `sync_all`, rename onto `live-credential`.
- `pub fn load(work_dir: &Path) -> Result<Option<Credential>>`: `None` when the file does not
  exist; open with `O_RDONLY|O_NOFOLLOW|O_NONBLOCK` (a symlink fails with `ELOOP`: return an
  error saying the credential file must not be a symlink), `fstat` it: regular file, owner the
  effective uid, `mode & 0o077 == 0`, size at most 8 KiB; otherwise an error naming the rule.
  Parse the JSON (`deny_unknown_fields`) and rebuild through `Credential::new`.
- `pub fn clear(work_dir: &Path) -> Result<bool>`: unlink (removes a symlink itself, never its
  target); `false` when absent.
- `credential_tests.rs`: unit tests for each refusal and the newline trimming.

## The gate (`gate/`)

Public items in `gate/mod.rs` exactly as the PUBLIC SURFACE lists them, plus `pub use
upstream::Upstream;`.

- `start` binds the listener: `Listen::Unix(path)` a `std::os::unix::net::UnixListener` at
  `path` (fails if anything exists there; chmod 0600); `Listen::Loopback` a
  `std::net::TcpListener` on `127.0.0.1:0` (never `0.0.0.0`). It generates a 32-byte proxy
  secret (hex of two `uuid::Uuid::new_v4()` simple forms is enough), the CA and the leaves
  (`ca.rs`), sets the listener non-blocking, and spawns one thread named `loom-live-gate`
  running a `tokio` current-thread runtime (`enable_io`, `enable_time`). `stop` signals
  shutdown through a `tokio::sync::oneshot`, aborts in-flight connections
  (`tokio::task::JoinSet::abort_all`), joins the thread, removes a Unix socket file and returns
  the report. `Drop` for `GateHandle` does the same when `stop` was never called.
- `ca.rs`: an rcgen ECDSA P-256 CA valid 24 hours, `IsCa::Ca(BasicConstraints::Constrained(0))`,
  key usages cert-sign and crl-sign, and name constraints permitting exactly the configured
  hosts; one leaf per host (SAN = host, server-auth EKU, valid 24 hours) signed by it. The CA
  private key never leaves memory. Server config:
  `ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
  .with_safe_default_protocol_versions()?.with_no_client_auth().with_cert_resolver(..)`, ALPN
  `[b"http/1.1"]`. The resolver serves the leaf for the SNI only when it equals the CONNECT host
  of this connection; anything else records a refused line (the name sanitised to printable
  ASCII, at most 253 bytes) and returns `None`, which fails the handshake.
- `connect.rs` (pure, unit-tested in `tests.rs`): read the proxy request head with `httparse`
  (at most 8 KiB, 10 s); return `Connect { host, port }` or a refusal status: 405 for any method
  but `CONNECT`, 407 for a missing or wrong `Proxy-Authorization` (decode `Basic`
  case-insensitively, compare `loom-live:<secret>` in constant time), 403 for a host outside the
  list or a port other than 443. A 407 carries `Proxy-Authenticate: Basic realm="loom-live"`.
  Every refusal is logged and counted in `refused`; none contacts the upstream.
- `upstream.rs`: `pub struct Upstream` (Clone). `public()`: TCP to `(host, 443)` with a 10 s
  connect timeout; TLS with a rustls-platform-verifier `ClientConfig` built on the aws-lc-rs
  provider; ALPN `http/1.1`. `fixed(addr, roots)`: TCP to `addr` for every host, verified with
  `roots`. Either way the SNI is the CONNECT host and verification is never disabled.
- `rewrite.rs` (pure, unit-tested): given the request head and the CONNECT host, refuse an
  `upgrade` header (400) and a missing or different `Host` (compare ASCII case-insensitively
  after removing a `:443` suffix; 421 Misdirected Request); then remove `authorization`,
  `x-api-key`, `proxy-authorization` and the hop-by-hop headers (`connection`, `keep-alive`,
  `te`, `trailer`, `transfer-encoding`, `upgrade`, and every name listed in `connection`), and
  insert `credential.header()`.
- `proxy.rs`: accept loop with a `tokio::sync::Semaphore` of 32 (a connection beyond it is
  closed and logged as refused); per connection: the CONNECT head (`connect.rs`), `200
  Connection Established`, a 10 s TLS handshake timeout, then
  `hyper::server::conn::http1::Builder` (header read timeout 30 s) serving `service_fn`. Per
  request: budget check first (over `max_requests`: 429 with the plain-text body "loom live:
  request budget of N exhausted", no upstream contact), then `rewrite`, then one fresh upstream
  connection (`hyper::client::conn::http1::handshake`, driver spawned on the JoinSet), request
  body wrapped in `http_body_util::Limited` at `max_request_bytes`, response streamed back
  through `budget::CountingBody`. An upstream connect or TLS failure answers 502 with a body
  naming the class of failure ("upstream TLS verification failed", "upstream unreachable"),
  never upstream bytes.
- `budget.rs`: shared atomic counters; `CountingBody<B>` adds each data frame's length to the
  run total, ends the body with an error once the total passes `max_response_bytes` (sets
  `budget_exhausted`), and finalises the request's log line when the body ends or drops.
- `log.rs`: the format in common.md.

## Traps

- Never `ServerConfig::builder()` or `ClientConfig::builder()` (two providers in the lock).
- Do not buffer bodies: a buffering gate stalls every streamed turn. The contract
  `gate-streams-the-response-before-it-ends` holds the upstream's second chunk until the client
  saw the first.
- The gate thread owns the runtime; nothing outside it blocks on async code.
- Unix-socket and loopback tests run only under `loom check`; your session's sandbox refuses
  both.
- `cargo fmt`; your one check: `cargo test --lib live:: --no-run`.

## Report

Files changed, the rustls/rcgen/hyper API choices you made where the brief left a choice, and
anything in the frozen contracts you believe is wrong (do not edit them; say why).
