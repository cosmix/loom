# runtime-and-dashboard-server / w2-proxy-process-remote: proxy shutdown, leader reaping, mutex poisoning, probe cache

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/runtime.md`, sections CR-06, R8, CR-31, E6.

## Findings covered
- CR-06: `ProxyHandle::drop` joins a tunnel thread that waits on an upstream which never closes, for up to `idle_timeout` (900 s).
- R8: `OwnedProcess::wait` reaps the group leader before `retire` signals the group, so the numeric group id can be reused in between.
- CR-31: five `Mutex::lock().unwrap()/expect()` sites panic after poisoning; `disable_for_this_process` also holds the latch across `eprintln!`.
- E6: three `OnceLock` caches in `remote_control.rs` keep the first answer for the life of the daemon.

## Files
Own (write), all under `loom/src/`:
- CR-06: `native_worker/proxy/mod.rs`, `native_worker/proxy/tunnel.rs`, `native_worker/proxy/tests_drop.rs` (new)
- R8: `process/owned.rs`, `process/lifecycle_tests.rs`
- CR-31: `remote_control.rs`, `process/sandbox_probe.rs`, `process/sandbox_probe_cache_tests.rs` (new), `web_server/hub.rs`
- E6: `remote_control.rs` (same file as CR-31), `remote_control/memo.rs` (new), `remote_control_tests.rs`, `commands/run/auth_preflight.rs`
Read only: `process/mod.rs` (not yours: `run_bounded_inner` calls `child.wait`), `native_worker/proxy/tests.rs` (396 lines; do not edit), `claude/auth.rs`.
Frozen (contract tests, never edit): `native_worker/proxy/drop_contract_tests.rs`.

## Changes

### 1. CR-06: close the upstream socket on handle drop
Today `Shared.live` (`proxy/mod.rs:68`) maps connection id to the client `UnixStream` only; `Drop for ProxyHandle` (line ~264) shuts down client sockets, so `splice`'s `tunnel-reverse` thread stays in `b.read` on the upstream `TcpStream` until the idle timeout.
1. `proxy/mod.rs`: change `live` to `Mutex<HashMap<u64, Live>>` with a private `struct Live { client: UnixStream, upstream: Option<TcpStream> }`. `Registration::new` inserts `Live { client, upstream: None }`. Add `Registration::attach_upstream(&self, upstream: &TcpStream) -> bool`: `try_clone()` it into `live[id].upstream`; return `false` (and clone nothing) when `shared.stop` is already set, otherwise `true`. A failed `try_clone` also returns `false`.
2. `Drop for ProxyHandle`: for each `Live` call `client.shutdown(Shutdown::Both)` and, when present, `upstream.shutdown(Shutdown::Both)`. Keep the order: stop flag, join the accept thread, sweep, join handlers, remove the socket file. Update the doc comment above it.
3. `Shared::serve(&self, client)` becomes `serve(&self, registration: &Registration, client: UnixStream)` and calls `tunnel::serve(client, &allowed, &self.limits, &self.connector, &|upstream| registration.attach_upstream(upstream))`. In `admit` (line ~214) the thread closure keeps `registration` alive and passes `&registration`.
4. `proxy/tunnel.rs`, `serve` (line ~59): add the parameter `attach: &dyn Fn(&TcpStream) -> bool`, called right after `open_upstream` returns `Ok`. When it returns `false`, `upstream.shutdown(Shutdown::Both)` and return without writing `ESTABLISHED` (this closes the race where a handler registers its upstream after the shutdown sweep). `splice` and its half-close behaviour do not change. `tunnel::serve` has one caller (`Shared::serve`); the macOS bridge calls `splice` directly (`native_worker/bridge.rs:130`) and is unaffected.

### 2. R8: keep the leader a zombie until `retire`
1. `process/owned.rs`: remove `#[cfg(target_os = "macos")]` from `wait_unreaped` (line ~90) and gate it `#[cfg(unix)]` (the module is unix only). The body reads `info.si_pid`, `info.si_status` as fields, which only macOS's `libc::siginfo_t` has; on Linux they are `unsafe` methods (`si_pid()`, `si_status()`; `si_code` is a field on both). Add a private `fn child_event(info: &libc::siginfo_t) -> (libc::pid_t, libc::c_int, libc::c_int)` returning `(pid, code, status)`, with a `#[cfg(target_os = "linux")]` body using the accessors under a `// SAFETY:` comment (the structure was filled by a successful `waitid` for a child) and a `#[cfg(not(target_os = "linux"))]` body using the fields. `wait_unreaped` uses it. Keep the `ECHILD` fallback to `try_wait`.
2. `OwnedProcess::wait` (line ~78) now calls `self.wait_unreaped(timeout)`. Update its doc: the leader stays a zombie, so its pid and group id cannot be reused until `retire` or drop reaps it.
3. `retire` (line ~144): replace `self.wait(..)` by `wait_with_deadline(&mut self.child, deadline.saturating_duration_since(Instant::now()))`, which reaps. Order is now: wait without reaping, `killpg`, reap. `expire` (line ~154) keeps `self.wait(grace)` after the `SIGTERM` (unreaped is what is wanted there).
4. Update the `kill_group` doc comment (lines ~188-191): the bounded runners now signal before reaping; the window remains only for a caller that never used `wait`.
5. Callers of `OwnedProcess::wait` (checked): `process/mod.rs:146`, `verify/criteria/executor.rs:192`, `git/runner/stream.rs:35`, `commands/ask/run.rs:126`, `native_worker/supervisor/teardown.rs:17`, `native_worker/boundary/probe_host.rs:149`. Each is followed by `retire()` or a drop, so the zombie is always reaped. Grep `\.wait(` over `loom/src` once more and report any caller that reads the pid or `try_wait`s after `wait` without retiring.
6. The test `wait_unreaped_reports_the_exit_and_leaves_the_leader_a_zombie` (`owned.rs` inline tests, line ~265): delete its `#[cfg(target_os = "macos")]` so it runs on Linux. Assertions unchanged.

