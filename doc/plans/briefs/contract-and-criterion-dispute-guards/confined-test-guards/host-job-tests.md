# Brief: host-job-tests (stage confined-test-guards)

## Goal

`cargo test --lib host_verify::` passes inside a loom host job on macOS without a `--skip`.

## Files you own

- `loom/src/host_verify/tests_runner.rs`, `tests_runner_group.rs`, `tests_runner_legacy.rs`, `tests_runner_pending.rs`, `tests_runner_recovery.rs`
- `loom/src/host_verify/tests_store.rs`
- `loom/src/host_verify/tests_wait.rs`

Test code only. If the cause is in production code, stop and report it with evidence; do not edit it.

## What is reported (unverified)

An adjudicator skipped these tests on its own account of them; treat each as a hypothesis:

1. The `host_verify::tests_runner*` suites and `host_verify::tests_store::pruning_keeps_an_old_job_with_a_live_group_guardian` hang or fail inside a host job, said to be because they kill process groups and guardians from inside the host job's own guarded, Seatbelt-confined process group; the criterion then times out at 1800 s.
2. `host_verify::tests_wait::a_terminal_outcome_leaves_a_newer_request_the_other_waiter_wrote` races a 300 ms helper thread against git subprocesses in `wait_or_request` and fails when the host is loaded.

The `host_verify` tests guarded by `confinement_unavailable` wrappers (`tests_child.rs:25`, `tests_child_complete.rs:16`, `tests_freeze.rs:35`) are another worker's concern through the probe; leave them.

## Method

- Reproduce first. Outside a host job, run each named test alone and note its time. The host-job run is your orchestrator's: ask it for the output of `loom check confined-test-guards --criterion 4` (the `host_verify::` criterion) and the per-criterion log path it prints. Work from that output, not from the account above.
- For 1: find what the test does to process groups (`rg -n 'killpg|process_group|guardian' loom/src/host_verify/tests_runner*.rs loom/src/host_verify/tests_store.rs`) and why that differs under a host job. Fix the test so it manages only processes it started, if that is the cause. If the test cannot run inside a host job at all, give it a guard that detects the condition by probing it (start and signal a child group of its own) and skips through `crate::process::sandbox_probe::skip_unless(<probe ok>, <test name>, <why>)` (`sandbox_probe.rs:218`), so CI's `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` makes the skip fatal there. Never guard on an environment variable.
- For 2: remove the race. Replace the fixed 300 ms wait with a synchronisation the test controls (a channel, a barrier, or polling a condition with a generous deadline). Do not skip it.

## Acceptance

- Each named test passes alone and with the whole `host_verify::` filter in your shell.
- Report, per test: the cause you confirmed with evidence, the change, and whether it now runs or skips in a host job (from the orchestrator's `loom check` output).
- Report every assertion you changed, with the reason (the orchestrator files the integrity dispute from this list).
