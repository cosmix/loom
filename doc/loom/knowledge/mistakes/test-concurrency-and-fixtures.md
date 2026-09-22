# Test Concurrency And Fixtures

> Racy tests: fds, ETXTBSY, serial env, stdin hangs

## An Inherited Descriptor Keeps an flock Alive After the Owner Releases It (2026-08-10)

**What happened:** `daemon::server::lock::tests::held_and_free_lock_states_are_distinct` failed roughly one run in nine under full parallel load, and passed every time in isolation. After `drop(guard)`, `inspect_lock` occasionally still reported `Held`.

**Why:** flock ownership belongs to the open file description, and `fork` duplicates it. Any _other_ test in the same binary that spawns a command inherits the lock descriptor for the window between fork and exec, and that inherited copy holds the lock alive even after the owner closes its own descriptor — `O_CLOEXEC` drops it at exec, not at fork. Demonstrated directly: a child that sleeps without exec'ing leaves the probe reading `HELD`; the same child with `execl` leaves it `FREE`.

**Prevention:** a test asserting "released" against a flock cannot assume the next probe observes it. Poll to a deadline instead of probing once, and report the last observed state (`held` vs `indeterminate`) in the failure message so the next failure is diagnosable. More generally, treat single-probe assertions about process-global OS state as flaky-by-construction in a multithreaded test binary that also spawns processes.

**Note for production:** the same window applies to the daemon singleton lock. A child forked during the microseconds `orchestrator.lock` is open can hold it past daemon exit until that child execs, so an immediate restart could briefly see "another daemon instance holds the singleton lock". Not observed in practice; recorded so the symptom is recognisable.

## A Non-Serial Test Read an Env Var a `#[serial]` Sibling Mutates (2026-09-03)

**What happened:** `verify::criteria::tests::runner_tests::test_run_acceptance_caches_pass_and_skips_second_execution` failed the pre-push gate on one machine at `assertion failed: second.results()[0].cached`, and passed on the same tree in another environment.

**Why:** `cache_tests::cache_policy_bypass_from_env` sets `LOOM_ACCEPTANCE_CACHE=0` process-wide for its duration under `#[serial]`. The runner test was not `#[serial]`, so nothing kept the two apart, and it read the ambient value via `CriteriaConfig::default()` and `CachePolicy::from_env()`. Whether the two overlap depends on core count and scheduling, so the failure reproduces on one machine and never shows on another.

**Prevention:** a test whose subject reads the process environment must either pin the value through the config surface (`with_cache_policy`) or be `#[serial]` alongside every test that mutates that variable. `#[serial]` only serialises against other `#[serial]` tests; it does nothing for a non-serial reader.

**Fix:** the runner test pins `CachePolicy::Use` (`verify/criteria/tests/runner_tests.rs`), matching its bypass sibling, so it no longer reads the environment at all.

## ETXTBSY Is a Fork/Exec Race Under Concurrent Tests, Not a Permissions Bug (2026-09-04)

**What happened:** three independent test failures across two stages, all `Os { code: 26,
kind: ExecutableFileBusy, message: "Text file busy" }`, all only under concurrent/
`--all-targets` runs and never when the failing test ran alone:
`orchestrator::terminal::native::wrapper::tests` (two exec sites, ~18% of 17 runs), and a
hand-rolled subprocess test fixture in `quota/codex.rs`'s `poll_once` tests spawning a
freshly-written+chmod'd script.

**Why:** the classic Linux ETXTBSY fork/exec race — the kernel refuses `exec` while ANY
process holds a write fd on that inode. In a multi-thousand-test multi-threaded binary,
another thread's `fork`+`exec` of a just-written script can race a thread still holding the
file open for write; the failure rate scales with concurrency.

**Detection:** a test that passes alone and fails only under `--all-targets`, with error code
26 naming the just-written executable, is this race — never a chmod/permissions problem, and
never specific to one test's script (it hit two independent exec sites in different modules).

**Prevention:** wrap `Command::spawn` in a bounded retry (5 attempts, ~20ms sleep) on
`raw_os_error() == Some(libc::ETXTBSY)` — keep the retry in PRODUCTION code too if a real
external tool self-updating mid-spawn is the same failure mode (`spawn_retrying_text_busy`).
Verify a flake fix by REPETITION, not one green run: 0 failures in 11 full-suite runs after
the fix, against ~18% before, is the only way to know it held — a single green gate run
proves nothing about a flake.

## A Test That "Kills" a Peer by Dropping an fd Is Racy Under Concurrent Process Spawns (2026-09-04)

