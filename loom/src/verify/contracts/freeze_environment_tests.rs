//! `judge_run` with the run's output: a red whose output shows an environment failure is refused,
//! a red that is the test's own failure and a build failure are accepted.

use super::*;
use crate::testrun::registry;

fn spec() -> ContractSpec {
    ContractSpec {
        id: "no-follow".to_string(),
        file: "tests/no_follow.rs".to_string(),
        test: Some("no_follow::rejects_symlink".to_string()),
        runner: Some("cargo-test".to_string()),
        cwd: None,
        scenario: "a symlink planted in the path".to_string(),
        rejects: "an open that follows symlinks".to_string(),
    }
}

fn red() -> RunSummary {
    RunSummary {
        executed: Some(1),
        passed: Some(0),
        failed: Some(1),
        ..RunSummary::default()
    }
}

fn judge(summary: RunSummary, stdout: &str, stderr: &str) -> Result<ContractRunReport, String> {
    judge_run(
        &spec(),
        registry::by_name("cargo-test"),
        summary,
        Some(101),
        Some((stdout, stderr)),
    )
}

#[test]
fn a_red_caused_by_a_refused_sandbox_is_refused() {
    let refused = "sandbox-exec: sandbox_apply: Operation not permitted";

    let error = judge(red(), "", refused).expect_err("an environment red");

    assert!(error.contains("contract 'no-follow' fails for an environment reason"));
    assert!(error.contains("(operation not permitted)"));
    assert!(error.contains("no nested OS sandbox"));
    assert!(judge(RunSummary::default(), refused, "").is_err());
}

#[test]
fn a_red_that_is_the_tests_own_failure_is_accepted() {
    let panic = "assertion `left == right` failed\n  left: 1\n right: 2";

    let report = judge(red(), "", panic).expect("an ordinary red");

    assert_eq!(report.outcome, "failed");
}

#[test]
fn a_build_failure_stays_an_accepted_red_even_with_a_marker_in_its_output() {
    let build = RunSummary {
        build_failed: true,
        ..RunSummary::default()
    };

    let report = judge(build, "", "error: could not compile; read-only file system");

    assert_eq!(report.expect("a build failure").outcome, BUILD_FAILED);
}

#[test]
fn a_counted_assertion_naming_eperm_keeps_its_verdict() {
    let counted = "running 1 test\ntest signals::kills_child ... FAILED\n\nfailures:\n\n\
                   ---- signals::kills_child stdout ----\nassertion `left == right` failed\n  \
                   left: \"EPERM\"\n right: \"ESRCH\"\n\nfailures:\n    signals::kills_child\n\n\
                   test result: FAILED. 0 passed; 1 failed; 0 ignored\n";

    let report = judge(red(), counted, "");

    assert_eq!(
        report
            .expect("a counted failure is the test's own red")
            .outcome,
        "failed",
        "a counted assertion keeps its verdict"
    );
    let refused = "Error: kill EPERM\nbwrap: setting up uid map: refused";
    assert!(
        judge(RunSummary::default(), "", refused).is_err(),
        "a launcher refusal is still an environment red"
    );
}
