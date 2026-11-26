# Dry Run Verification Gaps

> Dry-run check gaps: bwrap exit 1 masks setup failure; after_stage flake cause

## A Confined Check Expecting Exit 1 Passes When bubblewrap Fails to Start

bubblewrap exits 1 when its own setup fails, so a confined `after_stage` or wiring check declaring `exit_code: 1` is judged passing even when the command never started. Acceptance and after_stage both run through `run_prepared_live`; only the verdict code differs (`criterion_eval` and `assertions::evaluate`). Revisit when a check's expected exit code must be told apart from a sandbox setup failure.

## after_stage Checks Fail Intermittently on Linux CI

Confined commands on Linux CI were judged failing with "output capture incomplete" although they exited cleanly: CI showed it for `dry_run_real_tests::the_checkout_holds_committed_files_only`, and the earlier `after_stage_checks_are_judged_as_completion_judges_them` failures, whose logs were never read, probably had the same cause. bubblewrap's outer process exits as soon as the sandbox's pid 1 reports the command's status through `event_fd`. That pid 1 stays in the leader's process group and releases stdout and stderr only at the end of its own exit, after the kernel tears down the sandbox's mount namespace. So when a command completes, pipe collection keeps waiting past `CLEANUP_GRACE` while the killed group still has a member, up to `DYING_GROUP_LIMIT` (`process/cleanup_deadline.rs`, used by `executor::run_prepared_live` and `process::run_bounded_inner`). A timed-out command keeps the plain 1 s grace, and so do holders outside the group. The mechanism is inferred from bubblewrap's and the kernel's exit ordering and has not been measured on CI; a recurrence with the group wait in place points elsewhere ([mistake](../mistakes/spawn-observer-and-group-retirement-races.md)).

A timeout kill of `a_command_slow_with_a_warm_cache_is_a_timeout` cannot reach other tests' processes: `escapees::members` selects only by the unreaped leader's pid, group and session. The dry-run real tests are `#[serial_test::parallel]` and never run beside a `#[serial]` test, which could replace `PATH` while they resolve `bwrap`. A runner error is reported as `NotRun` ("could not run at the base commit: <cause>"), and a `Failed` outcome logs its runner failures to stderr.

`verify/criteria/tests/cache_input_runner_tests.rs` still sets `PATH` to its fixture directory plus `/usr/bin:/bin`; a host with `bwrap` elsewhere loses it for non-serial, non-parallel confined tests during that test.