### 3. CR-31: poison-tolerant locks
1. `remote_control.rs`: add `fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> { m.lock().unwrap_or_else(PoisonError::into_inner) }` (import `Mutex, MutexGuard, PoisonError` from `std::sync`; `DISABLED_FOR_PROCESS` is declared `std::sync::Mutex<Option<String>>`). Use it at the three sites (`disable_for_this_process` line ~259, `disabled_for_process` ~272, the test-only reset ~278). In `disable_for_this_process` take the guard in an inner block, record whether this call latched, drop the guard, then `eprintln!` only when it latched (the guard must not be held across the print).
2. `process/sandbox_probe.rs`, `cached_path_probe` (line ~40): `cache.lock().unwrap_or_else(PoisonError::into_inner)`; add `PoisonError` to the existing `use std::sync::{Mutex, OnceLock};` line (same line, no extra line). The file is 396 lines and must end at most 399: add exactly three lines directly after `mod host_job_lane;` (no blank line): `#[cfg(test)]`, `#[path = "sandbox_probe_cache_tests.rs"]`, `mod cache_tests;`. Check `wc -l` after `cargo fmt`.
3. `web_server/hub.rs`, `subscribe` and `publish`: replace both `.expect("hub subscribers poisoned")` by `.unwrap_or_else(PoisonError::into_inner)`.
4. Leave `commands/status/web/broadcast.rs` and other sites (not yours).

### 4. E6: re-probe when the binary or the login changes
Do this after CR-31 (same file; it uses `locked`).
1. New `remote_control/memo.rs`, declared in `remote_control.rs` as `mod memo;` (`remote_control.rs` is not a `mod.rs` file, so the child lives at `remote_control/memo.rs`):
   `pub(super) struct Memo<T> { slot: Mutex<Option<Entry<T>>> }`, `impl<T: Clone> Memo<T>` with `pub(super) const fn new() -> Self` and
   `pub(super) fn get(&self, binary: &Path, ttl: Option<Duration>, now: Instant, probe: impl FnOnce() -> T) -> T`.
   `Entry { key: Key, taken: Instant, value: T }`; `Key { path: PathBuf, len: u64, modified: Option<SystemTime> }` built from `std::fs::metadata(binary)` (a failed `metadata` gives `len 0, modified None`). `get` locks with `super::locked`, returns the stored value when the key is equal and (`ttl` is `None` or `now.saturating_duration_since(taken) < ttl`), otherwise runs `probe` while holding the lock (a second caller waits instead of probing twice), stores `Entry`, returns the value. The slot holds one entry.
