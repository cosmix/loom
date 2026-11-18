//! Tests for sharing a rejected build between the contracts of one completion
//! check, over a runner that counts its calls.

use super::tests::{fixture_with, frozen_at, run_check, Fixture, Replay, ONE_PASS};
use super::*;
use crate::verify::contracts::test_support::{contract, write_files, CONTRACT_FILE};
use std::cell::Cell;

const REJECTED: &str = include_str!("../../testrun/fixtures/cargo-test/build-error.stderr");
const ONE_FAIL: &str = include_str!("../../testrun/fixtures/cargo-test/one-fail.stdout");

/// A runner that answers every command alike and counts the commands run.
struct Counting {
    stdout: &'static str,
    stderr: &'static str,
    exit: i32,
    calls: Cell<usize>,
    output_complete: bool,
}

impl Counting {
    fn new(stdout: &'static str, stderr: &'static str, exit: i32) -> Self {
        Self {
            stdout,
            stderr,
            exit,
            calls: Cell::new(0),
            output_complete: true,
        }
    }
}

impl ProbeRunner for Counting {
    fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
        self.calls.set(self.calls.get() + 1);
        Ok(ProbeRun {
            stdout: self.stdout.to_string(),
            stderr: self.stderr.to_string(),
            exit_code: Some(self.exit),
            timed_out: false,
            output_complete: self.output_complete,
            cached: None,
        })
    }
}

struct Incomplete(&'static str);

impl ProbeRunner for Incomplete {
    fn run(&self, command: &str, package_dir: &Path) -> Result<ProbeRun> {
        let mut run = Replay(self.0).run(command, package_dir)?;
        run.output_complete = false;
        Ok(run)
    }
}

#[test]
fn incomplete_capture_cannot_pass_a_contract() {
    let fx = fixture_with(Vec::new());
    for output in [ONE_PASS, ""] {
        let error = run_check(&fx, &Incomplete(output)).unwrap_err().to_string();
        assert!(error.contains("output capture was incomplete"), "{error}");
    }
}

/// The fixture with contracts `c0`, `c1`, ... in `files`, one per entry.
fn fixture_of(files: &[&str]) -> Fixture {
    let mut fx = fixture_with(Vec::new());
    for file in files.iter().filter(|file| **file != CONTRACT_FILE) {
        write_files(&fx.worktree, &[(file, "#[test]\nfn t() {}\n")]);
    }
    fx.stage.contracts = files
        .iter()
        .enumerate()
        .map(|(n, file)| ContractSpec {
            id: format!("c{n}"),
            file: file.to_string(),
            test: Some(format!("t{n}")),
            ..contract()
        })
        .collect();
    fx
}

fn check(fx: &Fixture, runner: &Counting) -> String {
    run_check(fx, runner).unwrap_err().to_string()
}

fn not_run(id: &str, first: &str) -> String {
    format!("contract '{id}' was not run: the build it shares with contract '{first}' failed")
}

#[test]
fn contracts_of_one_build_run_it_once() {
    let fx = fixture_of(&["tests/x_contract.rs"; 4]);
    let runner = Counting::new("", REJECTED, 101);

    let error = check(&fx, &runner);

    assert_eq!(runner.calls.get(), 1);
    assert!(error.contains("contract 'c0' does not build"), "{error}");
    for id in ["c1", "c2", "c3"] {
        assert!(error.contains(&not_run(id, "c0")), "{error}");
    }
    assert_eq!(error.matches("does not build").count(), 1, "{error}");
}

#[test]
fn incomplete_compiler_output_is_not_shared_as_a_failed_build() {
    let fx = fixture_of(&["tests/x_contract.rs"; 2]);
    let mut runner = Counting::new("", REJECTED, 101);
    runner.output_complete = false;

    let error = check(&fx, &runner);

    assert_eq!(runner.calls.get(), 2);
    assert_eq!(error.matches("output capture was incomplete").count(), 2);
    assert!(!error.contains("was not run"), "{error}");
}

#[test]
fn two_builds_run_twice() {
    let fx = fixture_of(&[
        "tests/x_contract.rs",
        "tests/x_contract.rs",
        "tests/y_contract.rs",
        "tests/y_contract.rs",
    ]);
    let runner = Counting::new("", REJECTED, 101);

    let error = check(&fx, &runner);

    assert_eq!(runner.calls.get(), 2);
    assert!(error.contains(&not_run("c1", "c0")), "{error}");
    assert!(error.contains(&not_run("c3", "c2")), "{error}");
}

#[test]
fn a_failing_test_is_not_shared() {
    let fx = fixture_of(&["tests/x_contract.rs"; 3]);
    let runner = Counting::new(ONE_FAIL, "", 101);

    let error = check(&fx, &runner);

    assert_eq!(runner.calls.get(), 3);
    assert!(!error.contains("was not run"), "{error}");
}

#[test]
fn a_pass_is_not_shared() {
    let fx = fixture_of(&["tests/x_contract.rs"; 3]);
    let runner = Counting::new(ONE_PASS, "", 0);

    run_check(&fx, &runner).unwrap();

    assert_eq!(runner.calls.get(), 3);
}

#[test]
fn a_rejection_without_the_compiler_report_is_not_shared() {
    let fx = fixture_of(&["tests/x_contract.rs"; 2]);
    let killed = "error: process didn't exit successfully: `rustc` (signal: 9, SIGKILL)\n";
    let runner = Counting::new("", killed, 101);

    check(&fx, &runner);

    assert_eq!(runner.calls.get(), 2);
}

#[test]
fn collection_problems_are_reported_for_a_contract_that_did_not_run() {
    let mut fx = fixture_with(vec![frozen_at(".")]);
    write_files(
        &fx.worktree,
        &[
            (
                "src/lib.rs",
                "#[path = \"elsewhere.rs\"]\nmod contract_tests;\n",
            ),
            ("src/contract_tests.rs", "#[test]\nfn t() {}\n"),
        ],
    );
    fx.stage.contracts = ["c0", "c1"]
        .iter()
        .map(|id| ContractSpec {
            id: id.to_string(),
            file: "src/contract_tests.rs".to_string(),
            test: Some("contract_tests::t".to_string()),
            runner: Some("cargo-test".to_string()),
            ..contract()
        })
        .collect();
    let runner = Counting::new("", REJECTED, 101);

    let error = check(&fx, &runner);

    assert_eq!(runner.calls.get(), 1);
    assert!(error.contains(&not_run("c1", "c0")), "{error}");
    assert!(
        error.contains("contract 'c1': src/lib.rs declares contract_tests with #[path"),
        "{error}"
    );
}
