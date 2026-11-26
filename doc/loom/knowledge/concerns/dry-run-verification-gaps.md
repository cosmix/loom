# Dry Run Verification Gaps

> Dry-run check gaps: bwrap exit 1 masks setup failure; after_stage flake cause

## A Confined Check Expecting Exit 1 Passes When bubblewrap Fails to Start

bubblewrap exits 1 when its own setup fails, so a confined `after_stage` or wiring check declaring `exit_code: 1` is judged passing even when the command never started. Acceptance and after_stage both run through `run_prepared_live`; only the verdict code differs (`criterion_eval` and `assertions::evaluate`). Revisit when a check's expected exit code must be told apart from a sandbox setup failure.

## after_stage Checks Fail Intermittently on Linux CI

`dry_run_real_tests::after_stage_checks_are_judged_as_completion_judges_them` failed on Linux CI with check 0 (`false`, `exit_code: 1`) reported as failing while acceptance `true` passed in the same run. The CI logs of those runs were never read, so the cause is inferred from the tree and unconfirmed on CI. The claim that a timeout kill of `a_command_slow_with_a_warm_cache_is_a_timeout` reaches the `false` run is wrong: `escapees::members` selects only by the unreaped leader's pid, group and session, and no other test's process can match.

A cause the tree does show: a `#[serial]` test replaced the process `PATH` while this non-serial test resolved `bwrap` from it, so the runner returned an error ([mistake](../mistakes/spawn-observer-and-group-retirement-races.md)). The dry-run real tests are `#[serial_test::parallel]` and never run beside a `#[serial]` test. They still run beside each other, so a recurrence points at something else: a runner error is reported as `NotRun` ("could not run at the base commit: <cause>"), and a `Failed` outcome logs its runner failures to stderr, so the next failure names its cause.

`verify/criteria/tests/cache_input_runner_tests.rs` still sets `PATH` to its fixture directory plus `/usr/bin:/bin`; a host with `bwrap` elsewhere loses it for non-serial, non-parallel confined tests during that test.
