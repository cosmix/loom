//! Libtest-named selections split by the cargo target compiling each test.

use super::*;
use crate::testrun::registry;
use crate::verify::criteria::{ProbeRun, ProbeRunner};
use crate::verify::impact_tests::runs::{Run, Site};
use std::cell::RefCell;
use tempfile::TempDir;

const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

fn cargo() -> &'static dyn TestRunnerAdapter {
    registry::by_name("cargo-test").unwrap()
}

fn target(file: &str, name: &str) -> TestTarget {
    TestTarget {
        file: file.to_string(),
        name: Some(name.to_string()),
    }
}

/// A package with a library, an integration test and a file no target compiles.
fn package(root: &Path, dir: &str) {
    let files = [
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "mod a;\n"),
        ("src/a.rs", ""),
        ("src/orphan.rs", ""),
        ("tests/it.rs", ""),
    ];
    for (path, text) in files {
        let path = root.join(dir).join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn each_cargo_target_gets_its_own_exact_group() {
    let root = TempDir::new().unwrap();
    package(root.path(), "");
    let targets = [
        target("src/lib.rs", "a::tests::one"),
        target("tests/it.rs", "flow"),
        target("src/lib.rs", "a::tests::two"),
    ];
    let command = select_command(root.path(), Path::new(""), cargo(), &targets);
    assert_eq!(
        command.as_deref(),
        Some(
            "cargo test --lib -- --exact a::tests::one a::tests::two && \
             cargo test --test it -- --exact flow"
        )
    );
}

#[test]
fn a_test_with_no_certain_target_keeps_the_unscoped_selection_alone() {
    let root = TempDir::new().unwrap();
    package(root.path(), "pkg");
    let targets = [
        target("src/orphan.rs", "orphan::t"),
        target("src/lib.rs", "a::t"),
    ];
    let command = select_command(root.path(), Path::new("pkg"), cargo(), &targets);
    assert_eq!(
        command.as_deref(),
        Some("cargo test -- orphan::t && cargo test --lib -- --exact a::t")
    );
}

#[test]
fn a_package_without_a_manifest_is_one_unscoped_selection() {
    let root = TempDir::new().unwrap();
    let targets = [target("src/lib.rs", "a::t"), target("src/lib.rs", "b::t")];
    let command = select_command(root.path(), Path::new(""), cargo(), &targets);
    assert_eq!(command.as_deref(), Some("cargo test -- a::t b::t"));
}

#[test]
fn an_unnamed_selection_selects_nothing() {
    let root = TempDir::new().unwrap();
    package(root.path(), "");
    let unnamed = TestTarget {
        file: "src/a.rs".to_string(),
        name: None,
    };
    assert_eq!(
        select_command(root.path(), Path::new(""), cargo(), &[unnamed]),
        None
    );
}

#[test]
fn a_runner_selecting_by_file_is_not_split() {
    let root = TempDir::new().unwrap();
    let pytest = registry::by_name("pytest").unwrap();
    let targets = [TestTarget {
        file: "t_a.py".to_string(),
        name: None,
    }];
    let package_dir = root.path();
    assert_eq!(
        select_command(root.path(), Path::new(""), pytest, &targets),
        pytest.select_command(&targets, package_dir)
    );
}

#[test]
fn a_runner_that_cannot_name_a_target_runs_one_selection() {
    let root = TempDir::new().unwrap();
    package(root.path(), "");
    let nextest = registry::by_name("cargo-nextest").unwrap();
    let targets = [target("src/lib.rs", "a::t"), target("tests/it.rs", "flow")];
    assert_eq!(
        select_command(root.path(), Path::new(""), nextest, &targets).as_deref(),
        Some("cargo nextest run -E 'test(=a::t) or test(=flow)'")
    );
}

struct Recorder(RefCell<Vec<String>>);

impl ProbeRunner for Recorder {
    fn run(&self, command: &str, _package_dir: &Path) -> anyhow::Result<ProbeRun> {
        self.0.borrow_mut().push(command.to_string());
        Ok(ProbeRun {
            stdout: "running 2 tests\n\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 \
                     measured; 0 filtered out; finished in 0.00s\n"
                .to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            timed_out: false,
            output_complete: true,
            cached: None,
        })
    }
}

#[test]
fn the_site_runs_and_reports_the_scoped_chain() {
    let root = TempDir::new().unwrap();
    package(root.path(), "");
    let recorder = Recorder(RefCell::new(Vec::new()));
    let site = Site {
        root: root.path(),
        runner: &recorder,
        place: "here",
    };
    let targets = [target("src/lib.rs", "a::t"), target("tests/it.rs", "flow")];
    let Run::Passed { command, .. } = site.run(Path::new(""), cargo(), &targets) else {
        panic!("the recording runner passes");
    };
    let expected = "cargo test --lib -- --exact a::t && cargo test --test it -- --exact flow";
    assert_eq!(command, expected);
    assert_eq!(*recorder.0.borrow(), [expected]);
}