**What happened:** `daemon::server::broadcast::tests::a_dead_peer_is_evicted_while_a_live_one
_is_kept` failed 2 of 10 full runs: the write to the supposedly-dead peer SUCCEEDED.

**Why:** the test simulated a closed peer with `drop(dead_reader)`, but closing an fd only
releases ONE reference to the socket. A concurrent `std::process::Command` fork in another
test thread can inherit a duplicate of that fd and keep the socket alive until the child
reaches its own exec — so `write_message()` on the "dead" peer doesn't return `EPIPE`. The
production code was never wrong.

**Prevention:** in a test binary that also spawns processes, any test simulating a closed
peer by dropping an fd is racy. Assert on socket state instead: `dead_reader.shutdown
(Shutdown::Both)` before the drop marks the SOCKET itself dead, which no forked fd copy can
undo.

## A Test That Calls a Hook's stdin Entry Point Hangs a Backgrounded Gate (2026-09-13)

**What happened:** the state-confinement gate ran `cargo test --all-targets` from a background shell. `commands::hook::tests_pre_compact::pre_compact_always_returns_ok` calls `pre_compact()`, which reads the real process stdin to EOF (`commands/hook/pre_compact.rs:39-47`). The background shell's stdin was a pipe that never closed, so the test blocked, `cargo test` never exited, and the gate never reported. The orchestrator had told the user the gate was running and waited for a completion notice that could not come; the hang went unnoticed for more than four hours, until the user asked.

**Why:** the test drives the stdin reader instead of the payload core the module already splits out for tests (`reset_for_payload`). It passes wherever stdin is `/dev/null` or a pipe that closes (CI, both git hooks), so nothing had flagged it. The gate script bounded no step, so a hang looked exactly like a slow run, and a background job notifies only when it exits.

**Prevention:** a test drives a hook's payload core with a literal string, never an entry point that reads `std::io::stdin()`. The readers today are `commands/hook/{pre_compact,user_prompt,relay,project_types}.rs` and `commands/knowledge/mod.rs`. A verification script meant for the background starts with `exec </dev/null` and wraps every step in `timeout`. A gate still out past its expected duration gets its logs read, not more waiting.

**Fix:** `pre_compact()` delegates to `pre_compact_from(input: impl Read)`, and the test calls `pre_compact_from(std::io::empty())`. The orchestrator's gate script now reads `/dev/null` and bounds every step.

## A Backgrounded `cat` Never Drains a Fake Subprocess's stdin (2026-09-05)

**What happened:** two subprocess-test gotchas in `quota/codex.rs`'s `poll_once` tests — and the
first recorded prevention for one of them was itself wrong, which is how the flake reached CI.

1. A fake script that never reads stdin and exits immediately races the parent's writes: if the
   script exits first the parent gets `Broken pipe (os error 32)` rather than a clean write.
   The prevention recorded here on 2026-09-04 — "background a stdin drain (`cat >/dev/null &`)"
   — **does not drain anything**. POSIX assigns `/dev/null` to the standard input of an
   asynchronous list in a shell without job control, before any explicit redirection, and
   `/bin/sh` is `dash` on Ubuntu CI. The backgrounded `cat` reads `/dev/null`, exits at once,
   and the script exits behind it. All it bought was the fork+exec delay, which hid the race
   locally and left it live in CI: `the_child_exiting_without_ever_replying_is_reported_precisely`
   failed 0/40 unloaded runs but 2/30 under 32 busy loops, asserting
   `"failed to write to codex app-server stdin"` against
   `"codex app-server closed without replying"`.
2. Teardown always calls `child.wait_timeout(Duration::from_secs(2))` before killing, on every
   exit path including shutdown; against a script that ignores stdin closing (e.g. `sleep 30`),
   this adds a full ~2s to the test's elapsed time even after the reply-wait loop gave up early.

**Why:** a fixture cannot paper over a production defect. `poll_once` treated any write failure
as fatal, so a child that died before reading its request was reported as a loom-side write
error instead of by what it printed — the same misreport a real `codex app-server` crashing on
startup would produce. Every attempt to keep a reader alive in the fixture was working around
that, and the cheapest-looking workaround happened not to work at all.

