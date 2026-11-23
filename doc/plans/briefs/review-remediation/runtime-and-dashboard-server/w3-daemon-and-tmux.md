# runtime-and-dashboard-server / w3-daemon-and-tmux: daemon shutdown, descriptors, tmux socket teardown, lsof cwd

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/runtime.md`, sections CR-27, CR-28 (a and b), CR-29 (a and e).

You run no `git` command, touch no path under `.loom/`, and run no tests: compile with `cargo check --all-targets` from `loom/` only. You answer from the base tree: no other worker's edits are visible to you, and none of them touch your files. The orchestrator runs the tests you write. Look up every symbol with `loom map --find-all <symbol>`, `loom map --outline <file>` or `rg`; line numbers below are hints at revision `62be2a3d2`.

## Findings covered
- CR-27: `ServerThreads::join` gives each of five daemon threads its own five-second wait, so five wedged threads cost 25 s. One shared deadline is wanted. `WorkerPool::shutdown` stays unbounded on purpose.
- CR-28(a): `DaemonServer::redirect_output_to_log` discards both `libc::dup2` results and reports success.
- CR-28(b): `nonblocking_socket` in `daemon/deadline_transport.rs` creates the socket without `FD_CLOEXEC`, so a concurrent `Command::spawn` leaks it into children.
- CR-29(a): `tmux::teardown_socket` fails forever on a stale socket file whose server is gone, because `kill-server` exits non-zero and the file stays.
- CR-29(e): `get_process_cwd_macos` matches any `lsof` line containing `cwd` and splits on whitespace, so a worktree path with a space is truncated.
Left out on purpose: CR-28(c), CR-29(b), (c), (d).

## Files
Own (write), under `loom/src/`:
- `daemon/server/lifecycle.rs`, `daemon/server/lifecycle/threads.rs` (new), `daemon/server/lifecycle/fds.rs` (new), `daemon/server/lifecycle/tests.rs`
- `daemon/deadline_transport.rs`
- `orchestrator/terminal/tmux/mod.rs`, `orchestrator/terminal/tmux/viewer.rs`, `orchestrator/terminal/tmux/viewer/retire.rs`, `orchestrator/terminal/tmux/tests_socket_teardown.rs` (new)
- `orchestrator/terminal/native/pid_tracking.rs`, `orchestrator/terminal/native/pid_tracking_tests.rs`
Read only: `orchestrator/terminal/tmux/tests_spawn.rs` (397 lines, do not edit), `orchestrator/terminal/tmux/socket.rs`, `orchestrator/terminal/tmux/tests_retire.rs`.
Frozen: none.

## Changes

### 1. CR-27 and file split: `daemon/server/lifecycle.rs`
`lifecycle.rs` is 382 lines; `run_server` is listed in `loom/maintainability-baseline.txt` at 82 lines and must not grow. Move `struct ServerThreads`, its `impl`, and `fn wait_with_timeout` (bottom of the file, after `cleanup`) into a new child module `lifecycle/threads.rs`, declared in `lifecycle.rs` as `mod threads;` with `use threads::ServerThreads;`. Make the struct and its five fields `pub(super)`; `run_server` constructs it with the same literal as today and still calls `threads.token_tracker.stop()` and `threads.join()`.
In `threads.rs`:
- `const JOIN_TIMEOUT: Duration = Duration::from_secs(5);`
- `pub(super) fn join(self)` becomes `self.join_by(Instant::now() + JOIN_TIMEOUT)`; add `fn join_by(self, deadline: Instant)`.
- `pub(super) fn join_all(handles: Vec<(Option<thread::JoinHandle<()>>, &'static str)>, deadline: Instant)`: for each present handle, poll `is_finished()` in steps of at most 50 ms until `Instant::now() >= deadline`; join a finished handle, otherwise print the existing warning (`Warning: {name} thread did not terminate within timeout`) and move on. One `deadline` is shared by every handle in the call.
- `join_by` calls `join_all` with orchestrator, log_tail, status_broadcast, quota_poller in that order, and only afterwards `join_all(vec![(self.token_tracker.take_thread(), "token_tracker")], deadline)` (the tracker handle is taken after the orchestrator was waited for, as the current comment explains; keep that comment).
`WorkerPool::shutdown` and the `client_pool.shutdown()` call in `run_server` do not change.

### 2. CR-28(a): checked `dup2`
New `lifecycle/fds.rs`, declared `mod fds;` in `lifecycle.rs`: `pub(super) fn redirect_fd(source: RawFd, target: RawFd) -> io::Result<()>`: loop on `libc::dup2(source, target)`; a result below 0 returns `io::Error::last_os_error()` unless its kind is `Interrupted`, which retries. `redirect_output_to_log` calls it for fd 1 and fd 2 with `.context("redirecting daemon stdout to orchestrator.log")?` and `.context("redirecting daemon stderr to orchestrator.log")?` and keeps the `// SAFETY:`-style reasoning in a comment on the helper.

### 3. CR-28(b): `daemon/deadline_transport.rs`
In `nonblocking_socket`, directly after wrapping the raw descriptor in `OwnedFd`, call `libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC)` and return `io::Error::last_os_error()` when it is below 0 (with a `// SAFETY:` comment like its neighbours). Use `fcntl` on both platforms: macOS has no `SOCK_CLOEXEC`. A fork between `socket` and `fcntl` remains possible and matches std's own non-atomic path on macOS.

### 4. CR-29(a): `tmux::teardown_socket`
1. `tmux/viewer/retire.rs`: add `pub(in crate::orchestrator::terminal::tmux) fn remove_stale_socket(path: &Path) -> Result<bool>`: take `fs::symlink_metadata(path)` (a `NotFound` returns `Ok(true)`, any other error is returned); return `Ok(false)` unless the file type is a unix socket (`FileTypeExt::is_socket`; mandatory, because `connect()` on a regular file also answers `ECONNREFUSED` on Linux) and `metadata.uid()` equals `libc::getuid()`; return `Ok(false)` when `!has_no_listener(path)`; otherwise call the existing private `remove_dead_socket(path, &metadata)?` and return `Ok(true)` for `Some(_)` (removed, or already gone) and `Ok(false)` for `None` (a listener appeared under the start lock). Leave `has_no_listener`, `remove_dead_socket` and `retire_viewer` as they are.
2. `tmux/viewer.rs` (line ~28, next to `pub(crate) use retire::retire_viewer;`): add `pub(in crate::orchestrator::terminal::tmux) use retire::remove_stale_socket;`.
3. `tmux/mod.rs`, `teardown_socket`: when `kill_socket_server(&socket_path)` is false, call `viewer::remove_stale_socket(&socket_path)?` and return `Ok(())` when it is `Ok(true)`; otherwise keep today's `bail!("tmux kill-server failed for ..")`. Update the doc comment: a socket file that nothing listens on and that this user owns is removed under tmux's start lock; any other failed kill stays an error and keeps the socket. The two existing tests (`kill_session_retains_an_unverifiable_stale_socket`, `failed_teardown_retains_the_only_socket_handle`) use a regular file and must keep passing.
4. `tmux/mod.rs`: add `#[cfg(test)] mod tests_socket_teardown;` next to the other test modules at the bottom.

### 5. CR-29(e): `orchestrator/terminal/native/pid_tracking.rs`
Replace the body of `get_process_cwd_macos` (macOS only) with: `run_process_discovery_probe("lsof", &["-a", "-d", "cwd", "-Fn", "-p", &pid_arg], <same label>)?` followed by `cwd_from_lsof_fields(&String::from_utf8_lossy(&output.stdout))`. Add `#[cfg(any(target_os = "macos", test))] fn cwd_from_lsof_fields(text: &str) -> Option<PathBuf>`: walk the lines; a line equal to `fcwd` arms a flag; any other line starting with `f` clears it; a line starting with `n` while the flag is armed returns `Some(PathBuf::from(&line[1..]))` when the rest is not empty; no match returns `None`. Keep the file under 400 lines (343 today).

## Interfaces shared with other workers
None.

## Tests you write (the orchestrator runs them)
1. `daemon/server/lifecycle/tests.rs` (114 lines): 
   - `join_all_shares_one_deadline_across_threads`: five threads each blocked on its own `mpsc::Receiver::recv()`; `join_all` with `Instant::now() + 400 ms`; elapsed under 1 s (a per-thread deadline would take 2 s); then drop the senders so the threads end.
   - `join_all_joins_finished_threads`: a thread that has already returned is joined without waiting for the deadline (elapsed under 200 ms with a 5 s deadline).
   - `redirect_fd_reports_a_bad_descriptor_and_redirects_a_good_one`: `redirect_fd(-1, 1)` is `Err` with `raw_os_error() == Some(libc::EBADF)`; with `spare = libc::dup(2)` as target and a temp file's raw fd as source, `redirect_fd` is `Ok`, a write to `spare` reaches the temp file, and `spare` is closed at the end (never target fd 1 or 2 itself, so the test runner's output stays intact).
2. `daemon/deadline_transport.rs` inline `mod tests`: `nonblocking_socket_is_close_on_exec`: `fcntl(fd, F_GETFD)` is at least 0 and has `FD_CLOEXEC`.
3. `tmux/tests_socket_teardown.rs`: `use super::socket::tests::TmuxTmpDirGuard; use super::*; use serial_test::serial;`, both tests `#[serial]`, `TmuxTmpDirGuard::set(tempdir)`, guarded by `require_unix_socket!` from `crate::process::sandbox_probe` like `native_worker/proxy/tests.rs` does:
   - `teardown_removes_a_socket_nothing_listens_on`: bind a `UnixListener` at `socket_path_for("loom-stale-teardown")` (create the parent directory first), drop it (the file stays), `teardown_socket` is `Ok`, the path is gone.
   - `teardown_keeps_a_socket_that_still_has_a_listener`: keep the listener alive; `teardown_socket` is `Err` whose text contains `tmux kill-server failed`, and the path still exists.
4. `native/pid_tracking_tests.rs`: `cwd_from_lsof_fields_reads_the_cwd_field_whole` with `"p123\nfcwd\nn/Users/a b/work tree\nf4\nn/tmp/cwd-notes.txt\n"` giving `Some("/Users/a b/work tree")`; the same data with the `f4` entry first and `fcwd` last still finds the cwd; text without `fcwd` gives `None`.

## Proof
`cargo check --all-targets` from `loom/`, nothing else.

## Traps
- `run_server` must not get longer: the baseline file lists it at 82 lines.
- `ServerThreads` fields were private to the file; after the move they must be `pub(super)` or `run_server` stops compiling.
- `remove_stale_socket` without the `is_socket` check would delete a regular file at the socket path (the existing regular-file tests catch that).
- `teardown_socket` still returns `Ok(())` early when the path does not exist; keep that line first.
- `cwd_from_lsof_fields` must not accept a `n` line after an `f4`-style entry; the flag is cleared by any other `f` line.
