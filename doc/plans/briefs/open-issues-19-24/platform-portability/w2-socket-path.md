# W2: socket path resolution and classification (issue #23)

Read `doc/plans/briefs/open-issues-19-24/common.md` first. You are a sonnet worker of stage
`platform-portability`. Plan Decision 5 is binding.

## Role and issue

On a worktree, every client joins `orchestrator.sock` onto the worktree's `.loom/work` symlink
spelling. That path is `R + stage id + 41` bytes; past 103 bytes (macOS) the connect fails with
`InvalidInput` before the syscall. `rpc::try_send_request` maps that to a hard error, and the
completion broker has its own copy of the connect that reports `daemon_transport`, so completion
ends `verified_pending_ack`. Fix: one resolver, used by every client, and a path that cannot fit
classifies as `DaemonReach::Unreachable`.

## Files owned (the plan's W2 row, package-relative to `loom/`)

`src/daemon/socket.rs` (new), `src/daemon/rpc.rs`, `src/daemon/rpc_tests.rs` (exists at the base),
`src/commands/stage/control_complete.rs`, `src/commands/stage/tests/control_complete.rs`,
`src/daemon/server/core.rs`, `src/daemon/server/shutdown.rs`,
`src/commands/status/ui/tui/daemon_client.rs`, `src/commands/status/web/broadcast.rs`,
`src/commands/repair/daemon_checks.rs`, `src/verify/review/observer.rs`,
`src/commands/init/execute.rs`, `src/commands/init/execute/tests.rs`,
`src/commands/status/ui/tui/app.rs`. Write nothing else.

## Pinned interfaces (common.md, quoted)

- "`pub const SOCKET_FILE: &str = "orchestrator.sock";`"
- "`pub const SUN_PATH_MAX: usize = 104;` (moved from
  `loom/src/daemon/server/lifecycle/socket_limit.rs`, which W1 deletes)"
- "`pub fn socket_path_fits(path: &Path) -> bool` (byte length strictly below `SUN_PATH_MAX`)"
- "`pub fn socket_path(work_dir: &Path) -> PathBuf` (canonicalized `work_dir` joined with
  `SOCKET_FILE`; on canonicalisation failure, the given spelling joined with it)"
- "`pub fn socket_path_problem(work_dir: &Path) -> Option<String>` (None when
  `socket_path(work_dir)` fits; else a message with the byte count, `104` and the path, and the
  advice to move the repository to a path of at most 74 bytes)"
- "The daemon's own bind keeps `work_dir.join(SOCKET_FILE)`; every client uses `socket_path`."
- W1 declares `mod socket;` and the `pub use` in `src/daemon/mod.rs` and calls
  `socket_path_problem` from `prepare_background_run` (`src/commands/run/mod.rs`), right after
  `work_dir.load()?` and before `mark_plan_in_progress`. You never edit `daemon/mod.rs` or
  `commands/run/mod.rs`; the run refusal is W1's, not yours. W1 also replaces PR #25's
  `pub(crate) use rpc::socket_path;` in `daemon/mod.rs` with those lines, which is why the
  `socket_path` you delete from `rpc.rs` stays reachable as `crate::daemon::socket_path`.

## Root cause (re-verified at the base: PR #25 head `11859505`)

PR #25 patched part of this; the plan finishes it with one resolver in `daemon/socket.rs`.

