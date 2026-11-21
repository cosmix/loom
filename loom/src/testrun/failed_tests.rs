//! The failed-test readers against every recorded run (`fixtures/<adapter>/`).

use crate::testrun::fixture_support::{load, scenarios};
use crate::testrun::{registry, RunOutput};

/// `(adapter, the test its failing scenarios name)` for the runners with a
/// reader; each records `one-fail` and `suite` scenarios failing one test.
const NAMED: &[(&str, &str)] = &[
    ("cargo-test", "tests::beta_fails"),
    ("cargo-nextest", "fixture tests::beta_fails"),
    ("go-test", "TestBetaFails"),
    ("pytest", "test_fixture.py::test_beta_fails"),
    ("vitest", "fixture.test.js > beta_fails"),
    ("jest", "beta_fails"),
];

fn failing(scenario: &str) -> bool {
    scenario.starts_with("one-fail") || scenario.starts_with("suite")
}

#[test]
fn a_failed_run_names_its_failed_test_and_a_passing_or_unbuilt_one_names_none() {
    for (adapter_name, test) in NAMED {
        let adapter = registry::by_name(adapter_name).expect("registered adapter");
        let recorded: Vec<String> = scenarios(adapter_name)
            .into_iter()
            .filter(|scenario| !scenario.starts_with("killed"))
            .collect();
        assert!(
            recorded.iter().any(|s| failing(s)),
            "{adapter_name} has a failing fixture"
        );
        for scenario in recorded {
            let (stdout, stderr, exit_code) = load(adapter_name, &scenario);
            let out = RunOutput {
                stdout: &stdout,
                stderr: &stderr,
                exit_code,
            };
            let expected = failing(&scenario).then(|| vec![test.to_string()]);
            assert_eq!(
                adapter.failed_tests(&out),
                expected,
                "{adapter_name} {scenario}"
            );
        }
    }
}

/// No reader names a test from another runner's failing output.
#[test]
fn no_reader_misreads_another_runners_failure() {
    for (own, _) in NAMED {
        for (other, _) in NAMED.iter().filter(|(other, _)| other != own) {
            let adapter = registry::by_name(own).expect("registered adapter");
            for scenario in scenarios(other).into_iter().filter(|s| failing(s)) {
                let (stdout, stderr, exit_code) = load(other, &scenario);
                let out = RunOutput {
                    stdout: &stdout,
                    stderr: &stderr,
                    exit_code,
                };
                assert_eq!(
                    adapter.failed_tests(&out),
                    None,
                    "{own} on {other} {scenario}"
                );
            }
        }
    }
}

#[test]
fn libtest_names_each_failed_test_once_across_its_result_lines() {
    let stdout = "running 3 tests\n\
        test a::one ... FAILED\n\
        test a::two - should panic ... FAILED\n\
        test a::three ... ok\n\
        \n\
        failures:\n    a::one\n    a::two\n\n\
        test result: FAILED. 1 passed; 2 failed\n";
    let out = RunOutput {
        stdout,
        stderr: "",
        exit_code: Some(101),
    };
    let adapter = registry::by_name("cargo-test").expect("registered adapter");
    assert_eq!(
        adapter.failed_tests(&out),
        Some(vec!["a::one".to_string(), "a::two".to_string()])
    );
}