**Prevention:** fix the code, not the fixture. A `BrokenPipe` on a request write to a child is
not an outcome worth reporting: the child's stdout (a reply, a JSON-RPC error, or EOF) is.
`poll_once` now reports only write errors whose `ErrorKind` is not `BrokenPipe` and otherwise
falls through to `await_reply`, so both orderings of the race produce the same verdict.
If a fixture genuinely must hold the read end open, the shell must save the descriptor before
backgrounding — `exec 3<&0; cat <&3 >/dev/null &` — or stay alive itself (`sleep 30`).
`cat <&0 >/dev/null &` fails too: fd 0 is already `/dev/null` by the time the duplication runs.
Check any such claim with `printf 'x\n' | sh -c 'cat > out & wait'`; an empty `out` means the
drain never ran. And any test asserting a tight "returns within Xs" bound on code with an
unconditional teardown grace window must budget that grace on top of the deadline/shutdown
latency.

**Fix:** `loom/src/quota/codex.rs` — `write_requests` returns `std::io::Result<()>` and
`poll_once` matches `Err(e) if e.kind() != ErrorKind::BrokenPipe` for the only fatal case; the
five dead `cat >/dev/null &` drains are gone from `codex_tests.rs`. A race that only fires under
load needs a loaded runner to catch: `scripts/flake-check.sh` re-runs `quota::`, `process::`,
`verdict_apply_tests::` and `stalled_judge_tests::` under CPU contention, pinned to 4 CPUs where
`taskset` exists, wired into CI, the release workflow's publish gate, and the pre-push hook.

## A Success-Path Deadline Tighter Than Production Is a Flake (2026-09-06)

**What happened:** `quota::codex::tests::garbage_and_an_over_long_line_are_skipped_before_the_reply` failed the pre-push flake check with `codex app-server timed out`: the fake child had not written its reply within the 5 s the test gave `poll_once`. The `quota::` run took 8.30 s against a normal 3 s. The script itself takes under 0.1 s even under the check's pinned load, and 60 loaded re-runs (the default load, under a pty, and with 32 spinners) never reproduced it, so something on the runner held the child for more than 5 s.

**Why:** the five reply-expecting tests passed a 5 s deadline for no reason. Production gives the exchange 15 s (`CODEX_DEADLINE`), and on the success path the deadline never fires; it only caps a broken exchange, so any value shorter than production's trades nothing for a flake the moment the runner stalls the child. The three timing tests carried the same shape, about 1 s of margin between the expected elapsed time and the assert bound.

**Prevention:** in a subprocess test, a deadline the success path never reaches is not a timing assertion, so give it a generous value (`REPLY_DEADLINE`, 60 s). An elapsed-time bound proves one thing, that the code did not wait out the child's `sleep 30` or the deadline; set it well under that escape and well over the expected time, and say in the comment what it rules out. Build an over-long fixture line with a printf field width (`printf '%70000s\n' ''`) under `#!/bin/sh`; a 70000-element brace expansion needs bash and buys nothing.

**Fix:** `loom/src/quota/codex_tests.rs`: `REPLY_DEADLINE` replaces the five 5 s deadlines, the elapsed bounds go from 4/5/3 s to 15/20/15 s with comments naming what each rules out, and the over-long-line fixture is plain sh.

## One Panic Between set_current_dir and Its Restore Fails Sixty Unrelated Tests (2026-09-06)

**What happened:** a full `cargo test --all-targets` reported 71 failures across memory, stage, stop, map and merge-lifecycle tests, all `Failed to get current dir: NotFound`. Only one test had a real defect: it asserted an `INDEX.md` row shape that had changed, panicked after `std::env::set_current_dir(&test_dir)` and before the restore, and its temp dir was then dropped — leaving the whole test process with a deleted cwd for every later test.

**Why:** the cwd is process-global; a test that changes it and panics before restoring never runs the restore line, and `TempDir`'s drop removes the directory the process is still standing in. `#[serial]` does not help — it only orders the tests, it cannot restore the cwd.

**Prevention:** when many unrelated tests fail with `current_dir` NotFound, run `cargo test --lib -- --test-threads=1` and fix the FIRST failure only; the rest are the cascade. Run a suspect module in isolation (`cargo test --lib <module>`) to separate a real failure from contamination. A new cwd-changing test should restore via a guard type (Drop) rather than a trailing statement.

**Fix:** corrected the one assertion; the other 70 passed untouched.

## `pre_compact_always_returns_ok` Blocks When the Test Process Has an Open Stdin (2026-09-13)

**What happened:** a full `cargo test --all-targets` run launched as a background shell job stopped
making progress at `commands::hook::pre_compact::tests::pre_compact_always_returns_ok`, which
libtest reported as running for over 60 seconds, and the run never finished. The test calls
`pre_compact()` (`commands/hook/pre_compact.rs`), which reads stdin to end-of-file. That background
job's stdin stayed open, so the read never returned. CI, the pre-commit hook and loom's acceptance
runner start tests with stdin at end-of-file, where the same test returns at once.