2. `remote_control.rs`: replace the three `OnceLock`s:
   - `static VERSION: Memo<Option<(u64, u64, u64)>>` behind `fn cached_version(path: &Path) -> Option<(u64, u64, u64)>` (no TTL; probe `probe_claude_version`).
   - `static HELP: Memo<bool>` behind the existing name `cached_named_arg_supported(path)` (no TTL; probe `probe_named_arg_support`).
   - `static AUTH: Memo<AuthProbe>` behind `pub(crate) fn cached_stage_auth(claude_path: &Path) -> AuthProbe` (returns by value; TTL `const AUTH_TTL: Duration = Duration::from_secs(300)`; probe `stage_auth_status`).
   - `preflight` uses `cached_version` instead of `probe_claude_version`. Delete `cached_preflight_enabled`; `resolve` calls `preflight(&path).is_enabled()`. `claude_supports_remote_control` stays unmemoized. Keep `DISABLED_FOR_PROCESS` (deliberate latch).
   - Remove the now unused `OnceLock` import; fix the doc comments that say "at most once per process" (`resolve`, `resolve_invocation`, `cached_stage_auth`).
3. `commands/run/auth_preflight.rs:64`: drop `.clone()` after `cached_stage_auth(claude_path)` (it returns an owned value); fix the doc comment above `require_stage_login`.

## Interfaces shared with other workers
None. `Memo`, `locked`, `Live`, `attach_upstream` and the new `tunnel::serve` parameter are private to this worker's files. The frozen contract `drop_contract_tests.rs` only calls `proxy::start_with(&Path, &[&str], Limits, Connector) -> Result<ProxyHandle>`, unchanged.

## Tests
1. `proxy/tests_drop.rs` (new; declare `#[cfg(test)] mod tests_drop;` in `proxy/mod.rs` next to `mod tests;`; `proxy/tests.rs` stays untouched): helpers local to the file: `silent_server() -> (SocketAddr, mpsc::Sender<()>)` (accepts one connection, reads until EOF, then holds the stream until the channel fires or is dropped), a connector mapping `api.openai.com` to that address, a client that sends `CONNECT api.openai.com:443 HTTP/1.1\r\nHost: x\r\n\r\n` and reads the 200 head. Use `super::socket_dir()`, `super::SOCKET_NAME`, `start_with`, `Limits`, `Connector`, and `require_unix_socket_and_loopback!` from `crate::process::sandbox_probe`.
   Test `dropping_the_handle_returns_while_the_upstream_stays_open`: `Limits { idle_timeout: Duration::from_secs(30), ..Limits::default() }`, open the tunnel, run `drop(handle)` on a thread, `recv_timeout(5 s)` on its completion. Fails today (blocks about 30 s). Then release the silent server.
