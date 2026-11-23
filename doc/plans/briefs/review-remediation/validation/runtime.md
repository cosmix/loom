# Validation: native worker, process, daemon, terminal backends, layering

Tree revision `62be2a3d2`. Paths are repo-relative. Line numbers are as read in this tree.
Reviews: `doc/CODEX-CODE-REVIEW-2026-11-23.md` (R6, R7, R8, E6) and `doc/AGY-CODE-REVIEW-2026-11-23.md` (CR-06, CR-27, CR-28, CR-29, CR-31, CR-32, CR-74).

## R6 — Progress can consume the space needed to release a worker

**Verdict:** NARROWED (the defect is real; `Progress` itself is not the consumer).
**Evidence:**
- `loom/src/native_worker/store.rs:252-280` `EventLog::append` rejects any record that would pass `MAX_RECORD_BYTES` (4 MiB, `loom/src/native_worker/ids.rs:16`) with `StoreError::TooLarge`. There is no reserve: `Terminal`, `OwnershipReleased` and `OwnershipUnverified` use the same limit as every other event.
- `loom/src/native_worker/supervisor/teardown.rs:118-137` `settle` appends `Terminal` and then the ownership event. If either append fails, `settle_exit` (`teardown.rs:175-188`) returns exit 1 and prints "settle failed".
- `loom/src/native_worker/runner_recover.rs:121-161` `settle_lost_supervisor` uses the same `EventLog` and the same limit, so the daemon's recovery fails each pass and the record stays `Ownership::Held`. That blocks stage operations through `index.rs:297-318` (`phase != Terminal || ownership == Held` makes the unit active).
- `Progress` is bounded: `supervisor/mod.rs:157-160` writes it at most once per `PROGRESS_EVENT_INTERVAL_SECS` (30 s, `ids.rs:30`) inside the 1500 s deadline, so about 50 small records. The unbounded writers are `NativeEvent::ServerRequestRefused { method }` (`machine/mod.rs:187-195`, one record per provider server request, `method` up to the 1 MiB `MAX_FRAME_BYTES`), `Message` details (`machine/messages.rs:139`), and `TurnStarted`/`CancelRequested` strings. Four 1 MiB `method` strings fill the log.
- A failed `persist` (`supervisor/mod.rs:238-242`) propagates `Err` through `run_session`; `supervise_boundary` (`supervisor/mod.rs:326-334`) still retires the boundary, then calls `settle_exit` with `Outcome::Unknown`, which fails on the full log as above.
**Fix seam:**
- Change `EventLog::append` (`store.rs:252`) to apply two limits: ordinary events stop at `MAX_RECORD_BYTES - SETTLEMENT_RESERVE_BYTES`; settlement events (`Terminal`, `OwnershipReleased`, `OwnershipUnverified`) may use the full `MAX_RECORD_BYTES`. Readers (`read_events`, `store.rs:202`, via `read_regular(..., MAX_RECORD_BYTES)`) keep the full limit, so a file at the cap still reads.
- Add `NativeEvent::is_settlement(&self) -> bool` in `loom/src/native_worker/model.rs` (variants at lines 118-130) and `pub const SETTLEMENT_RESERVE_BYTES: u64 = 16_384` beside `MAX_RECORD_BYTES` in `loom/src/native_worker/ids.rs:16`.
- Truncate `reason` and `detail` of settlement events to 2 KiB at the append site so three settlement records always fit the 16 KiB reserve (floor with `str::floor_char_boundary`, already used at `store.rs:197`).
- Also cap `method` in `machine/mod.rs:193` to 256 bytes so one server request cannot cost 1 MiB. Callers (`settle`, `settle_lost_supervisor`, `runner.rs:251-260`) need no change.
- No separate settlement store: that would need new ordering rules in `index.rs::fold`.
**Regression test:** extend `loom/src/native_worker/store_tests.rs` (helpers `invocation()`, direct access to `log.bytes` as in `event_log_size_limit_does_not_poison_or_consume_sequence`, line ~299) and `loom/src/native_worker/runner_recover_tests.rs` (helpers `record()`, `tail()`, `mark_spawn_begun`). Test 1: set `log.bytes = MAX_RECORD_BYTES - SETTLEMENT_RESERVE_BYTES`; assert `append(Progress)` is `TooLarge` while `append(Terminal{..})` and `append(OwnershipReleased{..})` succeed. Test 2: fill a record with `ServerRequestRefused` events until `TooLarge`, call `settle_lost_supervisor`, assert `tail()` is `Terminal` then the ownership event. Both fail today (`TooLarge`).
**Files a fix touches:** `loom/src/native_worker/store.rs`, `loom/src/native_worker/model.rs`, `loom/src/native_worker/ids.rs`, `loom/src/native_worker/machine/mod.rs`, `loom/src/native_worker/store_tests.rs`, `loom/src/native_worker/runner_recover_tests.rs`.
**Size / tier:** S-M (about 60 lines); judgment (journal ordering and data-loss invariants).
**Overlap:** R7 (same `EventLog` and `read_events`); `loom/src/native_worker/store.rs` is edited by both, so one worker should own both.

