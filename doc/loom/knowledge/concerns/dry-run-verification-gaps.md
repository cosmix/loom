# Dry Run Verification Gaps

> Dry-run check gaps: bwrap exit 1 masks setup failure; after_stage flake cause

## A Confined Check Expecting Exit 1 Passes When bubblewrap Fails to Start

bubblewrap exits 1 when its own setup fails, so a confined `after_stage` or wiring check declaring `exit_code: 1` is judged passing even when the command never started. Acceptance and after_stage both run through `run_prepared_live`; only the verdict code differs (`criterion_eval` and `assertions::evaluate`). Revisit when a check's expected exit code must be told apart from a sandbox setup failure.

## after_stage Checks Fail Intermittently on Linux CI

A confined Linux command's output pipes are held by the sandbox's pid 1 (bubblewrap's init keeps stdio) until its exit finishes, after the sandbox's namespaces are torn down. Under `--new-session` that pid 1 calls `setsid` before it is forked off, so it leads its own session and group, outside the leader's, and the outer bwrap exits as soon as pid 1 reports the command's status. A finished confined command therefore waits for end of file up to `DYING_GROUP_LIMIT` (10 s) past retirement (`CleanupDeadline::contained`, chosen by `PreparedCommand::contains_its_processes`, Linux confined runs only); every holder left dies with the sandbox's pid namespace. Unconfined runs and `process::run_bounded_inner` poll the killed group instead, and timed-out runs keep the plain `CLEANUP_GRACE`. The mechanism comes from bubblewrap's source and kernel exit ordering; `executor_lifecycle_tests.rs` pins pid 1's own session inside the sandbox, and no test reproduces the slow teardown.

An incomplete capture names its cause in the failure text and the stream marker: `output collection timed out: <stream> open <s> after exit, <writer state><group state>`, or `error reading output: <stream>: <error>` (`verify/criteria/executor_capture.rs`). A recurrence therefore says which case it is. `read_stream` retries `EINTR`; the generic `process::drain` does not.

A timeout kill of `a_command_slow_with_a_warm_cache_is_a_timeout` cannot reach other tests' processes: `escapees::members` selects only by the unreaped leader's pid, group and session. The dry-run real tests are `#[serial_test::parallel]` and never run beside a `#[serial]` test, which could replace `PATH` while they resolve `bwrap`. `verify/criteria/tests/cache_input_runner_tests.rs` still sets `PATH` to its fixture directory plus `/usr/bin:/bin`; a host with `bwrap` elsewhere loses it for non-serial, non-parallel confined tests during that test.
