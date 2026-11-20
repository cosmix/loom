# W1: daemon re-exec launch and readiness (issue #21)

Read `doc/plans/briefs/open-issues-19-24/common.md` first. You are the opus worker of stage
`platform-portability`. Plan Decision 4 is binding.

## Role and issue

`loom run` prints its banner on macOS and the daemon never comes up. The daemon is a double
`fork()` of a process that already ran threads, and the grandchild touches CoreFoundation, which
aborts it after the success byte went out. Replace the fork with a re-exec of the loom binary,
report a daemon that dies at startup, and delete the fork path on every platform.

## Base: PR #25

Your worktree is cut from PR #25 (`fix/macos-review-round-recording`, head `11859505`) plus the plan
commit; the line numbers below are re-read there. PR #25 added a macOS workaround for the same abort
this brief removes, `src/commands/run/objc_fork_safety.rs`; you delete it (Task 11).

## Files owned (the plan's W1 row, package-relative to `loom/`)

`src/daemon/server/lifecycle.rs`, `src/daemon/server/lifecycle/socket_limit.rs` (delete),
`src/daemon/server/lifecycle/tests.rs`, `src/daemon/server/launch.rs` (new),
`src/daemon/server/launch/tests.rs` (new), `src/daemon/server/environment.rs`,
`src/daemon/server/mod.rs`, `src/daemon/mod.rs`, `src/commands/run/mod.rs`,
`src/commands/run/daemon_child.rs` (new), `src/cli/types.rs`, `src/cli/dispatch.rs`,
`src/main.rs`, `src/orchestrator/terminal/native/detection.rs` (comments only),
`src/fs/tmux_tmpdir.rs` (comments only), `src/commands/run/objc_fork_safety.rs` (delete). Write
nothing else.

## Read first (line numbers at the PR #25 head)

- `src/daemon/server/lifecycle.rs:41-141` (`start`: pipe, two forks, setsid, env apply, flock,
  pid and tokens, log redirect), `:143-166` (`redirect_output_to_log`), `:175-225`
  (`run_server`: socket-path check, bind, success byte), `:348-363` (`rotate_log`).
