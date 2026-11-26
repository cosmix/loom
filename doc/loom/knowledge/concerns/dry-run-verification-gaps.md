# Dry Run Verification Gaps

> Dry-run check gaps: bwrap exit 1 masks setup failure; Linux after_stage flake

## A Confined Check Expecting Exit 1 Passes When bubblewrap Fails to Start

bubblewrap exits 1 when its own setup fails, so a confined `after_stage` or wiring check declaring `exit_code: 1` is judged passing even when the command never started. Acceptance and after_stage both run through `run_prepared_live`; only the verdict code differs (`criterion_eval` and `assertions::evaluate`). Revisit when a check's expected exit code must be told apart from a sandbox setup failure.

## after_stage Checks Fail Intermittently on Linux CI

`dry_run_real_tests::after_stage_checks_are_judged_as_completion_judges_them` fails on Linux CI about 15 s into `cargo test --lib`. Check 0 (`false`, `exit_code: 1`) is reported as failing; check 1 (`echo hi`, `stdout_contains: ["absent"]`) is meant to fail, and acceptance `true` passes in the same run, so bubblewrap works and only the `false` verdict is wrong. The cause is unconfirmed: it did not reproduce in a Linux container under load. Three causes fit: a runner error, a signal (no exit code), or incomplete output capture. A runner error is reported as `NotRun` ("could not run at the base commit: <cause>"), and a `Failed` outcome logs its runner failures to stderr, so the next failure names which one.

In both CI failures the 2 s timeout of `a_command_slow_with_a_warm_cache_is_a_timeout` (`sleep 4`) fired while the after_stage run was in progress, and neither test is `#[serial]`. Leading hypothesis: that timeout kill reaches a process outside its own job and signals the `false` run. The per-thread spawn observer did not remove the flake, so the crossing path is elsewhere. To check: loop `cargo test --lib dry_run_real_tests -- --test-threads=8` on Linux and read the note or stderr on failure; if marking both tests `#[serial]` stops the flake, the timeout kill is hitting the wrong process.