2. `process/lifecycle_tests.rs`: `wait_leaves_the_leader_unreaped_until_retire`: spawn `sh -c "exit 3"` with `process_group(0)`, `OwnedProcess::new`, `wait(10 s)` is `Some(status)` with code 3; `crate::process::process_is_zombie(pid)` and `process_start_time(pid).is_some()`; after `retire()` `process_start_time(pid)` is `None`. Fails today on Linux (reaped by `wait`).
3. `process/sandbox_probe_cache_tests.rs`: `a_panicking_probe_does_not_poison_the_cache`: a local `static CACHE: PathProbeCache = OnceLock::new();` (`use super::*`), `catch_unwind` around `cached_path_probe(&CACHE, dir, |_| panic!(..))` is `Err`, then `cached_path_probe(&CACHE, dir, |_| true)` is `true`. Fails today (poisoned `expect`).
4. `web_server/hub.rs` inline tests: `a_poisoned_hub_still_publishes_and_subscribes`: share the hub in an `Arc`, a spawned thread locks `subscribers` and panics, `join` is `Err`; then `subscribe(None)` and `publish` work and the frame arrives. Fails today (panic).
5. `remote_control_tests.rs` (204 lines today): 
   - `#[serial]` `a_poisoned_latch_still_latches`: `DISABLED_FOR_PROCESS.clear_poison()` first; a thread locks the static and panics; `disabled_for_process()` returns `false` without panicking; `disable_for_this_process("x")` latches (`true` afterwards); finish with `DISABLED_FOR_PROCESS.clear_poison()` and `reset_disabled_for_process()`. Fails today.
   - helper `fn fake_claude(dir: &Path, version: &str) -> PathBuf`: writes an executable `#!/bin/sh` script (mode 0o755) named `claude` that answers `--version` with `"<version> (Claude Code)"`, `--help` with a line containing `--remote-control [name]`, and `auth status --json` with `{"loggedIn": true, "authMethod": "claude.ai"}`.
   - a small local `PathGuard` in this file (sets `PATH` to one directory, restores the old value on drop; `claude::find_claude_path` resolves `claude` through `which`, so `PATH` is what points it at the fake).
   - `#[serial]` `resolve_follows_a_replaced_binary`: `PathGuard` on a temp dir holding `fake_claude(.., "2.1.60")`, `reset_disabled_for_process()`, an empty temp work dir (the config defaults to `Auto`): `resolve(work_dir)` is `true`; rewrite the script as `2.0.0` (different length, so a different key): `resolve(work_dir)` is `false`; rewrite as `2.1.60` again: `true`. Fails today (`cached_preflight_enabled` keeps its first answer).
   - `#[serial]` `preflight_reports_the_replaced_version`: same setup, through `preflight(&path)`: `Disabled` with a reason containing `2.0.0` after the rewrite.
   - `memo_reprobes_on_expiry_key_change_and_survives_a_panicking_probe` (no `#[serial]`, local `Memo<u32>`, injected `Instant`s): value kept before the TTL, replaced after `now + ttl + 1 s`; a `None` TTL entry is replaced when the file's length changes; a probe that panics under `catch_unwind` does not stop the next `get`.
6. Existing tests whose assertions change: none. `disable_for_this_process_latches_and_keeps_the_first_reason` keeps `DISABLED_FOR_PROCESS.lock().unwrap()` (test code, unpoisoned there).
Contract test to turn green: `native_worker::proxy::drop_contract_tests::*`.

## Proof
From `loom/`: `cargo check --lib`, `cargo test --lib native_worker::proxy`, `cargo test --lib process::owned`, `cargo test --lib process::lifecycle`, `cargo test --lib process::sandbox_probe`, `cargo test --lib remote_control`, `cargo test --lib web_server::hub`, `cargo test --lib commands::run::auth_preflight`.

## Traps
- `wait` no longer reaps: any code path that drops the `Child` out of an `OwnedProcess` without `retire` would leave a zombie; none exists today, keep it so.
- `libc::siginfo_t` fields differ between Linux and macOS; do not copy the macOS body unchanged to Linux (it does not compile).
- `Shutdown::Both` on the upstream clone ends the blocked `read` in the reverse thread; closing only the client side is today's bug.
- `attach_upstream` must not take the `live` lock while `Drop` holds it for a sweep in a way that deadlocks: `Drop` locks, shuts sockets down and releases before joining handlers (it does today).
- The latch test poisons a process-wide static: always `clear_poison()` before and after, or later tests panic.
- `fake_claude` is executed right after being written by a multithreaded test process: write, `sync_all`, close, then run; the helper retries the write once on a transient `ETXTBSY`.
- `process/sandbox_probe.rs` has no room for blank lines: stay at 399 lines or fewer.
