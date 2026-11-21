//! What the guard lets through, each entry with the reason it is safe.

/// Test files that may send a kill directly (rule 1), each for a reason that
/// makes the raw call the thing under test or the checked step it relies on.
/// Rule 3 still applies to them.
pub const ALLOWED: &[(&str, &str)] = &[
    (
        "src/test_stand_in.rs",
        "the helper: it checks every kill it sends and bounds every wait",
    ),
    (
        "src/test_stand_in/adopted.rs",
        "the helper's orphans and adopted groups: it checks every kill it sends",
    ),
    (
        "src/verify/criteria/confine_seatbelt_signal_tests.rs",
        "its sleeper must run outside every sandbox while a confined command tries to \
         kill it; stop() checks the kill and both of its reaps are bounded",
    ),
    (
        "src/host_verify/tests_runner_group.rs",
        "kills the runner's process group and fails the test on the error, as the runner does",
    ),
    (
        "src/host_verify/tests_runner.rs",
        "waits on its child only after kill() returned Ok",
    ),
];

/// Single calls exempt from rules 1 and 4, as (file, the call as findings
/// print it, why the raw call is safe).
pub const ALLOWED_CALLS: &[(&str, &str, &str)] = &[
    (
        "src/process/tests.rs",
        "terminate(child.id())",
        "terminate is the code under test; the child is a StandIn waited on with a bound",
    ),
    (
        "src/process/tests.rs",
        "terminate(999999999)",
        "terminate is the code under test, given a pid above any the kernel hands out",
    ),
    (
        "src/fs/permissions/hooks/policy_tests_fixture.rs",
        "child.wait_with_output()",
        "the hook ends on its own once it reads the payload, whose pipe the test closes first; \
         no signal is involved, as with output()",
    ),
    (
        "src/fs/permissions/hooks/policy_tests_stage_gate.rs",
        "child.wait_with_output()",
        "the hook ends on its own once it reads the payload, whose pipe the test closes first; \
         no signal is involved, as with output()",
    ),
    (
        "src/process/sandbox_probe/reap.rs",
        "kill_and_reap(&mutchild)",
        "kill_and_reap is the code under test, and the test asserts it reports the child ended",
    ),
    (
        "src/commands/pressure/terminal_tests.rs",
        "kill(Pid::from_raw(driver.id()asi32),signal)",
        "each signal is the stimulus whose handler is under test; the driver is a StandIn \
         waited on with a bound",
    ),
];
