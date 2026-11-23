# Detached Spawn In Tests

>

## Never Spawn a Surviving Process From a Test (2026-08-21)

**What happened:** the prompt hook's background self-heal
(`commands/hook/reconcile_graph.rs::spawn_if_needed`, Appendix A.12 of the
retrieval-precision proposal) launches a detached `loom hook reconcile-graph`
with `process_group(0)` so it survives the hook's exit. Seven tests in
`commands/hook/tests_user_prompt_e2e.rs` drive `retrieve_for_prompt`, which
calls it. One of those tests removes `LOOM_WORK_DIR` on purpose to exercise the
checkout fallback — so `WorkDir::new(".")` searched UPWARD from the crate
directory and resolved to the real loom checkout instead of a fixture. Its
semantic layer is genuinely stale, so the spawn fired against an 8,500-node
repository. Every `cargo test` run leaked more children, each walking the whole
tree through tree-sitter. The machine exhausted 125 GB and had to be rebooted.

**Why:** three ordinary decisions composed into a runaway.

- `WorkDir::new` searching upward is correct for reading and dangerous for
  writing: a test's working directory is inside the real repo, so an unset
  environment variable does not fail, it silently retargets the real checkout.
- `process_group(0)` is exactly right for the production hook and exactly wrong
  under a test harness, which exits without reaping what it started.
- `cargo test` reports nothing about a leaked child. The suite goes green while
  the children keep running, so the cost lands on the next run, not this one.

**Prevention:**

- Guard process creation at the lowest level — the function that calls
  `Command::spawn` — never at the caller. The staleness check, lock decision and
  lock claim above it stay under test; only the spawn is suppressed.
- `cfg!(test)` covers `#[cfg(test)]` modules in this crate but NOT the
  integration targets under `loom/tests/*.rs`, which link the lib built without
  it. Cover both, e.g. an `AtomicBool` defaulting to `!cfg!(test)`.
- Never start expensive background work against a project root reached only by
  an upward directory walk. Inference is fine for READING an index, not for
  spawning work against it: require an explicit `LOOM_WORK_DIR`, or an existing
  `.loom/cache/context-v1` proving loom already maintains this checkout.
- When reviewing any `Command::spawn`, ask what happens when a test reaches it.

**Fix:** spawn guarded by an `AtomicBool` defaulting to `!cfg!(test)`, plus an
inferred-root refusal in `spawn_if_needed`, plus a test asserting the guard is
in force so it cannot regress silently.

## Drop Order Deleted a tmux Socket Before kill-server Ran (2026-10-02)

**What happened**: every full test run left four `tmux` servers alive (`-s loom-web-term-stage`); 188 had accumulated.

**Why**: `loom/tests/e2e/web_terminal.rs` `Fixture` declared its `TmuxTmpDirGuard` before its `TmuxServerGuard`. Rust drops fields in declaration order, so the isolated socket directory was deleted first and `tmux -S <socket> kill-server` failed silently.

**Prevention**: declare a process guard before the guard that removes what it needs (fields drop top to bottom), make the kill wait for the process to exit, and count leftover processes before and after a test run.

**Fix**: field order swapped, a bounded wait added (`loom/tests/e2e/tmux_guards.rs`); the leaked servers were killed.

## Rebinding a Just-Dropped Listener Races Concurrent Child Spawns

**What happened:** `web_server::serve::tests::zero_slots_still_waits_for_the_worker_resource_owner` passed alone and failed once in the parallel lib run with `AddrInUse` when it rebound the address its dropped listener had held.

**Why:** std opens listeners with `SOCK_CLOEXEC`, but a child another test thread is spawning holds a copy of every descriptor between fork and exec, so the address stays bound after this process drops its last copy. Measured: 620 of 20000 rebinds busy with four threads spawning `/bin/true` (max wait 0.9 ms), none without spawners.

**Prevention:** prove release through ownership first (a completion flag or `Arc` count), then retry the rebind on `AddrInUse` with a short deadline (1 s), so a listener this process still owns fails the test.

**Fix:** `rebind_when_released` in `loom/src/web_server/serve_tests.rs`.

## Test Planning Servers Ran the Real Quota Poller

**What happened:** plan_server tests failed intermittently on macOS. Two wildcard-bind tests got 408 (request head timed out) and others reported `still stopping`. Later, a lib test binary launched directly (its path run in a loop, without `cargo test`) started the real poller in every planning-server test again: the tests printed `quota: claude: failed to open quota lock` and ran the real `codex app-server`, and the extra child spawns widened the fork windows behind other flakes.