- `src/daemon/rpc.rs:70-75` is `pub(crate) fn socket_path(work_dir)`: the canonicalized `work_dir`
  joined with `orchestrator.sock` (the pinned semantics), re-exported by `src/daemon/mod.rs:10`
  (`pub(crate) use rpc::socket_path;`, W1's file). `try_send_request` (`:160`) maps every
  `InvalidInput` from the connect to `Unreachable` through a blanket arm (`:177-179`, doc bullet
  `:152-154`). That arm also swallows an `InvalidInput` that is a real error (a path with an
  interior NUL fails the same way), so the plan replaces it with a length pre-check.
- `src/commands/stage/control_complete.rs:72-81` keeps a private `send_request` that calls
  `crate::daemon::socket_path(work_dir)` (PR #25) and then dials with `UnixStream::connect` and no
  classification.
- `src/daemon/rpc_tests.rs` already exists (the PR moved the inline tests there, assertion lines
  verbatim) and holds two PR tests: `a_work_dir_symlink_past_the_socket_path_limit_still_reaches_the_daemon`
  (`:168`, sandbox-guarded) and `a_socket_path_too_long_even_resolved_is_unreachable` (`:201`).
- Other clients still join the raw spelling: `daemon/server/core.rs:116` (`check_status`),
  `daemon/server/shutdown.rs:24`, `status/ui/tui/daemon_client.rs:17` (via `tui/app.rs:118`),
  `status/web/broadcast.rs:229`, `repair/daemon_checks.rs:165`, `verify/review/observer.rs:70`
  (message text only).
- `completion_evidence.rs:196-204`, `state_relay.rs:37`, `contracts.rs:99`,
  `dispute_transport.rs:221`, `observer.rs:101` go through `rpc::try_send_request` and are fixed by
  the resolver inside it; do not edit them.

## Tasks (anchored by symbol)

1. **`daemon/socket.rs`** (about 60 lines plus tests): the constants and three functions above.
   `socket_path`: `work_dir.canonicalize().unwrap_or_else(|_| work_dir.to_path_buf()).join(SOCKET_FILE)`
   (the body of the PR's `rpc.rs` `socket_path`, joined with `SOCKET_FILE`; second precedent
   `control_complete.rs:46-48`). `socket_path_problem` message, exactly this shape:
   `daemon socket path '<p>' is <N> bytes; AF_UNIX paths must be under 104 bytes. Move the
   repository to a path of at most 74 bytes.` Derive 74 from a const
   (`SUN_PATH_MAX - 1 - ".loom/work/orchestrator.sock".len() - 1`).
2. **`rpc.rs`**: move the body of the PR's `pub(crate) fn socket_path` (`:70-75`, with its doc
   comment) into `daemon/socket.rs` (Task 1) and delete it from `rpc.rs`, so `rpc.rs` keeps no
   `fn socket_path` of its own (acceptance: `rg -q -F 'fn socket_path' src/daemon/rpc.rs` must find
   nothing, so name no helper `socket_path` there). Import `use super::{socket_path,
   socket_path_fits};` and call `socket_path_fits(` before connecting (the plan's wiring check greps
   that literal). In `try_send_request`, after the `lstat` match and before the connect, return
   `Ok(DaemonReach::Unreachable)` when `!socket_path_fits(&socket_path)`. Delete the PR's blanket
   `Err(e) if e.kind() == ErrorKind::InvalidInput => return Ok(DaemonReach::Unreachable)` arm, its
   comment, and the matching doc bullet (`:152-154`); any other `InvalidInput` falls to the final
   `Err(e)` arm and stays an error. Order is binding: lstat `NotFound` gives `NotListening`; any
   other lstat error gives `Unreachable`; a path that does not fit gives `Unreachable`; then the
   connect mapping. Rewrite the doc comment "lives here, and only here" (`:141`) so it is true after
   your change (the daemon's own status/stop/TUI/web clients classify for themselves), describe the
   length pre-check in place of the `InvalidInput` bullet, add the too-long case to the
   `Unreachable` variant doc and to the `send_request` `Unreachable` message (`:197-203`).
3. **Tests file**: the PR already moved the inline tests to `src/daemon/rpc_tests.rs` and wired
   `#[cfg(test)] #[path = "rpc_tests.rs"] mod tests;` at the end of `rpc.rs`; there is nothing to
   move. Keep every existing line, the PR's two tests included (both pass under the pre-check: the
   first resolves to a short real path, the second hits the pre-check). `rpc_tests.rs` does
   `use super::*;`, so `socket_path` must stay in scope in `rpc.rs` (the import in Task 2) or the
   file must import it itself.
4. **`control_complete.rs`**: delete `send_request` (the PR's one-line change to it,
   `crate::daemon::socket_path(work_dir)`, goes with it); `request_completion` calls
   `crate::daemon::send_request(work_dir, &request)` (note the argument order). Remove the now
   unused imports (`UnixStream`, `Duration`, `Context`, `read_message`, `write_message`); keep
   `Request`, `Response`, `read_user_token` (the tests file does `use super::*;`).
5. **Other clients**: `core.rs:116` and `shutdown.rs:24` use `crate::daemon::socket_path(work_dir)`;
   `core.rs:93` and `:194` use `SOCKET_FILE` (the bind path keeps `join`). `daemon_client::connect`
   keeps its `&Path` parameter but resolves first:
   `let socket_path = crate::daemon::socket_path(socket_path.parent().unwrap_or(Path::new(".")));`
   and reads the token from that resolved parent. `status/web/broadcast.rs:229` passes
   `&crate::daemon::socket_path(work_path)`. `tui/app.rs:118` replaces its
   `work_path.join("orchestrator.sock")` with `crate::daemon::socket_path(work_path)` (one line;
   the file is 391 lines, so change nothing else there). `daemon_checks.rs:165` uses
   `socket_path(work_dir)`. `observer.rs:70` prints `crate::daemon::socket_path(&self.work_dir)`.
6. **`init/execute.rs`**: add a small helper `warn_on_long_socket_path(work_dir_path: &Path) ->
   Option<String>` that prints `println!("  {} {problem}", "!".yellow().bold())` and returns the
   message when `crate::daemon::socket_path_problem(work_dir_path)` is `Some` (the return value
   lets the test below skip stdout capture). Call it from `create_or_adopt_work_dir`, as one
   line after the `if adopting { .. } else { .. }` block and before its `Ok(())`: by then the
   directory exists (`adopt_existing` or `initialize` ran), so `socket_path` canonicalizes even
   the relative root `WorkDir::new(".")` can return, against the cwd that `execute` already
   treats as the repo root. Never touch `execute` itself: it is ledgered at 110 lines in
   `loom/maintainability-baseline.txt` and must not grow by a single line. `create_or_adopt_work_dir`
   is not ledgered. The file is 350
   lines; stay under 400. Add a test to `execute/tests.rs` (new lines only) that
   `warn_on_long_socket_path` reports a problem for a work root whose socket path is past the
   limit and nothing for a short one.

## Tests to write (exact names)

- `src/daemon/socket.rs` inline `mod tests`: `socket_path_resolves_a_symlinked_state_root`
  (a TempDir symlink to a short real dir resolves to the real dir joined with `SOCKET_FILE`),
  `an_unresolvable_root_keeps_the_given_spelling`,
  `socket_problem_names_bytes_limit_and_path`,
  `socket_problem_is_none_for_a_short_root`, `socket_problem_measures_the_resolved_path` (a long
  symlink spelling over a short real root gives `None`).
- `src/daemon/rpc_tests.rs`, appended after the existing tests (all new lines). The PR's two tests
  already cover the too-long path (`a_socket_path_too_long_even_resolved_is_unreachable`) and a
  long symlink spelling over a short real root
  (`a_work_dir_symlink_past_the_socket_path_limit_still_reaches_the_daemon`); write no test that
  repeats them. Add the real worktree layout, which they do not cover:
  `a_worktree_spelling_past_sun_path_is_answered`: bind `tmp/r/.loom/work/orchestrator.sock`, create `tmp/r/.worktrees/<80 x 'a'>/.loom/` and symlink
  `work` there to `../../../.loom/work`, assert the link spelling's socket path is at least 108
  bytes, serve one `Pong` from a thread exactly as `a_live_listener_is_answered` does, expect
  `Answered(Pong)`. Guard the bind with
  `crate::process::sandbox_probe::skip_unless(unix_socket_bindable(..), "<test path>", "<why>")`.
- `src/commands/stage/tests/control_complete.rs`, appended:
  `request_completion_reaches_the_daemon_through_a_long_worktree_spelling` (same layout as above,
  pattern of `reads_the_user_token_through_a_symlinked_work_dir`; the listener thread reads one
  `Request` and replies `Response::Ok`; assert `request_completion(...)` is `Ok`); sandbox-guarded.
- `src/commands/repair/daemon_checks.rs` `mod tests`, appended:
  `a_daemon_child_command_line_is_a_loom_run_command_line`: `is_loom_run_cmdline("/usr/local/bin/loom
  run --daemon-child /repo/.loom/work")` is true, and an unrelated `loom runner` line is false.

## Patterns to copy

`rpc_tests.rs` `:140-166` (`a_live_listener_is_answered`: real listener, shut down before drop, sandbox guard);
`control_complete.rs` tests `:43-63` (symlinked work dir). Do not copy `core.rs` `check_status`'s
`UnixStream::connect(&socket_path)` shape into new code: classification belongs in
`try_send_request`.

## Traps

- Acceptance greps: `rg -q -F 'UnixStream::connect' src/commands/stage` must find nothing. Do not
  write that text in code, tests or comments anywhere under `src/commands/stage` (tests may bind a
  `UnixListener`; they never dial with `UnixStream::connect`, use `try_send_request` or
  `request_completion`).
- Knowledge, `mistakes/live-state-pollution.md` and `mistakes/detached-spawn-in-tests.md` (cited in
  common.md): tests use TempDirs only, never the live `.loom/work`, never a surviving process.
  A Linux stage sandbox denies `AF_UNIX` socket creation: every bind or dial test must be guarded by
  `skip_unless`, or it passes for the wrong reason.
- `daemon/server/tests.rs:18` expects the bind path unresolved; never canonicalize the daemon's own
  `socket_path` field.
- `commands/init/execute.rs` `execute` is ledgered at 110 lines and must not change at all
  (Task 6); the warning is called from `create_or_adopt_work_dir`.
- Existing assertion lines are never edited. Moving them verbatim is allowed.
- `rpc.rs` is 219 lines at the base and shrinks (the `socket_path` body leaves); `rpc_tests.rs` is
  212 lines and grows by one test; both stay under 400.
- Completion is never spooled (no `StageRequest` completion variant exists; spooling would be
  forgeable). Do not add one.

## The one check

None before the wave returns (the crate does not compile until W1 and W5 finish).

## Report

Files changed; any client you could not move to `socket_path` and why; the `tui/app.rs:118` note;
deviations from the pins.