**Workaround:** from any harness whose stdin may stay open (a background job, an agent's shell, an
interactive terminal), run the suite as `cargo test ... < /dev/null`.

**Not fixed:** the test reads the real process stdin; `reset_for_payload` exists so tests can drive
the hook without it, and this test could use it or be removed, since `pre_compact()` returns
`Ok(())` by construction.

## A `#[serial]` Test That Rewrote `PATH` Broke Unrelated Tests (2026-09-13)

**What happened:** a test for the remote-control crash path installed a fake `claude` by setting the process-wide `PATH` to a single temp bin dir and pointing `HOME` at a fake home, restoring both on drop. It was marked `#[serial]`. In the full suite three unrelated, non-serial tests (`orchestrator::core::event_handler::recover_hung_tests::*`) failed with `failed to spawn a stand-in agent process: NotFound`; they passed in isolation. The same fake was also order-dependent: `remote_control::cached_preflight_enabled` memoizes the preflight in a process-lifetime `OnceLock`, so whichever test probed first fixed the answer for the rest of the run.

**Why:** `#[serial]` orders a test only against other `#[serial]` tests. Every non-serial test in the binary kept running on other threads and resolved executables through the rewritten `PATH`.

**Prevention:** a test never mutates process-wide environment (`PATH`, `HOME`, auth variables) to steer the code under test. Add an injectable seam instead, as `SessionBackend::tmux_available` does. Detection: a test that passes alone and fails in the full run with `NotFound` spawning a process means some other test rewrote `PATH`.

**Fix:** the crash handler reads Remote Control activity through an injectable `Orchestrator` field; the test sets it instead of the environment.

## `cfg(test)` Env-Snapshot Fakes Never Apply Inside a `loom/tests/*.rs` Integration Target (2026-09-14)

`EnvSnapshot::from_process_env` (`loom/src/relay/emit.rs:79`) returns an empty, deterministic snapshot only under `cfg(test)` — a cfg that applies to unit tests compiled INTO the library crate, never to `loom/tests/*.rs` integration targets, which link the non-test lib and therefore always read the REAL process environment. An integration test that calls a public command reading `EnvSnapshot` (e.g. `worktree_cmd::remove`) takes the live `RelayMode::Relay` path inside any actual session and fails unpredictably (e.g. `worktree_remove_safety` 8/8). **Prevention:** integration tests must drive the explicit-mode seam directly (e.g. `remove_with_mode(.., RelayMode::Operator)`) rather than the env-reading wrapper — `cfg(test)` fakes are a unit-test-only convenience, never available to an integration target.

## `tests/phantom_merge.rs` Ran `repair --fix` Against the Real Home Directory (2026-09-14)

**What happened:** a loom stage edited `loom-hooks/*.sh`, then its `cargo test` run called `repair::execute(true)` in-process from `tests/phantom_merge.rs` with no `HOME` redirect. `--fix` reinstalled the worktree's hook scripts into the real `~/.claude/hooks/loom`, overwriting the copy the operator's already-running (older) daemon expected. That daemon then refused every subsequent spawn with `loom hook scripts in ~/.claude/hooks/loom are missing or differ from this loom build`. Running `loom repair --fix` from the old binary reverted the hooks, and the next stage that touched `loom-hooks/` repeated the cycle. This happened on 2026-09-14 for the integration-verify and knowledge-distill stages of PLAN-loop-recovery.

**Why:** `repair::execute` resolves `dirs::home_dir()` to locate hook scripts, codex hooks, `settings.json`, and other home-relative assets (`commands/repair/settings_checks.rs::hook_scripts_issue`, `commands/repair/hooks.rs::check`, `home_assets::check`). The test isolated its working directory (`with_cwd`) but never isolated `HOME`, so every one of those checks read and wrote the developer's real home.

**Prevention:** any test that calls `repair::execute`, `ensure_loom_permissions`, `install_loom_hooks`, or `install_codex_hooks` must redirect `HOME` and `LOOM_HOME` to a `TempDir` first. In the lib test binary, prefer the injectable `_to(dir)` variants (e.g. `install_loom_hooks_to`) over env mutation — `#[serial]` only orders serial tests against each other and does not fence the binary's non-serial tests, per "A `#[serial]` Test That Rewrote `PATH` Broke Unrelated Tests" above. Detection: an installed hook file under `~/.claude/hooks/loom` whose mtime falls inside a stage session and whose content matches the worktree rather than the currently-installed loom build.

