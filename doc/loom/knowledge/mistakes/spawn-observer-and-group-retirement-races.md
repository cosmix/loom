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
