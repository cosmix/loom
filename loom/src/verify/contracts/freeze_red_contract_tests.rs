//! Contracts for E1: a frozen red is a failing test or a compiler rejection.
//! A build that stopped before the compiler judged the code, or a run that
//! ended without an exit code, is no red at all.

use std::time::Duration;

use crate::testrun::registry;
use crate::verify::contracts::freeze::{judge_prepared_result, BUILD_FAILED};
use crate::verify::contracts::test_support::contract;
use crate::verify::criteria::CriterionResult;

fn ignore(_: &str) {}

fn run(stdout: &str, stderr: &str, exit_code: Option<i32>) -> CriterionResult {
    CriterionResult::new(
        "cargo test".to_string(),
        false,
        stdout.to_string(),
        stderr.to_string(),
        exit_code,
        Duration::ZERO,
        false,
    )
}

#[test]
fn a_build_stopped_before_the_compiler_is_refused() {
    let adapter = registry::by_name("cargo-test");
    assert!(adapter.is_some(), "the cargo-test adapter is registered");
    let run = run(
        "",
        "error: failed to run `rustc` to learn about target-specific information\n",
        Some(101),
    );
    let judged = judge_prepared_result(&contract(), adapter, &run, ignore);
    let message = match judged.judged {
        Ok(report) => panic!(
            "a build that stopped before the compiler froze as outcome `{}`",
            report.outcome
        ),
        Err(message) => message,
    };
    assert!(
        message.contains("before the compiler judged the code"),
        "{message}"
    );
}

#[test]
fn a_compiler_rejection_is_an_accepted_red() {
    let adapter = registry::by_name("cargo-test");
    assert!(adapter.is_some(), "the cargo-test adapter is registered");
    let run = run(
        "",
        "error[E0425]: cannot find value `x` in this scope\n\
         error: could not compile `fx` (test \"x_contract\") due to 1 previous error\n",
        Some(101),
    );
    let judged = judge_prepared_result(&contract(), adapter, &run, ignore);
    match &judged.judged {
        Ok(report) => assert_eq!(report.outcome, BUILD_FAILED),
        Err(message) => panic!("a compiler rejection was refused: {message}"),
    }
    assert!(
        judged.compile_failed,
        "the adapter reads a compiler rejection"
    );
}

#[test]
fn a_run_without_an_exit_code_is_refused() {
    let adapter = registry::by_name("cargo-test");
    assert!(adapter.is_some(), "the cargo-test adapter is registered");
    let stdout = "running 1 test\ntest tests::rejects_x ... FAILED\n\nfailures:\n    \
                  tests::rejects_x\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; \
                  0 measured; 0 filtered out; finished in 0.00s\n";
    let judged = judge_prepared_result(&contract(), adapter, &run(stdout, "", None), ignore);
    let message = match judged.judged {
        Ok(report) => panic!(
            "a run without an exit code froze as outcome `{}`",
            report.outcome
        ),
        Err(message) => message,
    };
    assert!(message.contains("without an exit code"), "{message}");
}
