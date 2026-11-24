# Host Job Test Guards

> macOS host-job denials, sandbox_probe guards, strict-CI skips

## Tests in a Host Job: What the Profile Denies and How Tests Guard

`loom check` and stage completion run loom's own suite as acceptance criteria inside a confined host job. On macOS that job runs under a Seatbelt profile (`verify/criteria/confine_seatbelt.rs`, `(deny default)` plus `process*`, `(allow signal (target same-sandbox))`, metadata reads and a read grant for the tree), so a test that needs anything else fails there although it passes in CI or an unconfined shell. About 176 tests launch a confined command; each one guards on a probe in `process/sandbox_probe.rs` and skips when the host job cannot run it.

### What a macOS host job denies

- A nested `sandbox-exec` with a profile that differs from the outer one (`sandbox_apply: Operation not permitted`, exit 71). Provision tests, `verify::before_after`, native wrapper tests and every test that confines a command hit it. `confined_commands_runnable()` starts a trivial `sandbox-exec ... true` (the macOS branch of `confined_command_blocker`; the Linux branch starts `bwrap`) and memoizes the answer; the launcher wait is bounded (`launcher.rs` polls `try_wait` for 5 s, then kills and reaps).
- Signals to a process outside the sandbox, and PTY allocation (`process_groups_signalable`, `pty_allocatable`); loopback and unix-socket bind (`loopback_bindable`, `unix_socket_bindable`); a socket path over SUN_LEN under the job's TMPDIR; writes to `/tmp` (plan locks live in `/tmp/loom-plan-locks-<uid>/`, `fs/locking.rs`, so amendment e2e tests fail there, and `apply_pending_verdicts` swallows the error into `tracing::warn`); `ps`, so the process tree is invisible (`process_tree_visible`: hook, relay and spawn-guard tests prove `LOOM_MAIN_AGENT_PID` is a live ancestor); `/dev/fd` process substitution; a read of an ancestor `bun.lock`; a TMUX_TMPDIR.
- Ancestor directories above the worktree are listable (a literal grant for each, `confine_ancestors.rs` `listing_rules`), so a tool sees the names there but reads no file except an existing ancestor `package.json` (`read_rules`); an ancestor `Cargo.toml` and `.cargo/config.toml` are hidden.

### Guard rules

