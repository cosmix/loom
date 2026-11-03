# live-profile P1: the run root, HOME seeding, one entry run, the forwarder

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first; "Layout", "The
profile's environment and HOME", "The gate" and "Entry verdict" are the spec. The frozen
contracts are `tests/live_profile_contracts.rs` and `tests/live_profile_contracts/**`; read
them first, never edit them. The stage description's PUBLIC SURFACE is authoritative. You run
in parallel with P2 (the `loom live` CLI) and S1 (the `live:` field); P2's `loom live exec`
calls your `forward::serve`, and the contracts run it through `CARGO_BIN_EXE_loom`, so keep
the names exact.

## Files (yours alone)

`src/live/mod.rs` (add `pub mod entry; pub mod forward; pub mod profile; mod run_root; mod
verdict;` and re-export `run_root::RunRoot` and `verdict::verdict`), `src/live/profile.rs`,
`src/live/run_root.rs`, `src/live/entry.rs`, `src/live/verdict.rs`, `src/live/forward.rs`,
`src/live/tests_profile.rs` (declared `#[cfg(test)] mod tests_profile;`).

## What you compose (merged by live-gate-and-boundary)

- `crate::live::gate::{start, GateConfig, Listen, Budget, Endpoint, GateHandle, GateReport,
  Upstream, ANTHROPIC_HOSTS}` and `crate::live::credential::{Credential, CredentialKind}`.
- `crate::verify::criteria::{run_live_with_timeout, LiveBoundary, GateMount, CommandSpec,
  CriterionResult, zero_test_failure}` (`zero_test_failure` is `pub(crate)`).
- `crate::process::sandbox_probe::{Facility, PROFILE_ENV, PROFILE_MARKER, PROFILE_LINE_PREFIX}`.

## `run_root.rs`

`RunRoot::create() -> Result<RunRoot>`: parent `std::env::temp_dir().join(format!("loom-live-
{euid}"))`, created 0700 when missing; refuse it unless `symlink_metadata` says a directory
(not a symlink) owned by the euid with `mode & 0o077 == 0`. Then a fresh `<12 hex>` child
(`create_dir`, never reusing one) holding `home/`, `tmp/`, `gate/`, each 0700. `run_id()`,
`path()`, `home()`, `tmp()`, `gate_dir()`. `Drop` removes the run root
(`std::fs::remove_dir_all`, errors to `tracing::warn!`). Keep the Unix-socket path under 100
bytes (`<temp>/loom-live-<euid>/<12 hex>/gate/gate.sock`); refuse a longer one with an error
naming the limit.

## `profile.rs`

- Constants `LIVE_PROXY_PORT`, `PLACEHOLDER_API_KEY`, `PLACEHOLDER_OAUTH_TOKEN`.
- `ClaudeInstall { binary, version }` and `find_claude() -> Result<ClaudeInstall, String>`:
  `crate::claude::find_claude_path()`, `canonicalize`, refuse a file whose first two bytes are
  `#!` ("claude at <path> is a script, not the native build; the live profile binds a native
  Claude Code only"), then `<binary> --version` through `crate::process::run_bounded` (10 s),
  first stdout line trimmed as `version`.
- `seed_home(home, seed)`: every file common.md lists, written with `O_CREAT|O_EXCL|O_NOFOLLOW`
  (the home is fresh; a pre-existing file is an error). JSON through `serde_json`, never string
  concatenation. The seed carries a `CredentialKind`, never a `Credential`: the real value has
  no way into the home.
- `profile_env(home, proxy_url, run_id) -> Vec<(OsString, OsString)>`: the proxy, `NO_PROXY`,
  CA, nonessential-traffic, auto-updater and profile-marker entries in common.md's order (B1's
  boundary adds `HOME`, `RUSTUP_HOME`, `PATH` and `LOOM_TEST_FACILITIES`), plus `TMPDIR` and
  `TMUX_TMPDIR` = the run root's `tmp/` on macOS.

## `entry.rs`

`run_entry(request) -> Result<EntryReport>`, each step a helper under 50 lines:

1. `RunRoot::create()`.
2. `gate::start` with `Listen::Unix(<gate dir>/gate.sock)` on Linux and `Listen::Loopback`
   elsewhere, `hosts` = `request.hosts`, the request's credential and upstream, `Budget::default()`.
3. Proxy address: `127.0.0.1:<LIVE_PROXY_PORT>` on Linux (the forwarder listens there inside the
   runner's own network namespace), the gate's loopback port elsewhere; `proxy_url =
   handle.proxy_url(addr)`.
4. `seed_home` with the gate's `ca_pem`, the credential's kind and `request.claude`.
5. `LiveBoundary { home, tmp, gate: GateMount::UnixDir(<gate dir>) | LoopbackPort(port),
   read_only: [canonical loom_bin, claude binary when present], env: profile_env(..),
   facilities: [NetworkGate, WritableHome] + [ClaudeCode] when present }`.
6. The command: on Linux `CommandSpec::program(loom_bin, ["live", "exec", "--listen",
   "127.0.0.1:3128", "--gate", <socket>, "--", "sh", "-c", command])`; elsewhere
   `CommandSpec::shell(command)`.
7. `run_live_with_timeout(spec, tree, working_dir, timeout, &boundary)`. An `Err` (no
   bubblewrap or sandbox-exec, a spawn refused) becomes `Unavailable { Facility, "the live
   boundary could not start: <error>" }`. On macOS a result whose stderr holds `sandbox_apply:`
   with a non-zero exit is `Unavailable { Facility, "nested Seatbelt: this process already runs
   inside a Seatbelt sandbox, which refuses another" }`.
8. `stop` the gate; `verdict(command, working_dir, &result, &report, credential.expose(),
   zero_test_guard)`; replace every occurrence of the credential value in `result.stdout` and
   `result.stderr` with `[loom-live-credential]`; the run root drops.

`Err` is reserved for "could not set up" (run root, gate start, seeding). A run that ran is `Ok`
with its outcome.

## `verdict.rs`

`verdict(..) -> EntryOutcome`: common.md's order exactly. Lines are matched with
`PROFILE_LINE_PREFIX` followed by ` skip ` or ` unavailable `, in stdout and stderr. The
zero-test step calls `zero_test_failure(command, cwd, result)` only when `zero_test_guard`.

## `forward.rs`

`serve(listen: SocketAddr, gate: &Path) -> Result<Forwarder>`: bind a `std::net::TcpListener`
(fails before anything runs when the address is taken), one accept thread; per connection a
`UnixStream::connect(gate)` and two copy threads (`std::io::copy`, then `shutdown(Write)` on the
other side). A failed connect closes the client connection and writes one line to stderr.
`Forwarder::local_addr()`. No async runtime: this runs inside the sandbox as `loom live exec`.

## Traps

- `HOME` for `read_roots` must stay the account's: B1 overrides it after the roots are taken.
  Never set `HOME` in `request` or the spec yourself.
- `NO_PROXY` matters: tests reach in-process servers on `127.0.0.1`, which a proxied client
  would send to the gate and get 405.
- The run root is removed on every path, error paths included (RAII).
- Your session cannot run the contracts (sockets, bubblewrap). `cargo fmt`; your one check:
  `cargo test --lib live:: --no-run`.

## Report

Files changed, the exact `EntryOutcome` serde shape, and anything in the contracts you believe
is wrong (do not edit them; say why).