**Fix:** `tests/phantom_merge.rs` gained a local `HomeGuard`/`isolate_home()` helper that points `HOME` and `LOOM_HOME` at a scratch `TempDir` for the test's duration and restores both on drop, bound with `let _home = isolate_home();` in every test that calls `repair::execute`. This is safe there specifically because the file is its own standalone test binary and every affected test is already `#[serial]`.

## A Relay Test Feeding `scratch_root` a `$TMPDIR`-Based Path Fails by Design (2026-09-13)

`relay::scratch::scratch_root` refuses any root under `/tmp`, and the sandbox's own `$TMPDIR` is
under `/tmp` — so a test that feeds `scratch_root` a directory derived from `$TMPDIR` fails on the
refusal, not on the code under test. `relay_e2e.rs` uses a directory outside `/tmp` instead
(`target/relay-e2e-tmp`); write a new relay test the same way.

## Headless CI Has No Terminal Emulator — Pin `LOOM_TERMINAL` in Tests That Build an Orchestrator (2026-08-10)

**What happened:** `merge_handler_attempt_tests::merge_probe_failure_does_not_consume_resolver_attempt_budget` passed on every dev box and failed in CI with `No terminal emulator found. Set TERMINAL environment variable or install one of: kitty, alacritty, ...`. It recurred on 2026-09-03 in five tests across `orchestrator/core/event_handler/verdict_retirement_tests.rs` and `orchestrator/core/stage_executor_tests.rs`.

**Why:** `Orchestrator::new` builds the session backend from the persisted `[terminal]` config (`SessionBackend::from_config`, `orchestrator/terminal/backend.rs:96-112`). With `SessionBackendKind::Native` — which is what an absent config resolves to — it eagerly constructs a `NativeBackend`, and `detect_terminal` probes the host. A GitHub runner has no emulator installed, so construction fails and the `.unwrap()` panics: a pure state-machine assertion killed by the host environment.

**Prevention — the heading above is stale.** `LOOM_TERMINAL` still works, but the mechanism the tree now uses is the config, not the env var: write a `TerminalConfig { backend: SessionBackendKind::Tmux }` into the work dir with `fs::work_dir::write_terminal_config` before calling `Orchestrator::new`. Tmux leaves the native lane unbuilt, so no detection runs (`backend.rs:99-102`, asserted by `backend::tests::from_config_tmux_leaves_the_native_lane_unbuilt`). No env var, no `#[serial]`. Working helpers: `event_handler/tests.rs::handoff_work_dir`, `stage_executor_tests.rs::work_dir`, `event_handler/stalled_judge_tests.rs::work_root`.

**The trap that caused the recurrence: the helper and the test must name the SAME directory.** `write_terminal_config(dir)` and `read_terminal_config(dir)` both key off the directory handed to `OrchestratorConfig::work_dir`. `handoff_work_dir()` returns the `TempDir`, not the work path, so each test recomputes it — and five tests recomputed it as `temp.path().join(".work")` while the helper had written to `temp.path().join(".loom").join("work")`. No config there, so the native lane came back and detection ran. The mismatch is invisible on macOS, where detection succeeds. When adding a test to those files, copy the `.loom/work` join from a neighbouring test rather than inventing the path.

**Detection rule:** to reproduce headless failures locally, build a `PATH` of symlinks that excludes every terminal binary and run the prebuilt test binaries with `DISPLAY`/`WAYLAND_DISPLAY`/`TERMINAL`/`LOOM_TERMINAL` unset. A fix verified only on a machine that has a terminal proves nothing.

## A Fixed Timestamp Checked Against a Now-Relative Window Expires (2026-09-13)

**What happened:** `commands/usage/discovery_tests.rs::explicit_root_discovers_old_mtime_file_without_home_fallback` wrote a transcript line stamped `2026-09-12T20:00:00Z` and parsed it within `range()`, a window measured back from `Utc::now()`. It passed until 2026-09-13T20:00Z and failed on every run after, on main and on every branch, with no code change: two gate runs ten minutes apart gave opposite results.

**Why:** a literal date compared against a sliding window encodes "recently" as a constant, so the test fails on a date nobody chose.

**Prevention:** a fixture timestamp read by a now-relative filter is computed from `Utc::now()` (one hour ago, say). A literal date belongs only in a test that also pins the window, for example `time_range::parse_since_at(spec, fixed_now)`. When a test fails for the first time with no code change, compare its date literals with the clock before anything else.

**Fix:** the fixture stamps its entry relative to `Utc::now()`; the old `UNIX_EPOCH` mtime, which is what the test is about, stays.