- Guard BEFORE the test spawns anything. A test that signals a process or allocates a PTY asks `skip_without_group_signals`, `skip_without_process_control` or `skip_without_pty` first, through the `require_group_signals!` and `require_process_control!` macros, which keep a guarded test under the 50-line function limit (a rustfmt-expanded `let-else` with a long test name takes 4-5 lines). A test that finds a stand-in through the process table also needs `process_tree_visible()`. Never skip such a test with `cargo test --skip`: the filter runs after the binary starts, and a stand-in spawned by an unguarded test outlives the run.
- Bound every cleanup wait: a test owns each process it starts through `crate::test_stand_in` ([Stand-In Processes in Tests](#stand-in-processes-in-tests)), whose cleanup has a deadline. A refused kill followed by an unbounded `wait()` hangs the test binary for as long as the stand-in lives. `GroupAnchor::retire` (`verify/criteria/spawn_observer.rs`) still waits on its guardian without a deadline after `kill()`; see [concerns](../concerns/sandbox-and-confinement-gaps.md#host-job-test-gaps).
- A fixture that pins `HOME` (`host_verify::tests_complete_support::Fixture::new`, through `CacheEnv::pin`) needs `#[serial]` on every test using it. `CacheEnv` pins nest in any drop order: a drop re-pins the newest alive pin, and the last restores the environment from before the first (`test_cache_env.rs::Pins`). A hand-rolled save-and-restore guard restores in drop order, which a collection runs first to last, leaving `HOME` at a deleted temp dir for every later test.
- A test that depends on a hard timing bound under load is a race, not a bridge bug: spool nothing into a session after the event that may close it (`plan_session/bridge`), and wait for the state the assertion needs before answering.
- `skip_unless` prints `SKIP <test>: <why>`, and panics instead under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`. CI's two full-suite steps set it (`.github/workflows/ci.yml`), so a skip there is a failure. srt confinement probes are the exception: a host missing `bwrap`, `socat` or `srt` skips even in strict mode; only a failing `srt -c true` fails it. libtest hides a passing test's `SKIP` line, so a gate log cannot tell a run from a skip; `--nocapture` shows it. The relay dispute e2e tests (`tests/integration/relay_dispute_replacement_e2e.rs`) skip in a host job and in a session, and run in an unconfined shell and the strict CI job.

### Contracts about confinement

A contract never launches a confined command. It asserts on the policy loom generates (`verify::criteria::confined_policy`, a test-only accessor): launcher argv, profile inputs, environment. Kernel enforcement is proven by guarded non-contract tests (`stage_cache/kernel_tests.rs`). On macOS no completion can prove kernel enforcement of a new profile.

## Fixture Traps Met by the Dispute Tests

- A contract-dispute filing resolves the stage worktree (`verify::contracts::site::stage_site`) to run `frozen_drift`, so any test that files one needs `<repo>/.worktrees/<stage>` holding the frozen files; a missing worktree is the refusal `has no worktree`. The file-dispute cell of `inbox_drain/tests_matrix.rs` needs the same `contract_worktree` fixture as the freeze cell.
- `verify::transitions::save_stage` is create-only (`persistence.rs:46`); a test that changes a fixture stage uses `update_stage`.
- Tests that assert the pre-change re-freeze (`gate_dedup_contracts.rs`, `plan_server/chat/drafts/delete_tests.rs`) fail once an accept freezes recorded files at vetted bytes; update them with the behaviour.
- `tests/integration` relay e2e tests fail in a session or host job with `this Bash call is not running inside session`, because the sandbox hides the process tree; that is the environment, not a defect.
- `adjudication_e2e` amendment tests need a writable `/tmp` for the plan lock; run them through `loom check`, not in a worker sandbox.
- `fd` hides a host job's gitignored `acceptance-<n>.log`; list the job directory with `fd -I`.

## Stand-In Processes in Tests

A lib test never starts a process it then kills by hand. `crate::test_stand_in` owns them:

- `StandIn::group_leader(cmd)` and `StandIn::direct(cmd)` for a child the test spawns (the first leads its own process group, so its descendants die with it); `StandIn::exits_alone(cmd)` for a child that ends by itself; `wait_within(bound)` in place of `child.wait()`.
- `Orphan::adopt(pid)` (or `from_identity`) for a process that left the test's tree, adopted where its pid is first read, never in a `Drop`; `AdoptedGroup::adopt(pgid)` for a group the code under test created; `reaped_pid()` for a pid that is known to be free.
- Guards: `require_signals!()` before a test that signals its own child or group, `require_orphan_signals!()` before one that owns an orphan or an adopted group (it also requires readable start times), and `require_process_control!` where the test reads the process table.

Cleanup on drop is bounded, reports instead of panicking while a test unwinds, and signals an id only while it still names the process the test started: a group only while its leader is unreaped, an orphan only when its start time still matches. On macOS `killpg` answers `EPERM` for a group whose members are all zombies; the helper accepts that only when the leader is a zombie and then waits for the group to go.

`tests/integration/stand_in_signal_guard.rs` enforces this over every test module, inline `#[cfg(test)]` item and file under `tests/`: no kill outside the helper in `src/` (`kill`, `killpg`, `Child::kill`, `terminate`, `kill_and_reap`, the `kill` command, any signal that is not a literal `None`, `SIGSTOP`, `SIGCONT`, `SIGINT` or `SIGHUP`), no `.child().wait()`, no discarded kill followed by an unbounded wait, and no unbounded wait on a child bound from `.spawn()` in the same function. Code under `tests/` uses `loom::process::sandbox_probe::reap`. An exception is a named entry in `stand_in_signal_guard/allowances.rs` with its reason, and a test fails when an entry stops being needed.

The ignored lib test `process::sandbox_probe::host_job_lane::the_lib_tests_pass_under_host_job_confinement` reruns the lib tests named in `LOOM_HOST_JOB_LANE_ARGS` under the production host-job profile and fails when none ran; a macOS CI step runs it. Run it locally before a full host-job gate when a change adds a test that starts a process.

## Tests That Depend on Who Owns a Directory

Inside a user namespace that binds the real root (an agent session's sandbox), files owned by the host's root report an unmapped uid (65534), so code that refuses an ancestor directory owned by neither root nor the effective user (`native_worker/boundary/private_root.rs::trusted_directory`) refuses `/`. A test that builds a private root starts with `require_directory_owners!` (`process/sandbox_probe/needs.rs`: `directory_owners_visible`, `skip_without_directory_owners`), which skips through `skip_unless`. The ownership check itself is never relaxed for a test. A test that writes below `/tmp` guards on `path_writable("/tmp")` the same way (`native_worker/boundary/tmp_tests.rs`).

## Fixture Directories Carry Explicit Modes

A directory a test creates takes `0o777 & !umask` from `tempfile::tempdir()` and `create_dir_all`, so its mode differs between a umask of 022 (CI, macOS) and 002 (Linux distributions with user-private groups). A test whose subject checks directory modes builds its tree with `crate::test_private_dirs::private_tempdir()` (0700) and `create_dirs(path)` (0755 at every level). No test calls `umask`: it is process-wide and `daemon/server/lifecycle.rs` changes and restores it.

## Kernel and Sandbox Facts That Differ From Linux Test Assumptions

- **`send(MSG_DONTWAIT)` blocks on macOS.** XNU's `sosendcheck` honors only `SS_NBIO`/`MSG_NBIO` on a blocking socket, so `MSG_DONTWAIT` does not make the call non-blocking. A writer that must not block polls `POLLOUT` and then sends at most `SO_SNDLOWAT` bytes (`daemon/server/client_write.rs`).
- **`RLIMIT_FSIZE` is checked against the file offset even with `O_APPEND` on macOS** (`vn_write`), so an append descriptor at offset 0 can grow a file past the limit. `lseek(SEEK_END)` before the append makes it match Linux's `generic_write_checks` (`pressure/record/publication.rs`).
- **Seatbelt `file-read-metadata` outranks a `file-read*` deny**; a deny must name `file-read-metadata` and `file-test-existence` to hide a file ([Stage Build Cache](../architecture/stage-build-cache.md)).
- **The agent and Codex sandboxes cannot create `AF_UNIX` sockets** (`socket()` fails, not only `bind`). A test that creates one guards with `sandbox_probe::skip_unless(unix_socket_bindable(..))`, and a macro such as `require_unix_socket_and_loopback!` reports ok when it skips, so a local pass proves nothing: run those tests (for example `native_worker::proxy`) through `loom check`, which runs them on the host.
- **Tests inherit the session's environment.** Inside a stage session `LOOM_STAGE_ID`, `LOOM_SESSION_ID`, `LOOM_WORK_DIR` and `LOOM_HOOK_PATH` are set. A test that builds a shell command from words (`orchestrator/core/manual_start.rs::run_printed_start` drops empty words) or shims `PATH` for a hook must clear or set them itself; `loom-hooks/tests/run-all.sh` unsets them, a direct run does not.
- **A fixture's own git commands are not hardened.** `test_git::git` bypasses the runner's `--ignore-submodules=dirty`, so a plain checkout in a repository holding a nested repo runs `git status` inside the submodule and fires its planted filter. Configure the filter and dirty the submodule only after the last fixture git command.
- **Assert the whole message.** `error.contains("more than {MAX_ENTRIES}")` also matched the work-cap message `more than 1000000 ...` (100000 is a prefix of it), so a chain test passed with the cache bypassed. Assert the full phrase (`more than {MAX_ENTRIES} configuration entries`).
- **A test that depends on a new shape of a frozen contract fixture** pins the shape the fixture needs: a version 1 standard stage needs `artifacts` or `acceptance` and, as a leaf, `wiring`, or `validate()` fails with unrelated errors, so fixtures carry `artifacts: [README.md]` and a `README.md` wiring check, and use `literal: false` (`literal: true` needs plan version 2).