- `src/daemon/server/environment.rs:1-95` (allowlists, `DaemonEnvironment`, `apply`).
- `src/commands/run/mod.rs:37-132` (`execute_background`, `prepare_background_run`,
  `run_startup_preflights`), `src/commands/run/objc_fork_safety.rs` (50 lines, PR #25).
- `src/cli/types.rs:63-88` (`Run`), `src/cli/dispatch.rs:290-310` (`dispatch_run`),
  `src/main.rs:16-33` (`UPDATE_SILENT_COMMANDS`).
- Precedents: `src/commands/hook/reconcile_graph.rs:320-360` (spawn guard and suppressed
  counter), `src/update_check/mod.rs:198-250` (re-exec through `current_exe`),
  `src/orchestrator/spawner.rs:96` (`read_log_tail(path, max_lines)`),
  `src/claude/session.rs:175-300` (tests that run `/bin/sh` children).

## Pinned interfaces (from common.md; quoted)

- "Socket (W2 writes `loom/src/daemon/socket.rs`; W1 declares it). W1's
  `loom/src/daemon/mod.rs` gets `mod socket;` and
  `pub use socket::{socket_path, socket_path_fits, socket_path_problem, SOCKET_FILE, SUN_PATH_MAX};`."
  `SUN_PATH_MAX` and `socket_path_fits` move out of `lifecycle/socket_limit.rs`, which you delete.
  The daemon bind keeps `work_dir.join(SOCKET_FILE)`.
- "Launch (W1). `loom/src/daemon/server/launch.rs`:
  `pub struct ReadyTiming { pub deadline: Duration, pub grace: Duration }`,
  `pub fn await_ready(child: &mut Child, reader: std::io::PipeReader, log_path: &Path, timing: ReadyTiming) -> anyhow::Result<()>`,
  `pub(crate) fn spawn_daemon(...) -> anyhow::Result<()>` (W1 settles its parameters), called
  from `DaemonServer::start` as `launch::spawn_daemon(`. Re-exported:
  `loom::daemon::{await_ready, ReadyTiming}`. Readiness bytes: `0x01` ready, `0x02` output now
  in `orchestrator.log`. Deadline 10 s, grace 1 s in production."
- "Daemon-child argv. `<loom> run --daemon-child <ABS_WORK_ROOT>` followed by the run's own
  config flags. The flag is `#[arg(long, hide = true)]` and conflicts with `--foreground`.
  `loom/src/commands/run/mod.rs` dispatches it as `daemon_child::execute(`."
- You consume W2's `crate::daemon::socket_path_problem(work_dir: &Path) -> Option<String>`.
  Settled here: `spawn_daemon(work_dir: &Path, config: &DaemonConfig) -> Result<()>`, which
  builds its `Command` through the pure `daemon_command(exe: &Path, work_root: &Path, config:
  &DaemonConfig, env: &DaemonEnvironment, terminal: Option<&str>) -> Command` (Design 2).
- The run refusal sits in `prepare_background_run` (Design 9). `run_startup_preflights` is never
  where it goes.

## Root cause (re-verified at the PR #25 head)

`lifecycle.rs:49` creates a pipe, `:54` forks, the parent blocks on an unbounded read (`:61`) and
`exit(0)`s on byte 1 (`:62`); PR #25 merged the parent's EOF and unexpected-byte arms into one that
prints `Daemon failed to start; see <work_dir>/orchestrator.log for the reason` (`:66-70`), which
the re-exec design replaces with `await_ready`'s error. `:82` `setsid`, `:87` second fork, `:97`
applies the env. The success byte is written at `:216-223`, before `spawn_orchestrator` (`:231`) and
`spawn_quota_poller` (`:235`). The quota poller builds a reqwest client, which reads the system
proxy through CoreFoundation; after a fork of a process that has run threads
(`process::run_bounded` reader threads, `process/mod.rs:209-221`, reached from `git_preflight`
and `plan_inputs`), objc aborts the grandchild. The parent already exited 0. PR #25 papered over the abort on macOS
(`commands/run/objc_fork_safety.rs`: `loom run` re-executes itself with
`OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES`); that only silences objc's check for the fork this brief
removes, and costs an extra exec of every macOS `loom run`, so Task 11 deletes it. The SAFETY
comments at `lifecycle.rs:52-53`, `:85-86`, `environment.rs:62` and the tokio claim at
`run/mod.rs:60-61` are false.

## Design (settled)

1. **Parent** (`loom run`): `DaemonServer::start` calls `ensure_private_control_dir`, then
   `launch::spawn_daemon(&self.work_dir, &self.config)` (PR #25's one-line EOF message goes with the
   fork code; failures now surface through `await_ready`). The parent never opens
   `orchestrator.log` and never takes the flock.
2. **Command**: split a pure `fn daemon_command(exe: &Path, work_root: &Path, config:
   &DaemonConfig, env: &DaemonEnvironment, terminal: Option<&str>) -> Command` from
   `spawn_daemon`. `spawn_daemon` passes `std::env::current_exe()`, `DaemonEnvironment::capture()`
   and `detect_terminal().ok()` mapped to `display_name()`; `daemon_command` never calls
   `detect_terminal` itself, so a headless CI host (no terminal emulator) can test it. It runs
   `exe` with `run --daemon-child <abs work root>` plus `--manual` when `manual_mode`, `-p <n>`
   when `max_parallel` is set, `--no-merge` when `!auto_merge`. `watch_mode` is always true in
   background and has no flag. Do not set the working directory: the parent's cwd is the repo
   root (`prepare_background_run`), and `loom repair`'s `daemons_serving` matches daemons by
   cwd. `stdin` null. One `std::io::pipe()`: `stdout(writer.try_clone()?)`, `stderr(writer)`,
   set by `spawn_daemon` after `daemon_command` returns. No `process_group`. Drop the `Command`
   after `spawn` so the parent holds no writer (otherwise EOF never arrives).
3. **Environment**: `env_clear()`, then the `DaemonEnvironment` allowlist, then `LOOM_TERMINAL`
   from the `terminal` argument when it is `Some` (the parent still has the terminal context).
   Add `RUST_LOG`, `SCCACHE_DIR`, `SCCACHE_CACHE_SIZE`, `LOOM_SCCACHE`, `RUSTC_WRAPPER` and
   `LOOM_HOME` to `HOST_ENV_ALLOWLIST`: the daemon reads the last three (`build_cache.rs`,
   `user_config/mod.rs`) and today they are silently dropped. Replace `DaemonEnvironment::apply`
   with `pub(super) fn apply_to(self, command: &mut Command)` doing `env_clear()` plus `envs`;
   make `capture_from` `pub(super)`. Delete `apply`. Remove the `unsafe { set_var("LOOM_TERMINAL") }`
   block in `run/mod.rs:57-63`.
4. **Spawn guard** in `launch.rs`: `static SPAWN_ENABLED: AtomicBool =
   AtomicBool::new(!cfg!(test));`, `#[cfg(test)] static SUPPRESSED_SPAWNS: AtomicUsize`, and
   `pub fn disable_spawn_for_tests()` (re-exported from `daemon/mod.rs` as in the precedent). When
   disabled, `spawn_daemon` increments the counter and returns `Ok(())` before building anything.
5. **`await_ready`**: poll the pipe with `nix::poll` (the `poll` feature is on) in slices of at
   most 50 ms, `child.try_wait()` between slices. Bytes: `0x01` sets ready, `0x02` sets
   log-active, every other byte is appended to the diagnostic text (lossy UTF-8). Rules:
   - Ready and still alive after `grace`: `Ok(())`.
   - Child exits before `0x01`, or exits inside `grace` after it: `Err` naming the exit
     (`exit status N`, or `signal SIGABRT` by `nix::sys::signal::Signal::try_from(n)` then
     `as_str()`, `signal N` when unknown), the diagnostic text, and, when `0x02` was seen, the last
     20 lines of `log_path` through `crate::orchestrator::spawner::read_log_tail`. Once
     `try_wait` reports the exit, read the pipe to EOF (the child's writer ends are closed, so
     the read ends) before building the error: text the child wrote just before it exited is
     still in the pipe and is kept.
   - The diagnostic text keeps at most its last 64 KiB (a const), cut at a UTF-8 boundary after
     the lossy conversion; a child that floods its pipe never grows the parent's buffer without
     bound.
   - EOF before `0x01` with the child alive: wait up to 1 s for exit, then terminate; error text
     "closed its output before it was ready".
   - Deadline: SIGTERM (`nix::sys::signal::kill`), poll `try_wait` up to 2 s, then
     `child.kill()` and `wait()`; the error contains `did not become ready` and the diagnostics.
   - After EOF with ready set, keep the grace wait on `try_wait` only (the ready fd closes after
     `0x01`, so EOF is normal).
6. **Child** (`commands/run/daemon_child.rs`, `pub(super) fn execute(work_root: &Path, config:
   DaemonConfig) -> Result<()>`): bail unless `work_root` is absolute, then
   `DaemonServer::with_config(work_root, config).serve()`. `execute_background` gains a
   `daemon_child: Option<PathBuf>` parameter and starts with
   `if let Some(root) = daemon_child { return daemon_child::execute(&root, daemon_config(..)); }`
   (once Task 11 removes the `objc_fork_safety` call, this is its first statement; leave PR #25's
   `print_log_location` and its call alone, E1 folds it into `guidance.rs` in stage 2).
   Extract `fn daemon_config(manual, max_parallel, auto_merge) -> DaemonConfig` and use it on both
   paths. `dispatch_run` destructures and passes the new field (add no lines to `dispatch`).
7. **`DaemonServer::serve`** in `lifecycle.rs` (`pub(crate)`, under 50 lines): `setsid()` (nix,
   safe); flock via `acquire_exclusive_lock` (child only); `record_daemon_binary`; remove the stale
   socket; publish pid and tokens (unchanged order); `let ready =
   std::io::stdout().as_fd().try_clone_to_owned()?` (CLOEXEC, taken before the redirect);
   `redirect_output_to_log` (drop `close(0)`; stdin is already null); write `[2u8]` to `ready`
   (ignore the error); `run_server(lock, ready)`. `run_server` takes `OwnedFd`, not `Option`.
   `start` shrinks below 50 lines.
8. **Cleanup**: `lifecycle.rs` imports `socket_path_fits`/`SUN_PATH_MAX` from `crate::daemon`;
   `lifecycle/tests.rs` keeps its `use super::{socket_path_fits, SUN_PATH_MAX}` line and assertions
   untouched (the imports in `lifecycle.rs` satisfy it). Use `SOCKET_FILE` where `lifecycle.rs`
   names `"orchestrator.sock"` (`:119`, `:329`). Drop `fork, pipe, ForkResult, close` imports.
   Rewrite the comments that claim a double fork, a daemon grandchild or a single-threaded
   daemonization path (`lifecycle.rs` umask comments, `redirect_output_to_log`, `main.rs:22-26`:
   `run` stays in `UPDATE_SILENT_COMMANDS` because the daemon child is `loom run --daemon-child`).
   After this change `rg -F 'fork()' src/daemon` must find nothing, comments included. Two more
   stale comments name symbols this change deletes: `src/cli/dispatch.rs:207-208` (`run_check`:
   "the same reasoning `daemon::server::environment::apply` relies on") and
   `src/daemon/server/lifecycle/tests.rs:2` ("see `socket_limit.rs`"; the check now lives in
   `crate::daemon::socket`). Rewrite those comment lines to name what exists (the first can
   state its own reasoning: one-shot CLI, no concurrent reader yet; edit that comment only, no
   code line of `run_check`; edit only the doc comment line of `tests.rs`, no assertion).
   `src/commands/stage/complete_cache.rs:6` carries the same stale `environment::apply`
   citation but nobody owns that file: report it, never edit it.
9. **Preflight**: in `prepare_background_run` (`run/mod.rs:87`), immediately after
   `work_dir.load()?` and before `plan_inputs::mark_plan_in_progress` (so a refusal never
   leaves the plan marked in progress), call `crate::daemon::socket_path_problem(work_dir.root())`
   and `bail!("{problem}")` on `Some`. Never in `run_startup_preflights`: `loom run --foreground`
   shares that function and binds no socket, so it must not be refused. Factor the check into a
   small `fn require_socket_path_fits(work_dir: &WorkDir) -> Result<()>` in `run/mod.rs` so the
   test below calls it directly.
10. `src/daemon/server/mod.rs`: add `mod launch;`. `src/daemon/mod.rs`: add the socket
    declarations pinned above and `pub use server::{await_ready, disable_spawn_for_tests,
    ReadyTiming};` (re-export from `server/mod.rs`). At the base `daemon/mod.rs:10` reads
    `pub(crate) use rpc::socket_path;` (PR #25): replace that line with `mod socket;` and the
    pinned `pub use socket::{socket_path, socket_path_fits, socket_path_problem, SOCKET_FILE,
    SUN_PATH_MAX};`, so `rpc::socket_path` is gone and `crate::daemon::socket_path` keeps resolving
    (W2 moves the function body into `socket.rs`).
11. **Delete PR #25's macOS fork workaround**: delete `src/commands/run/objc_fork_safety.rs`, the
    `mod objc_fork_safety;` line (`run/mod.rs:11`) and the call
    `objc_fork_safety::ensure_fork_safe_environment();` (first statement of `execute_background`,
    `:43`). It re-executes `loom run` with `OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES` to silence the
    objc abort that the fork causes; with no fork the abort cannot happen, and the re-exec costs an
    extra exec of every macOS `loom run`. The plan has an acceptance grep that no
    `OBJC_DISABLE_INITIALIZE_FORK_SAFETY` text remains under `src`, comments included.

## Tests to write (exact names)

In `src/daemon/server/launch/tests.rs` (declare with `#[cfg(test)] mod tests;` at the end of
`launch.rs`; the module path is `daemon::server::launch::tests`). Children are `/bin/sh -c`
scripts with stdin null and stdout/stderr on one `std::io::pipe()`; wrap each `Child` in a Drop
guard that kills and waits; start every script with `ulimit -c 0;`; use `exec sleep 30` so no
orphan holds the pipe. Use `ReadyTiming` values of at most 2 s.

- `a_ready_child_that_stays_alive_is_accepted`: `printf '\001'; exec sleep 30`, grace 100 ms,
  `Ok(())`.
- `an_early_exit_reports_status_and_text` (named by acceptance): `printf lockfail >&2; exit 3`;
  error contains `exit status 3` and `lockfail` (the text is written just before the exit, so
  this also proves the read to EOF after `try_wait` reports it).
- `diagnostic_text_keeps_only_the_last_64_kib`: a script that writes well over 64 KiB of filler
  to stderr, then `printf TAILMARK >&2; exit 3`; the error contains `TAILMARK`, and its length is
  under the cap plus a fixed margin for the status text.
- `ready_then_abort_names_the_signal_and_the_log_tail`: `$0` is the log path;
  `printf '\002'; echo boom >"$0"; printf '\001'; kill -s ABRT $$`, grace 2 s; error contains
  `SIGABRT` and `boom`.
- `a_silent_child_times_out_and_is_reaped` (named by acceptance): `exec sleep 30`, deadline
  200 ms; error contains `did not become ready`; afterwards `child.try_wait()` is `Ok(Some(_))`.
- `a_suppressed_spawn_is_counted_and_starts_nothing`: `spawn_daemon` on a TempDir returns
  `Ok(())` and `SUPPRESSED_SPAWNS` increased.
- `the_daemon_command_carries_flag_root_config_and_a_clean_environment`: call the pure
  `daemon_command(Path::new("/usr/bin/loom"), <root>, &config, &env, Some("kitty"))` with `env`
  from `DaemonEnvironment::capture_from([("HOME","/h"),("RUST_LOG","debug"),("SCCACHE_DIR","/s"),
  ("LOOM_ADMIN_TOKEN","x")])` and `DaemonConfig { manual_mode: true, max_parallel: Some(2),
  auto_merge: false, .. }`; assert `get_args()` equals `run --daemon-child <root> --manual -p 2
  --no-merge`, `get_envs()` has `HOME`, `RUST_LOG`, `SCCACHE_DIR`, `LOOM_TERMINAL` (value
  `kitty`) and not `LOOM_ADMIN_TOKEN`, and `format!("{command:?}")` starts with `env -i` (std
  prints that for `env_clear`). The test passes `Some("kitty")` and never calls
  `detect_terminal` (a headless CI host has no terminal emulator, so `detect_terminal()` would
  be `Err` there).
In `src/daemon/server/environment.rs` (new test fn, existing assertions untouched):
`rust_log_and_sccache_variables_are_captured`, which also captures `LOOM_SCCACHE`,
`RUSTC_WRAPPER` and `LOOM_HOME` and still drops an unlisted `LOOM_*` name.
In `src/commands/run/daemon_child.rs` (inline `#[cfg(test)] mod tests`):
- `the_daemon_child_flag_is_hidden_and_refuses_foreground`: `Cli::try_parse_from(["loom","run",
  "--daemon-child","/abs"])` is Ok; with `--foreground` it is Err; the rendered `run` help
  (`clap::CommandFactory`) does not contain `daemon-child`. (Placed here because `cli/types.rs`
  is at 391 lines and must not pass 400.)
- `a_relative_work_root_is_refused`.
- `a_socket_path_past_the_limit_refuses_the_run`: `require_socket_path_fits` on a `WorkDir` over a
  nonexistent TempDir subpath of 100 characters returns an error containing `104`. The call
  position is not testable without a repository; the review checks that
  `require_socket_path_fits` is the line right after `work_dir.load()?`.

## Patterns to copy and one to avoid

Copy `reconcile_graph.rs:320-360` for the guard and counter, and `claude/session.rs:182-298` for
`/bin/sh` child tests. Do not copy `process::run_bounded` or `update_check::spawn_refresh`
(`process_group(0)`): the daemon calls `setsid` itself, and a shared group would break
`loom stop`.

## Traps (knowledge, quoted)

- `mistakes/detached-spawn-in-tests.md`: "Guard process creation at the lowest level, the
  function that calls `Command::spawn`, never at the caller" and "cover both [cfg(test) and the
  integration targets], e.g. an `AtomicBool` defaulting to `!cfg!(test)`". No test may start a
  surviving process.
- `mistakes/daemon-singleton.md`: "`pgrep -af 'loom run'` returning more than one row is always
  wrong." The child argv stays `loom run --daemon-child ...`, so `is_loom_run_cmdline` keeps
  matching it. Taking the flock only in the child keeps one daemon per state directory.
- `mistakes/test-concurrency-and-fixtures.md`: an inherited descriptor keeps a flock alive after
  its owner closes it. The ready fd is CLOEXEC (`try_clone_to_owned`), never a plain `dup`.
- The crate denies undocumented `unsafe`; keep a `// SAFETY:` comment on the `dup2` and `umask`
  blocks and say what is true now (a freshly exec'd process, no worker thread started yet).
- `cli/dispatch.rs` `dispatch` (ledger 73), `lifecycle.rs` `start` (99; PR #25 shaved one line) and
  `run_server` (133)
  are ledgered: they may only shrink. Report each you shrank. Keep every file at or under 400
  lines (`cli/types.rs` is 391: add one field with a short doc comment only).
- `src/orchestrator/terminal/native/detection.rs:32,39,142,149` and `src/fs/tmux_tmpdir.rs:7`
  still say "before daemon fork": rewrite those comments to the re-exec truth ("before the
  daemon child is spawned"). Comment edits only; no code in either file changes.

## The one check

None before the whole wave returns: the crate does not compile until W2 and W5 finish. Do not
run cargo.

## Report

Files changed (the deleted `objc_fork_safety.rs` included); ledgered units shrunk; settled signatures (`spawn_daemon`, `daemon_command`,
`serve`, `apply_to`); deviations from the pins and why; stale comments found outside your files
(at least `src/commands/stage/complete_cache.rs:6`, which has no owner).