## R7 — A torn final journal line disables automatic reclamation

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/native_worker/store.rs:202-228` `read_events` returns `Corrupt("... unterminated last line")` when the text does not end with `\n`; `EventLog::open` (`store.rs:239-250`) calls it, so no appender can reopen the log.
- `loom/src/native_worker/index.rs:200` `load_named` calls `read_events`; `stage_units` (`index.rs:260-265`) therefore yields `Err` for that unit.
- `loom/src/native_worker/runner_recover.rs:225-232` (`pass_stage`): `let Ok(state) = unit else { corrupt += 1; continue; }` skips the unit before `decide`/`supervisor_gone`/`settle_lost_supervisor` run. The only output is a "N corrupt records" warning (`runner_recover.rs:261-264`).
- Stage disposition reports `Unverified` for it (`index_tests_damage.rs::disposition_unverified_for_corrupt_events`), so the stage is blocked with an operator-only recovery message. A torn tail needs a partial `write_all` (ENOSPC, power loss); a plain SIGKILL during one small `write(2)` rarely tears, so frequency is low.
**Fix seam:**
- Add to `store.rs` `pub fn repair_torn_tail(dir: &Path) -> Result<bool, StoreError>`. It reads the raw bytes (`read_regular(dir, "events.jsonl", MAX_RECORD_BYTES)`), returns `Ok(false)` unless the text is non-empty, lacks a final `\n`, and every complete line before the last `\n` parses with contiguous `seq` (refactor the per-line check of `read_events` into a shared `parse_lines(&str) -> Result<Vec<EventRecord>>`). It writes the dropped tail to `events.jsonl.torn-<unix-secs>` with `safe_create_new_in_workdir`, then truncates `events.jsonl` to the last newline through the `open_append` handle (`store_io.rs:51-70`; `set_len` works on an `O_WRONLY|O_APPEND` fd) and `sync_all`s. A middle-line corruption stays an error.
- Call it from `runner_recover.rs` before the `stage_units` walk: for each invocation directory of the stage (use `index::list_invocations(&stage_dir(work_dir, stage_id))`, `index.rs`), when `read_claim(dir)` is `pid <= 0` or `supervisor_gone(dir, &claim, ..)` is true (the same proof `decide` uses), run `repair_torn_tail`. The normal `stage_units` pass then loads the unit and `decide`/`settle_lost_supervisor` proceed unchanged.
- Decision: preserve and truncate always (do not try to complete an unterminated but valid-looking record); `settle_lost_supervisor` re-derives a lost `Terminal` as `Outcome::Unknown`.
**Regression test:** `loom/src/native_worker/runner_recover_tests.rs` (helpers `record()`, `mark_spawn_begun`, `tail()`, `run_pass` via `Pass`). Write a record, append raw bytes `{"seq":2` with no newline to `events.jsonl`, write a `supervisor.json` claim naming a dead pid (or `tombstone(dir)`, `runner_recover.rs:318`), call `run_pass`, assert `Step::Settled` and that `read_events` succeeds with the sidecar present. Middle-corruption refusal: replace line 1 with `broken\n{...}\n`, assert `Step::Warn` and no truncation. Unit tests for `repair_torn_tail` go in `store_tests.rs` beside `read_events_rejects_unterminated_last_line` (line ~171). Fails today: the unit stays `Err`.
**Files a fix touches:** `loom/src/native_worker/store.rs`, `loom/src/native_worker/runner_recover.rs`, `loom/src/native_worker/index.rs` (only if `list_invocations` needs `pub(super)`), `loom/src/native_worker/store_tests.rs`, `loom/src/native_worker/runner_recover_tests.rs`.
**Size / tier:** M; judgment (data preservation, recovery ownership).
**Overlap:** R6 (`store.rs`, `EventLog`).

## R8 — A reaped leader's numerical process group can be reused

**Verdict:** CONFIRMED (low likelihood, already documented in the code)
**Evidence:**
- `loom/src/process/mod.rs:145-152` `run_bounded_inner` calls `child.wait(timeout)`, which is `wait_with_deadline` -> `Child::try_wait` (`loom/src/process/owned.rs:62-64, 214-228`) and reaps the leader. Only afterwards `child.retire()` (`owned.rs:132-146`) runs `kill_group` (`owned.rs:190-193`) with the saved pid.
- The window exists only if every group member is gone and the pid is reused as a new group id before the `killpg`. While any descendant survives, the pgid stays allocated, so the realistic exposure is small. `kill_group`'s doc comment (`owned.rs:184-189`) and `signal_group_id` guards (`<= 1`, own group) acknowledge it.
- The same pattern is in `loom/src/verify/criteria/executor.rs:191-196` and any `OwnedProcess::wait` caller. A WNOWAIT variant already exists: `OwnedProcess::wait_unreaped` (`owned.rs:84-122`, `waitid(P_PID, WEXITED|WNOHANG|WNOWAIT)`), compiled only for macOS and used by `native_worker/boundary/probe_mac.rs:120` and `supervisor/teardown_mac.rs:16`.
**Fix seam:**
- Remove the `#[cfg(target_os = "macos")]` on `wait_unreaped` (`waitid` and `WNOWAIT` exist on Linux; `ECHILD` already falls back to `try_wait`). Make `OwnedProcess::wait` (`owned.rs:62`) call it on unix, so the leader stays a zombie (its pid and pgid cannot be reused) until `retire` runs.
- In `retire` and `expire` (`owned.rs:144, 154`) replace `self.wait(...)` with `wait_with_deadline(&mut self.child, ...)`, which reaps. `retire` already runs `kill_group` first, so the order becomes: wait unreaped, killpg, reap.
- Callers affected by `wait` semantics: `process/mod.rs:146`, `verify/criteria/executor.rs:192`, `host_verify` (through `retire`/`try_wait`); each already calls `retire` or drops the `OwnedProcess` (Drop calls `retire`), so the zombie is always reaped. Update the `kill_group` doc comment (`owned.rs:184-189`) to say the bounded runners now signal before reaping.
**Regression test:** `loom/src/process/lifecycle_tests.rs` (`ownership_waits_for_a_child_that_exits_after_wait_begins`, line ~84, pattern with `process_group(0)`) and move the macOS-only `wait_unreaped_reports_the_exit_and_leaves_the_leader_a_zombie` (`owned.rs:257-290`) to all unix. New assertion: after `owner.wait(...)` returns `Some(status)`, `process_is_zombie(pid)` (`process/identity.rs:72`, Linux) is true and `process_start_time(pid)` is `Some`; after `retire()` the pid is gone. Fails today on Linux (leader already reaped).
**Files a fix touches:** `loom/src/process/owned.rs`, `loom/src/process/lifecycle_tests.rs`.
**Size / tier:** S-M; judgment (process ownership, signals).
**Overlap:** none in the two reviews; touches every `OwnedProcess::wait` caller (`verify/criteria/executor.rs`, `process/mod.rs`).

## E6 — A long-lived process can retain obsolete Remote Control eligibility

**Verdict:** CONFIRMED (low severity)
**Evidence:**
- `loom/src/remote_control.rs:162-165` `cached_stage_auth` (`static OnceLock<AuthProbe>`), `:244-247` `cached_preflight_enabled` (`OnceLock<bool>`, it folds the version probe and the auth verdict), `:312-315` `cached_named_arg_supported` (`OnceLock<bool>`). None is keyed; the `claude_path` argument is ignored after the first call.
- Callers in the daemon: `resolve` (`:291-307`) on every spawn (`orchestrator/core/orchestrator.rs:226`, `orchestrator/terminal/native/launch/command.rs:31`), and `loom run` seeds the auth cache once (`commands/run/auth_preflight.rs:64`).
- Mitigation that narrows the impact: a launch failure after a stale "eligible" answer is caught by the crash handler, which latches `disable_for_this_process` and retries without the flag (`orchestrator/core/crash_handler.rs:326-335`). The reverse case (login or CLI fixed after a negative probe) stays stale until a daemon restart.
**Fix seam:**
- Replace the three `OnceLock`s with one small private memo type in `remote_control.rs`: `struct Memo<T> { slot: Mutex<Option<(Key, Instant, T)>> }` where `Key` is `(PathBuf, u64 len, SystemTime mtime)` for the binary (via `std::fs::metadata` of `claude_path`). Version probe and `--help` probe: re-probe when the key changes. Auth probe: key plus a 5-minute TTL (it is a subprocess per call, and `resolve` runs per spawn). `cached_preflight_enabled` becomes `preflight(path).is_enabled()` over the two memoized parts, with no third cache.
- `cached_stage_auth` returns `AuthProbe` by value (not `&'static`); the two callers `remote_control.rs:199` and `commands/run/auth_preflight.rs:64` already clone or borrow.
- Keep `DISABLED_FOR_PROCESS` as is (deliberate latch, documented at `:249-254`).
- Interaction with CR-31: both edit `remote_control.rs`. The memo's `Mutex` must use the same poison-tolerant locking CR-31 adds (a private `fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T>` using `unwrap_or_else(PoisonError::into_inner)`). Land CR-31 first (it introduces that helper and touches lines 259/272/278), then E6 reuses it. One worker should own both.
**Regression test:** `loom/src/remote_control_tests.rs` (uses `#[serial]`, `reset_disabled_for_process`; no fake-binary helper exists yet, so add `fn fake_claude(dir, version) -> PathBuf` writing an executable `#!/bin/sh` script that echoes `"<version> (Claude Code)"` and handles `auth status` and `--help`). Assert: probe a v2.1.60 script -> enabled; rewrite it as v2.0.0 (different length/mtime) -> next call disabled; a login that goes eligible -> ineligible flips after TTL (inject the TTL/clock through the memo so the test does not sleep). Fails today (first answer sticks).
**Files a fix touches:** `loom/src/remote_control.rs`, `loom/src/commands/run/auth_preflight.rs` (only if the return type change needs it), `loom/src/remote_control_tests.rs`.
**Size / tier:** M; judgment (cache keying and TTL policy).
**Overlap:** CR-31 (same file and the same mutex helper).

## CR-06 — `ProxyHandle::drop` hangs up to 900 s after the half-close change

**Verdict:** CONFIRMED (conditional on an upstream that stays open after our FIN)
**Evidence:**
- `loom/src/native_worker/proxy/mod.rs:264-282` `Drop for ProxyHandle`: sets `stop`, joins the accept thread, calls `client.shutdown(Both)` on every `live` clone (`Shared.live: Mutex<HashMap<u64, UnixStream>>`, `mod.rs:68`; only client sockets are tracked), then joins every handler.
- `loom/src/native_worker/proxy/tunnel.rs:277-304` `splice`: `a` (client) reads 0 after the shutdown, `copy_until_closed` returns `End::Finished`, so only `b.close_write()` runs (`tunnel.rs:293-294`). The `tunnel-reverse` thread (`tunnel.rs:286-291`) stays in `b2.read` with `SO_RCVTIMEO = idle` (default `Limits::idle_timeout` 900 s, `mod.rs:47`) until the upstream sends or closes. `handle.join()` (`tunnel.rs:301-303`) waits for it. A TLS server that keeps the connection open after a FIN (or one that never replies) holds the join for `idle`.
- Existing test `drop_removes_socket_and_joins_threads` (`proxy/tests.rs:353-368`) passes only because `echo_server()` (`tests.rs:170-185`) closes on EOF.
- `tunnel::serve` is also used by the macOS bridge via `splice(stream, proxy, None)` (`native_worker/bridge.rs:130`), but in a separate process; it does not share `ProxyHandle::drop`.
**Fix seam:**
- Track the upstream socket next to the client: change `Shared.live` to `HashMap<u64, Live>` with `struct Live { client: UnixStream, upstream: Option<TcpStream> }`. `Registration::new` (`mod.rs:243`) inserts `upstream: None`; add `Registration::attach_upstream(&self, &TcpStream)` storing `try_clone()`.
- `tunnel::serve` (`tunnel.rs:60-74`) takes an `attach: &dyn Fn(&TcpStream)` called right after `open_upstream` returns; `Shared::serve` (`mod.rs:73-76`) passes a closure bound to the registration (give `serve` the `id`). After attaching, if `shared.stop` is set, close the upstream and return, which closes the register-after-sweep race.
- `Drop` shuts down both sockets of each entry with `Shutdown::Both`. This ends both copy directions at once; `splice` itself and its half-close behaviour stay unchanged.
- Rejected alternative: a stop flag polled by `splice` needs short read timeouts and a new `idle` semantics; more code, same result.
**Regression test:** `loom/src/native_worker/proxy/tests.rs`. Helpers there: `socket_dir()`, `require_unix_socket_and_loopback!`, `connector(addr, &calls)`, `proxy(path, limits)` (it hardwires `echo_server()`; add a sibling that takes the upstream address), `open_tunnel(path)`. Add `silent_server()` (accepts, reads to EOF, then holds the stream until a channel fires). Test: `Limits { idle_timeout: Duration::from_secs(30), ..Limits::default() }`, open a tunnel to `silent_server`, run `drop(handle)` on a thread, assert it finishes within 5 s (`recv_timeout`). Fails today (blocks ~30 s).
**Files a fix touches:** `loom/src/native_worker/proxy/mod.rs`, `loom/src/native_worker/proxy/tunnel.rs`, `loom/src/native_worker/proxy/tests.rs`.
**Size / tier:** M; judgment (concurrency, shutdown ordering).
**Overlap:** none; `acd31bcf0` introduced the half-close path (`tunnel.rs:274-278` doc comment describes it).

## CR-27 — `WorkerPool::shutdown` unbounded and 25 s sequential join

**Verdict:** NARROWED (pool join is bounded by design; the sequential 5 s waits are real)
**Evidence:**
- `loom/src/daemon/server/lifecycle.rs:200-209`: `shutdown_flag.store`, `token_tracker.stop()`, `client_pool.shutdown()`, `threads.join()`. The comment at `:200-201` states the pool join is deliberate ("a stage-control handler may still be changing state" before the singleton is released).
- Handlers are bounded: `loom/src/daemon/server/client.rs:23-29` `CLIENT_READ_TIMEOUT = 5 s` through `DeadlineReader` (`client.rs:204`), write timeout 5 s (`client.rs:21, 181`), and the loop re-checks `shutdown_flag` (`client.rs:257-262`). `pool.rs:46-50` drops queued jobs through `cancelling`. Existing test `stop_cancels_queued_clients_and_bounds_active_reads` (`loom/src/daemon/server/client_shutdown_tests.rs:88-130`) asserts the pool shutdown returns within 7 s with a silent partial client. The only unbounded work is `serve_stage_control` (`client_dispatch.rs:41-52`, disputes, freeze), which the comment wants completed.
- `lifecycle.rs:295-317` `ServerThreads::join` plus `:319-331` `wait_with_timeout`: each of orchestrator, log_tail, status_broadcast, quota_poller and token_tracker gets its own 5 s, so five wedged threads cost 25 s. Confirmed.
**Fix seam:**
- Do not bound `WorkerPool::shutdown`. In `ServerThreads::join` compute `let deadline = Instant::now() + Duration::from_secs(5)` once and pass it to `wait_with_timeout(handle, name, deadline)` (`lifecycle.rs:319`: loop `while !handle.is_finished() && Instant::now() < deadline`). The token tracker wait (`:312-315`) uses the same deadline.
- Callers: only `ServerThreads::join` (`lifecycle.rs:308, 314`).
**Regression test:** `loom/src/daemon/server/lifecycle/tests.rs` (it has `path_of_len`, `run_server_writes_the_ready_byte_once...`). Add a test over `wait_with_timeout`: spawn three threads that sleep 10 s, call the join logic with a 200 ms deadline (make the deadline a parameter of `ServerThreads::join_by(deadline)` so the test does not wait 5 s), assert total elapsed under 1 s. Fails today (200 ms per thread, 600 ms, or 5 s per thread with the production value).
**Files a fix touches:** `loom/src/daemon/server/lifecycle.rs`, `loom/src/daemon/server/lifecycle/tests.rs`.
**Size / tier:** S; mechanical.
**Overlap:** CR-28 (`lifecycle.rs`).

## CR-28 — Unchecked `dup2`, missing `FD_CLOEXEC`, macOS `sun_len`

Three sub-claims.

**(a) Unchecked `dup2` — Verdict:** CONFIRMED
**Evidence:** `loom/src/daemon/server/lifecycle.rs:111-117`: two `libc::dup2` results are discarded inside the `unsafe` block; the function returns `Ok(())` (`:118`), called at `:91` with `?`.
**Fix seam:** extract `fn redirect_fd(source: RawFd, target: RawFd) -> io::Result<()>` (in `lifecycle.rs` next to `redirect_output_to_log`) that returns `io::Error::last_os_error()` when `dup2 < 0`; the method calls it for fds 1 and 2 and adds `.context("redirecting daemon stdout to orchestrator.log")`. Retry `EINTR` once in the helper.
**Regression test:** `loom/src/daemon/server/lifecycle/tests.rs`: `redirect_fd(-1, 1)` is `Err` (EBADF) and `redirect_fd(open_file_fd, spare_fd)` is `Ok`. Use a `dup`ed spare fd as the target so the test runner's stdout stays intact. Fails today (no helper; the existing path swallows the error).
**Files a fix touches:** `loom/src/daemon/server/lifecycle.rs`, `loom/src/daemon/server/lifecycle/tests.rs`.
**Size / tier:** S; mechanical.

**(b) Missing `FD_CLOEXEC` — Verdict:** CONFIRMED (low impact)
**Evidence:** `loom/src/daemon/deadline_transport.rs:133-151` `nonblocking_socket` calls `libc::socket(AF_UNIX, SOCK_STREAM, 0)` and sets only `O_NONBLOCK`. Users: `daemon/rpc.rs:179`, `daemon/server/core.rs:128`, `daemon/server/shutdown.rs:38`. A concurrent `Command::spawn` in the same process inherits the socket until `exec`; the child then keeps the connection open. The project sets close-on-exec on its other raw sockets and files (`O_CLOEXEC` in `fs/daemon_state.rs:31`, `fcntl(F_SETFD, FD_CLOEXEC)` in `commands/status/web/terminal/pty.rs:34`).
**Fix seam:** in `nonblocking_socket`, after the `F_SETFL` step add `libc::fcntl(fd, F_SETFD, FD_CLOEXEC)` with the same `< 0` error return (macOS has no `SOCK_CLOEXEC`, so use `fcntl` on both platforms; the tiny fork-between race remains and matches std's own non-atomic paths on macOS).
**Regression test:** `deadline_transport.rs` `mod tests` (`:196` onward, has `saturated_writer_observes_whole_exchange_deadline`): call `nonblocking_socket()` and assert `fcntl(F_GETFD) & FD_CLOEXEC != 0`. Fails today.
**Files a fix touches:** `loom/src/daemon/deadline_transport.rs`.
**Size / tier:** S; mechanical.

**(c) macOS `sun_len` left at 0 — Verdict:** REFUTED
**Evidence:** `deadline_transport.rs:181-196` `socket_address` zeroes the struct and `connect_once` (`:49-57`) passes `size_of::<sockaddr_un>()`. XNU's `getsockaddr` overwrites `sa_len` with the passed length, and Rust's own `UnixStream::connect` does not set `sun_len` either. The `sun_len` assignment in `daemon/test_support.rs:25` is only for `bind` in a test. Not verifiable on Linux, but no known failure; do not change.
**Fix seam / test / files:** none.

**Size / tier (CR-28 overall):** S; mechanical.
**Overlap:** CR-27 (`lifecycle.rs`).

## CR-29 — Terminal backend teardown, quoting, wrapper and `lsof` bugs

Five sub-claims.

**(a) `tmux::teardown_socket` leaks stale sockets — Verdict:** CONFIRMED
**Evidence:** `loom/src/orchestrator/terminal/tmux/mod.rs:313-327` bails when `kill_socket_server` (`tmux/socket.rs:131-139`, success only on exit 0) fails. `kill-server` against a socket file whose server is gone exits 1 ("no server running"), so the dead socket file stays and every later teardown fails the same way. Callers: `tmux/backend.rs:137` (`kill_session`), `tmux/viewer/retire.rs:49`. The existing tests `kill_session_retains_an_unverifiable_stale_socket` (`tmux/tests.rs:213-233`) and `failed_teardown_retains_the_only_socket_handle` (`tmux/tests_spawn.rs:69-87`) use a regular file, not a socket, and pin the intent "retain the only handle when the kill is uncertain". A bound unix socket with no listener is different: nothing is left to control. The helpers for that case exist already: `has_no_listener` and `remove_dead_socket(path, &metadata)` (`tmux/viewer/retire.rs:54-58, 63-`), which re-probes under tmux's `StartLock`.
**Fix seam:** in `teardown_socket`, when `kill_socket_server` is false, take `symlink_metadata` of the path; if it is a unix socket (`FileTypeExt::is_socket`) owned by the current uid and `has_no_listener(path)` holds, remove it with `remove_dead_socket` and return `Ok`; otherwise keep today's bail. Widen `has_no_listener` and `remove_dead_socket` from `pub(super)`/private to `pub(in crate::orchestrator::terminal::tmux)`. A regular file keeps today's error, so both existing tests keep passing. (`connect()` on a regular file also returns `ECONNREFUSED` on Linux, so the `is_socket` check is mandatory.)
**Regression test:** `loom/src/orchestrator/terminal/tmux/tests_spawn.rs` next to `failed_teardown_retains_the_only_socket_handle` (helpers `TmuxTmpDirGuard::set`, `socket_path_for`, `#[serial]`): bind a `UnixListener` at the socket path, drop it (file stays, no listener), call `teardown_socket`, assert `Ok` and the file is gone. Fails today.
**Files a fix touches:** `loom/src/orchestrator/terminal/tmux/mod.rs`, `loom/src/orchestrator/terminal/tmux/viewer/retire.rs`, `loom/src/orchestrator/terminal/tmux/tests_spawn.rs`.
**Size / tier:** S-M; judgment (socket ownership and deletion safety).

**(b) `NativeBackend::kill_session` stops at window close — Verdict:** NARROWED
**Evidence:** `loom/src/orchestrator/terminal/native/mod.rs:331-360` returns `Ok(())` after a successful window close, with a comment that the caller must confirm exit. The orchestrator's main caller does: `orchestrator/core/event_handler/stage_takedown.rs:160-190` runs `confirm_session_gone` and keeps survivors for the next attempt, where the window is already gone, `close_window_for_terminal` returns false and the code falls to `pid_only_terminate` (`native/pid_guard.rs:53-75`, SIGTERM through `terminate_verified`). The claimed `killpg`/`SIGKILL` does not exist on that path at all (it is a SIGTERM to the verified pid). Callers that do not confirm: `commands/sessions.rs:134-137` (`loom sessions kill` prints "killed" and removes the session file), `orchestrator/core/completion_handler.rs:89`, `orchestrator/core/judge_close.rs:45`.
**Fix seam:** no change to the daemon flow. For `commands/sessions.rs:134` only, call `backend.is_session_alive(&session)` after `kill_session`, retry `kill_session` once (second pass reaches `pid_only_terminate`), and report "may still be running" instead of removing the session file when the process survives. This is a UX fix, not teardown escalation; recommend deferring it with the rest of CR-29 unless the owner wants it.
**Regression test:** `loom/src/commands/sessions.rs` tests (if any) or `loom/src/orchestrator/terminal/native/tests.rs`; low value. Not recommended as a work item.
**Files a fix touches:** `loom/src/commands/sessions.rs`.
**Size / tier:** S; needs-exploration.

**(c) Terminal.app front-window title race and non-POSIX login shells — Verdict:** split. Title race: NOT-STATICALLY-VERIFIABLE. Login-shell claim: REFUTED.
**Evidence:** `loom/src/orchestrator/terminal/emulator.rs:201-219` runs `do script "<wrapper>"` then `set custom title of front window to "<title>"`. Using `front window` rather than the window or tab `do script` created is imprecise, but the observable race needs macOS. Window lookup elsewhere is `every window whose name is "<title>"` (`native/window_ops/macos.rs:28, 180`), so any change to how the title is set changes what that match sees; unverifiable here. The shell text is only `shell_path(wrapper)`, a quoted absolute path (`emulator.rs:26-28`, `shell_words`-style escape), which works in fish and csh unless the path contains a single quote. The wrapper has its own shebang.
**Fix seam:** none recommended without a macOS run; if done, `set newTab to do script ...` then `set custom title of newTab to ...` and re-check `window_ops/macos.rs` matching.
**Regression test:** `loom/src/orchestrator/terminal/emulator/tests.rs::test_terminal_app_build_command` (line 95) asserts script text only.
**Files a fix touches:** `loom/src/orchestrator/terminal/emulator.rs` (if pursued).
**Size / tier:** S; needs-exploration (macOS only).

**(d) `wrapper.rs` writes `pid_key` without `validate_id` or `O_EXCL` — Verdict:** NARROWED
**Evidence:** `loom/src/orchestrator/terminal/native/wrapper.rs:95-135` builds `wrapper_script_path(work_dir, pid_key)` (`native/pid_tracking.rs:56-58`) and `fs::write`s it. `pid_key = "{tracking_key}-{session.id}"` (`native/mod.rs:236-243`, `native/launch.rs:332`); both parts are loom-generated, and the same launch writes the settings capsule, which runs `validate_id(session_id)` (`native/session_settings.rs:90`). Stage ids come from plan validation. No in-tree caller passes an unchecked key, so this is hardening. `O_EXCL` would break a legitimate rewrite of the same session's wrapper on a respawn path, so it is not a safe blanket change.
**Fix seam:** in `create_session_wrapper_script` (`wrapper.rs:84`) call `crate::validation::validate_id(pid_key)` first (the key charset `[A-Za-z0-9_-]` fits `loom-<stage>-<session>`); skip `O_EXCL`.
**Regression test:** `loom/src/orchestrator/terminal/native/tests_wrapper_env.rs` (calls `create_wrapper_script(...)`, line 9): a `pid_key` of `../x` returns `Err` and writes nothing.
**Files a fix touches:** `loom/src/orchestrator/terminal/native/wrapper.rs`, `loom/src/orchestrator/terminal/native/tests_wrapper_env.rs`.
**Size / tier:** S; mechanical.

**(e) `get_process_cwd_macos` substring-matches `cwd` and splits on whitespace — Verdict:** CONFIRMED
**Evidence:** `loom/src/orchestrator/terminal/native/pid_tracking.rs:317-338`: `line.contains("cwd")` matches any `lsof` line mentioning `cwd` (an open file with `cwd` in its path, the command name), then `parts[8]` takes one whitespace-split field, so a worktree path with a space is truncated and the secondary cwd check at `:303-310` fails closed (the pid is not found). The only caller is `:303`.
**Fix seam:** run `lsof -a -d cwd -Fn -p <pid>` and parse the field output: the line starting `n` after `fcwd` is the path (`PathBuf::from(&line[1..])`). Put the parser in `fn cwd_from_lsof_fields(&str) -> Option<PathBuf>` under `#[cfg(any(target_os = "macos", test))]` so Linux CI tests it. The probe goes through `run_process_discovery_probe` (already used).
**Regression test:** `loom/src/orchestrator/terminal/native/pid_tracking_tests.rs`: feed `"p123\nfcwd\nn/Users/a b/work tree\nf4\nn/tmp/cwd-notes.txt\n"`, expect `/Users/a b/work tree`. Fails today (no parser; the old logic returns `/Users/a`).
**Files a fix touches:** `loom/src/orchestrator/terminal/native/pid_tracking.rs`, `loom/src/orchestrator/terminal/native/pid_tracking_tests.rs`.
**Size / tier:** S; mechanical.

**Overlap (CR-29):** none noticed for these files in either review.

## CR-31 — Unhandled `Mutex` poisoning panics

**Verdict:** CONFIRMED; worse than described for `remote_control.rs`
**Evidence:**
- `loom/src/remote_control.rs:259` (`disable_for_this_process`), `:272` (`disabled_for_process`), `:278` (test-only reset): `DISABLED_FOR_PROCESS.lock().unwrap()`. `disable_for_this_process` holds the guard across `eprintln!` (`:262-266`), and `eprintln!` panics when stderr cannot be written (the daemon's stderr is `orchestrator.log`; a full disk is enough). A poisoned latch then makes every `resolve` call (`:291`, run on each spawn) panic through `disabled_for_process()`.
- `loom/src/process/sandbox_probe.rs:40`: `cache.lock().expect(...)` while `probe(dir)` runs under the lock (`:41-43`); a panicking probe poisons the cache for every later probe.
- `loom/src/web_server/hub.rs:28, 39`: `.expect("hub subscribers poisoned")`; the critical sections do `try_send` and `retain`, which do not panic, so poisoning there is unlikely.
- Convention: the tree already uses `unwrap_or_else(PoisonError::into_inner)` in about 20 places (for example `loom/src/native_worker/proxy/mod.rs:260`, `loom/src/plan_server/commands/owner.rs:68`, `loom/src/daemon/server/client.rs:243`); the guarded data in all three sites stays consistent after a panic (an `Option`, a map insert after the probe, a `Vec`).
**Fix seam:** replace the five `unwrap()/expect()` sites with `.unwrap_or_else(PoisonError::into_inner)` (add `use std::sync::PoisonError`). In `disable_for_this_process`, drop the guard before `eprintln!`. The same pattern remains at `loom/src/commands/status/web/broadcast.rs:113` (marked KNOWN in the review; same one-line fix) and `status/data/rounds.rs`, `plan_server/run/dashboard.rs`; include them if the worker wants one sweep, otherwise leave.
**Regression test:** `loom/src/remote_control_tests.rs` (`#[serial]`, test at line 90 shows the latch idiom): poison `DISABLED_FOR_PROCESS` from a child thread that locks and panics (`std::thread::spawn(...).join()`), then assert `disabled_for_process()` returns without panicking and `disable_for_this_process("x")` latches. `loom/src/web_server/hub.rs` inline `mod tests` (can reach the private `subscribers` field): poison, then `publish`/`subscribe` still work. `process/sandbox_probe.rs`: a local `static CACHE: PathProbeCache`, one probe closure that panics inside `catch_unwind`, then a second call returns its answer. All fail today with a panic.
**Files a fix touches:** `loom/src/remote_control.rs`, `loom/src/process/sandbox_probe.rs`, `loom/src/web_server/hub.rs`, and tests in `loom/src/remote_control_tests.rs`, `loom/src/web_server/hub.rs`, `loom/src/process/sandbox_probe.rs` (or its test module).
**Size / tier:** S; mechanical.
**Overlap:** E6 (same file, same lock helper; do CR-31 first). Duplicated per-module `lock` helpers exist (`native_worker/proxy/mod.rs:260`, `plan_server/ws/hub.rs:142`, `plan_server/commands/owner.rs:68`, `plan_server/summaries/codex_spawner/registry.rs:49`); a shared helper is a separate cleanup.

## CR-32 — Undocumented upward and circular module dependencies

**Verdict:** CONFIRMED for all five named edges; two of them are cheaper than the review suggests.
**Evidence (each `use` line found):**
- `loom/src/handoff/completion/attest.rs:14`: `use crate::commands::stage::admin_hmac::{constant_time_eq, hmac_sha256};`. The file is `loom/src/commands/stage/admin_hmac.rs` (48 lines, `pub(crate)` module at `commands/stage/mod.rs:6`; `SHA256_LEN` is `pub(super)`). Other user: `commands/stage/admin_proof.rs:6`.
- `loom/src/subagent_lifecycle/claude.rs:8`: `use crate::commands::subagents::ledger::StartedAgentTypeIndex;`. `commands/subagents/ledger.rs` is only `pub(crate) use crate::provider_data::ledger::*;`, and `tokens/discover.rs:12` already imports `crate::provider_data::ledger::StartedAgentTypeIndex`.
- `loom/src/stage_service/contracts_choices.rs:15`: `use crate::daemon::ContractRunReport;`. The type is defined in `loom/src/stage_service/mod.rs:43` and merely re-exported by `daemon/protocol.rs:93` (and `daemon/mod.rs:11`), so the "cycle" is a self-import through the daemon re-export. The reverse edge `daemon -> stage_service` (`daemon/server/contracts.rs:18`) is a legitimate downward call. Other users through the daemon path: `verify/contracts/{freeze.rs:8, store.rs:33, freeze/prepared.rs:8, test_support.rs:7}`, `host_verify/{freeze.rs:42, freeze_format.rs:10, freeze_memo.rs:13}`, `stage_service/dispute_replacement_tests.rs:11`.
- `loom/src/handoff/mod.rs:28` (`pub use crate::orchestrator::continuation::{continue_session, load_and_parse_handoff, load_handoff_content, load_handoff_v2, prepare_continuation, ContinuationConfig, ContinuationContext}`) and `loom/src/handoff/session_content.rs:21` (`use crate::orchestrator::monitor::stage_context_tokens;`, defined `orchestrator/monitor/heartbeat_store.rs:84`). The reverse edges exist: `orchestrator/continuation/context.rs:9-10`, `orchestrator/core/stage_handoff.rs:3`, `orchestrator/core/inbox_drain/complete.rs:13`, `orchestrator/monitor/completion_blockers.rs:6` import `crate::handoff`. The `handoff/mod.rs` comment says the continuation code lives in `orchestrator` because of spawner and signal dependencies.
- `loom/src/process/sandbox_probe/needs.rs:8`: `use crate::daemon::{socket_path_fits, SUN_PATH_MAX};`, defined `loom/src/daemon/socket.rs:16, 29`. `daemon` already imports `process` (lifecycle, rpc), so `process` is the lower layer.
- `models` edges: `models/stage/merge_block.rs:8` (`crate::git`), `models/forward_receipt/transcript/argv.rs:24` (`crate::assets::install`), `models/dispute.rs:15-18` (`crate::verify::*`), `models/stage/{methods,types,checks}.rs` and `models/dispute_criterion.rs:7` (`crate::plan::*`). The `crate::fs` hits in `models` are doc links (`models/constants.rs`) and a test import (`models/stage/phases_contract_tests.rs:12`), so "models -> fs" is not a production edge.
**Fix seam (where each primitive would move; no further design):**
- HMAC (`hmac_sha256`, `constant_time_eq`, `SHA256_LEN`): move `commands/stage/admin_hmac.rs` to `loom/src/utils/` (a leaf module next to `utils::sha256_hex`, which `native_worker/store.rs:15` already uses); update `admin_proof.rs:6` and `handoff/completion/attest.rs:14`.
- `StartedAgentTypeIndex`: no move; change `subagent_lifecycle/claude.rs:8` to `crate::provider_data::ledger::StartedAgentTypeIndex`. S, mechanical.
- `ContractRunReport`: no move; change `stage_service/contracts_choices.rs:15` and `dispute_replacement_tests.rs:11` to `super::ContractRunReport` (`contracts.rs:51` already does). The `verify` and `host_verify` imports can switch to `crate::stage_service::ContractRunReport`; leaving them on the daemon re-export is also acceptable.
- `SUN_PATH_MAX` and `socket_path_fits`: move to a new `loom/src/process/unix_socket.rs` (pure functions, no daemon layout knowledge); keep `socket_path`, `socket_path_problem`, `SOCKET_FILE` in `daemon/socket.rs`, which re-exports the two names so `daemon/server/lifecycle.rs:17` and `daemon/rpc.rs:20` need no edit; point `process/sandbox_probe/needs.rs:8` at the new path.
- `handoff <-> orchestrator`: the lower-layer home would be the continuation module and `stage_context_tokens` (heartbeat store reader) moved under `handoff` or a neutral module; it drags the spawner/signal code, so it is L and not part of a mechanical pass. Recommend recording it as accepted in `doc/loom/knowledge/architecture/core-abstractions.md` instead, which lists four violations that do not match the tree (it names none of the above).
- `models -> plan/verify/git/assets`: out of scope for a mechanical pass.
**Regression test:** none behavioral. A layering guard is optional: a unit test in `loom/src/` that `rg`-style scans `process/` for `crate::daemon` and `handoff/` for `crate::commands` would fail today; only add it if the plan wants enforcement.
**Files a fix touches:** `loom/src/commands/stage/admin_hmac.rs` (moved), `loom/src/commands/stage/mod.rs`, `loom/src/commands/stage/admin_proof.rs`, `loom/src/handoff/completion/attest.rs`, `loom/src/utils.rs` (module registration), `loom/src/subagent_lifecycle/claude.rs`, `loom/src/stage_service/contracts_choices.rs`, `loom/src/stage_service/dispute_replacement_tests.rs`, `loom/src/daemon/socket.rs`, `loom/src/daemon/mod.rs`, `loom/src/process/mod.rs`, new `loom/src/process/unix_socket.rs`, `loom/src/process/sandbox_probe/needs.rs`.
**Size / tier:** M for the four cheap edges together; the handoff/orchestrator cycle and `models` edges are L and not recommended now. mechanical.
**Overlap:** CR-30/orchestrator confirmed inversions listed in the review; `native_worker/proxy` has its own sun_path margin comment (`proxy/mod.rs:26`) that could reuse the moved constant.

## CR-74 — DeveloperTools, inbox capacity, worker control, goproxy

Five sub-claims (the review bundles two KNOWN items under one bullet).

**(a) `DeveloperTools::detect` caches a transient failure — Verdict:** CONFIRMED (macOS only, low severity)
**Evidence:** `loom/src/stage_cache/developer.rs:26-41`: `static TOOLS: OnceLock<Option<DeveloperTools>>`; a 10 s timeout (`LOOKUP_TIMEOUT`, `:12`) of a cold `xcrun`, or a missing Command Line Tools install, stores `None` for the daemon's life. Consumers: `stage_cache/session.rs:25`, `verify/criteria/confine_developer_tools.rs:13` (their doc says the two never disagree), `stage_cache/*` environment builders. Impact: confined builds fall back to the `/usr/bin` xcrun shims until the daemon restarts.
**Fix seam:** replace the `OnceLock<Option<..>>` with `static TOOLS: Mutex<Option<DeveloperTools>>` that caches only a successful lookup (use the poison-tolerant lock idiom); a failure is retried on the next call, throttled by a `Mutex<Option<Instant>>` last-failure stamp (60 s) so a missing toolchain does not spawn `xcrun` on every spawn. Extract `fn detect_with(cell, lookup_fn)` so tests need no macOS.
**Regression test:** `loom/src/stage_cache/contract_tests.rs` area or a new `developer_tests.rs`: with an injected lookup that fails once then succeeds, assert the second call returns `Some`. Fails today (the first `None` sticks).
**Files a fix touches:** `loom/src/stage_cache/developer.rs`, and its tests.
**Size / tier:** S; judgment (cache and retry policy).

**(b) Developer toolchain shadows Homebrew — Verdict:** NARROWED (intentional; recommend no change)
**Evidence:** `loom/src/stage_cache/environment.rs:262-283` `developer_tools` puts `Toolchains/XcodeDefault.xctoolchain/usr/bin` and `usr/bin` ahead of the host `PATH`; `stage_cache/contract_tests.rs:328-352` (`developer_tools_bypass_the_xcrun_shims`) pins exactly that order for every consumer. The purpose is to avoid the xcrun shims, so Apple's `git`, `make`, `clang` win over Homebrew copies by design.
**Fix seam:** none unless the owner changes the policy; then reorder only the `host_path` insertion and update the contract test.
**Regression test / files:** none.
**Size / tier:** n/a.

**(c) `fs/inbox/write.rs` capacity count is non-atomic — Verdict:** CONFIRMED (soft cap overshoot only)
**Evidence:** `loom/src/fs/inbox/write.rs:55-60`: `count_json_entries` (`:88-104`) then later `linkat` (`:79-88`). Concurrent relay writers (separate hook processes of one session) can each see fewer than `MAX_PENDING_ENTRIES` (128, `relay/mod.rs:47`) and all link, overshooting by at most the number of concurrent writers. Nothing downstream requires an exact 128 (`commands/hook/relay.rs:321` only reports capacity).
**Fix seam:** document the cap as soft at `write.rs:55` and at `MAX_PENDING_ENTRIES`; an exact bound would need an `flock` on the session directory around count-and-link, which adds a lock to every hook write for no demonstrated harm. Recommend the doc-only change.
**Regression test:** none (`capacity_is_refused_at_the_pending_limit`, `write.rs:276`, covers the sequential case).
**Files a fix touches:** `loom/src/fs/inbox/write.rs`, `loom/src/relay/mod.rs`.
**Size / tier:** S; mechanical.

**(d) `worker_control.rs:40` lacks a `StageStatus::Executing` check — Verdict:** CONFIRMED
**Evidence:** `loom/src/orchestrator/core/inbox_drain/worker_control.rs:34-45` calls `require_owner` (`inbox_drain/apply.rs:201-218`), which checks session status and `stage.session == record.id` but not `stage.status`. `admit_start` (`native_worker/admission.rs`) has no `StageStatus` reference. The sibling handler `end_turn` (`apply.rs:293-306`) requires `Executing` explicitly. A lead session whose stage has moved on (`NeedsHandoff`, `Blocked`, a completion state) can still admit a new native worker.
**Fix seam:** in `apply_with_backend` (`worker_control.rs:34`), after `require_owner`, refuse `WorkerControlPayload::Start` unless `stage.status == StageStatus::Executing` with a message naming the status (`Settle::Refused`). Leave `Message` and `Cancel` allowed: cancelling workers must still work while a stage winds down.
**Regression test:** `loom/src/orchestrator/core/inbox_drain/worker_control_tests.rs`: `owned_stage()` (line 18) builds an `Executing` stage; add a test with `fx.stage(StageStatus::Blocked, Some(&record.id))` and `start_payload()`, assert `Settle::Refused` and no invocation directory exists (`store::stage_dir`). A cancel on a non-Executing stage still applies. Fails today (Start is admitted).
**Files a fix touches:** `loom/src/orchestrator/core/inbox_drain/worker_control.rs`, `loom/src/orchestrator/core/inbox_drain/worker_control_tests.rs`.
**Size / tier:** S; mechanical.

**(e) `goproxy_has_userinfo` `://` quirk — Verdict:** CONFIRMED (edge case)
**Evidence:** `loom/src/stage_cache/environment.rs:46-52`: `entry.split_once("://")` splits at the first `://` anywhere in the entry, so a scheme-less entry such as `user:pw@proxy.example/x?next=https://a` has its authority read as `a` and the userinfo is missed; the credential then reaches `GOPROXY` forwarding (`environment.rs:64`, `daemon/server/environment.rs:120`). Existing test: `stage_cache/host_env_tests.rs:50` (`a_goproxy_with_userinfo_is_dropped_whichever_entry_carries_it`).
**Fix seam:** treat `://` as a scheme separator only when the text before it has no `/` or `@` (`entry.split_once("://").filter(|(scheme, _)| !scheme.contains(['/', '@']))`).
**Regression test:** `loom/src/stage_cache/host_env_tests.rs`: add `user:pw@proxy.example/x?next=https://a` to the userinfo-dropped table. Fails today.
**Files a fix touches:** `loom/src/stage_cache/environment.rs`, `loom/src/stage_cache/host_env_tests.rs`.
**Size / tier:** S; mechanical.

**Overlap (CR-74):** (a) and (b) share `stage_cache/environment.rs` and `developer.rs` with each other; (d) `native_worker/admission.rs` neighbours R5/R6 work only by area.