**Why:** the tests dialed the listener's `local_addr()`, `0.0.0.0:<port>`, and a host network filter (Little Snitch) can hold a new binary's non-loopback flows past the server's 5 s head timeout. Separately, every test server started the real quota poller: it read the login keychain, called the usage endpoint, ran `codex` and wrote `~/.loom/quota`. A joined poll outlasted the 3 s stop timeout, in production too. The first guard was only the `TEST_LOOM_NO_QUOTA_POLL` variable from the workspace `.cargo/config.toml` `[env]`, which a directly launched test binary never receives.

**Prevention:** a test client dials loopback for a wildcard bind. No test starts a worker that reaches the keychain, the network or the real home directory. On macOS the keychain ignores HOME, so a temp HOME does not isolate it. A worker that writes only timestamped caches is never joined by server stop. A guard that keeps in-crate tests off real state is compiled in (`cfg!(test)`); an environment variable from the Cargo config covers only binaries Cargo launches and the processes they spawn.

**Fix:** `dial` in `plan_server/tests/support.rs`. `run_quota_poller` returns at once under `cfg!(test)` or when `TEST_LOOM_NO_QUOTA_POLL` is set; the variable stays for the `loom` processes tests spawn, and it is not `LOOM_`-prefixed because server test helpers strip `LOOM_*` from spawned processes. `summaries::start_quota_poller` detaches the poller from stop. Regression: `a_quota_poll_in_flight_does_not_hold_up_stop`.

## Planning-Server Tests Raced Forked Children and a Changed `LOOM_HOME`

**What happened:** five tests failed intermittently in the lib test binary, one of them in a host-job dry run of a pending plan: a connect expected to be refused right after a server stop succeeded (`tests::ws::the_server_keeps_serving_while_shutdown_drains`); a port probed free by bind-and-drop was busy at the next bind (`an_omitted_port_walks_past_a_busy_default`); a just-written fake `loom` failed to exec with `ExecutableFileBusy`; and two drafts tests listed nothing because they resolved the runtime directory themselves and the server resolved it again after a `#[serial]` test had changed `LOOM_HOME`. Every cause was already recorded in [test-concurrency-and-fixtures](test-concurrency-and-fixtures.md) (the forked descriptor copy, ETXTBSY, the non-serial env reader); the tests were written without applying those rules.

**Why:** a child forked on another test thread holds a copy of every descriptor until its exec. A closed listener therefore stays bound and accepts a connect for that long, and a dropped probe listener keeps its port. `#[serial]` keeps a test apart only from other `#[serial]` tests.

**Prevention:** prove a listener was released with `web_server::listener::rebind_when_released` (it polls the bind to a deadline and holds the address); never assert that a connect is refused or that a dropped probe left a port free. A probe-then-bind scenario repeats until an attempt agrees with its probe, and fails when none does. A spawn of an executable that a test or an updater may have just written goes through `process::spawn_retrying_text_busy`. A test that reads a `LOOM_HOME`-derived path twice (itself and through the code under test) is `#[serial_test::parallel]`, or passes the path in. Confirm a flake fix by repetition of the directly launched test binary at its default thread count, which shows these races far more often than `cargo test` with the pinned thread count.

**Fix:** `rebind_when_released` shared from `web_server/listener.rs`; the retry in `plan_server/run/child.rs`, `pressure::spawn_codex_logged` and `process::run_bounded`; `#[serial_test::parallel]` on the two drafts tests; the repeat in the two default-port tests.

## A Full-Binary Sweep Found Five More Tests That Assumed a Drop Closes at Once or Read the Ambient Home

**What happened:** five runs of the whole lib test binary at its default thread count failed in four tests intermittently and one always: two `daemon::wire` tests expected EOF right after `drop(sender)`; `fs::append_contract_tests::safe_open_append_holds_lock` expected a flock free right after the holder dropped; the resolver verification test of the merge handler ran a confined criterion, which creates the stage build cache under the process `HOME`, and passed only while a `#[serial]` test happened to have the cache pinned (in a plain shell it wrote to the real home); one provision-gate test lacked the confined-command guard of its siblings.

**Why:** the same fork window as above for the first three: a descriptor copy in a child that has not exec'd keeps a socket open and a flock held. The fourth read process state that only serial tests set.

**Prevention:** mark a socket closed with `shutdown(Shutdown::Both)` before the drop; observe a flock release with a bounded poll. A test that runs a confined criterion pins the cache with `test_cache_env::CacheEnv::pin()` and is `#[serial]`; a test that starts a confined command begins with `confined_commands_runnable()`. Sweep the whole binary by repetition after adding tests that spawn processes; a filter run hides the neighbours that open the window.

**Fix:** the four test files named above; three further full runs passed with no failure.
