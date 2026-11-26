# Spawn Observer And Group Retirement Races

> Process-wide spawn observer races; killpg missing a concurrent fork on macOS

## A process-wide spawn observer captured a parallel test's confined spawns

**What happened:** `after_stage_checks_are_judged_as_completion_judges_them` failed on Linux CI, reporting an after_stage `false` with `exit_code: 1` as failing. A `host_verify` test ran `run_job` at the same time and had installed a spawn observer. `prepare_group` read the observer from a process-wide static, so the dry run's spawn went through that test's observer. The observer's guardian start failed (no `.loom/work`), the spawn returned `Err`, and the dry run judged it `Failed` before the expected exit code was ever checked. The race also wrote the dry run's pids into the host test's `groups` file.

**Why:** `#[serial]` on the host_verify tests only keeps serial tests apart. Non-serial tests that spawn confined commands still ran beside them.

**Prevention:** process-wide mutable state that changes how other code behaves is per-thread or passed explicitly; `#[serial]` does not isolate it. `verify/criteria/spawn_observer.rs` keeps the observer in a `thread_local!` with a `!Send` guard. Every confined spawn of a job runs on the thread that installed it.

**Fix:** a thread-local observer slot. Regression test: `spawn_observer::another_threads_spawn_is_not_observed`.

## One killpg SIGKILL did not retire a group with a fork in flight

**What happened:** the hook test "direct Codex: retire descendants after leader exit" failed on macOS CI with `state: unknown`, exit 125 and `ownership_retained: True`. `_retire` in `loom-hooks/_codex-direct.py` sent one SIGKILL to the group while the leader was forking a grandchild. The grandchild joined the group after the signal went out, was reparented to launchd, and was never tracked. The group stayed non-empty until the deadline.

**Why:** on macOS a child forked during `killpg` can join the group after the signal and outlive it. Retirement treated one signal as final and only waited.

**Prevention:** a retirement loop re-signals the group while the probe still finds members, until the deadline. The group id cannot be reused while it has members. Success still needs an empty group.

**Fix:** `_retire` re-signals the group and the living tracked pids on every pass. Regression test: `run_missed_member_case` in `loom-hooks/tests/codex-direct-lifecycle.py`. Rust `OwnedProcess::retire` (`process/owned.rs`) still sends one `kill_group`; see [concerns](../concerns/sandbox-and-confinement-gaps.md).

## A serial test replaced PATH under a parallel test's confined spawn

**What happened:** `after_stage_checks_are_judged_as_completion_judges_them` kept failing now and then on Linux CI after the spawn observer became per-thread. `remote_control_tests.rs` set the process `PATH` to one directory holding a fake `claude` for the length of two `#[serial]` tests. A confined spawn on another thread in that window could not resolve `bwrap` (`confine_host.rs::bubblewrap`), the runner returned an error, and the dry run then reported an error as a failing check. The link to the CI failures is inferred from the code; the CI logs were not available.

**Why:** the investigation followed a timing correlation with another test's timeout kill and looped only the dry-run tests, so the test that changed the environment never ran beside them. `#[serial]` keeps serial tests apart and nothing else.

**Prevention:** a test that changes the process environment adds to a variable other threads read (prepend to `PATH`), never replaces it. A non-serial test that spawns commands resolved from the environment is `#[serial_test::parallel]`, which never overlaps a `#[serial]` test. To reproduce a cross-test flake, run the whole test binary, since the interfering test can be in any module.

**Fix:** `PathGuard::first` in `remote_control_tests.rs` keeps the other `PATH` entries; the four non-serial tests in `dry_run_real_tests.rs` are `#[serial_test::parallel]`.

## The Cleanup Grace Started at the Leader's Exit While Its Group Still Held the Pipes

**What happened:** On loaded Linux CI, confined criteria that exited cleanly were judged failing with "output capture incomplete" (`the_checkout_holds_committed_files_only`, CI run on 7ac2b9ca4). The 1 s `CLEANUP_GRACE` started when the outer bwrap was reaped, but the sandbox's pid 1, still in the leader's group, released stdout and stderr only after its mount namespace was torn down. An earlier investigation of the sibling `after_stage` flake blamed a `PATH` race alone, without reading the CI log, whose capture-incomplete lines would have named this cause.

**Why:** The cleanup contract assumed that once the leader exited and its group was SIGKILLed, only holders outside the group could keep the pipes open. In fact a killed process releases its files late in its exit, in `exit_task_work` after `exit_task_namespaces`, and bwrap reports the exit status before its pid 1 exits.

**Prevention:** Treat a killed group as still writing until it has no member, and bound collection by group membership rather than wall time since the leader exited. Read the failing CI log before settling on a flake cause; this sandbox needs the user to run `gh run view --log-failed` with the `!` prefix.

**Fix:** `process::CleanupDeadline` extends collection on the completed path while the group has a member, up to `DYING_GROUP_LIMIT`, then gives holders outside the group one more `CLEANUP_GRACE`.
