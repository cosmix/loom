//! The running-test readers against real runs killed while a test hung
//! (`fixtures/<adapter>/killed/`).

use super::fixture_support::load;
use super::registry::unfinished_tests;
use super::{RunOutput, RunningTests};

/// `(adapter, scenario, command, what the run shows)`. `None` names: the
/// runner's output does not show a test until it finished.
const KILLED: &[(&str, &str, &str, Option<&[&str]>)] = &[
    ("cargo-test", "pretty", "cargo test", Some(&["tests::hang"])),
    ("cargo-test", "terse", "cargo test -q", None),
    (
        "go-test",
        "verbose",
        "go test ./... -v",
        Some(&["TestHang"]),
    ),
    ("go-test", "plain", "go test ./...", None),
    (
        "pytest",
        "verbose",
        "pytest -v",
        Some(&["test_hang.py::test_hang"]),
    ),
    ("pytest", "plain", "pytest", None),
    (
        "unittest",
        "verbose",
        "python3 -m unittest test_hang -v",
        Some(&["test_hang (test_hang.HangTest.test_hang)"]),
    ),
    ("unittest", "plain", "python3 -m unittest test_hang", None),
    ("node-test", "plain", "node --test", None),
    ("bun-test", "plain", "bun test", None),
    ("vitest", "plain", "vitest run", None),
    ("vitest", "verbose", "vitest run --reporter=verbose", None),
    ("rspec", "plain", "rspec", None),
    (
        "rspec",
        "documentation",
        "rspec --format documentation",
        None,
    ),
    (
        "minitest",
        "verbose",
        "ruby -Itest test/hang_test.rb -v",
        Some(&["HangTest#test_hang"]),
    ),
    ("minitest", "plain", "ruby -Itest test/hang_test.rb", None),
    (
        "mix-test",
        "trace",
        "mix test --trace",
        Some(&["test hangs (MixhangTest)"]),
    ),
    ("mix-test", "plain", "mix test", None),
    ("swift-test", "plain", "swift test", None),
    ("ctest", "plain", "ctest --test-dir build", Some(&["hang"])),
    (
        "ctest",
        "parallel",
        "ctest --test-dir build -j2",
        Some(&["hang"]),
    ),
];

#[test]
fn a_killed_run_names_its_running_tests_only_where_the_output_shows_them() {
    let cwd = tempfile::tempdir().unwrap();
    for (adapter, scenario, command, names) in KILLED {
        let (stdout, stderr, exit_code) = load(adapter, &format!("killed/{scenario}"));
        let out = RunOutput {
            stdout: &stdout,
            stderr: &stderr,
            exit_code,
        };
        let named =
            names.map(|names| RunningTests::Named(names.iter().map(|n| n.to_string()).collect()));
        let expected = named.clone().or(Some(RunningTests::Unknown));
        let case = format!("{adapter} {scenario}");
        assert_eq!(
            unfinished_tests(command, cwd.path(), &out),
            expected,
            "{case}"
        );
        // Under a command no adapter recognises, no other runner's reader
        // misreads this output, and a runner that cannot tell says nothing.
        assert_eq!(
            unfinished_tests("make check", cwd.path(), &out),
            named,
            "{case}"
        );
    }
}

#[test]
fn a_build_that_timed_out_is_no_test_run() {
    let cwd = tempfile::tempdir().unwrap();
    let out = RunOutput {
        stdout: "building...\n",
        stderr: "",
        exit_code: None,
    };
    assert_eq!(unfinished_tests("npm run build", cwd.path(), &out), None);
}
