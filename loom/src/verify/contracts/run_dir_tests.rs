//! Tests for the contract run directory: the contract's `cwd`, then the
//! recorded directory, then the nearest runner config, then the working
//! directory.

use super::*;
use crate::testrun::{RunOutput, RunSummary, TestTarget};
use crate::verify::contracts::test_support::contract;
use tempfile::TempDir;

/// An adapter whose single-test command names the file it was given, run
/// from the directories holding `markers`.
struct Marked(&'static [&'static str]);

impl TestRunnerAdapter for Marked {
    fn name(&self) -> &'static str {
        "marked"
    }
    fn language(&self) -> &'static str {
        "typescript"
    }
    fn recognizes(&self, _argv: &[String]) -> bool {
        false
    }
    fn is_full_run(&self, _argv: &[String]) -> bool {
        false
    }
    fn single_test_command(&self, file: &str, test: &str, _package_dir: &Path) -> String {
        format!("run {file} {test}")
    }
    fn run_root_markers(&self) -> &'static [&'static str] {
        self.0
    }
    fn select_command(&self, _targets: &[TestTarget], _package_dir: &Path) -> Option<String> {
        None
    }
    fn parse(&self, _out: &RunOutput<'_>) -> RunSummary {
        RunSummary::default()
    }
}

const BUNFIG: Marked = Marked(&["bunfig.toml"]);

/// A worktree holding `dirs` and the empty files `files`.
fn worktree(dirs: &[&str], files: &[&str]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    for dir in dirs {
        std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
    }
    for file in files {
        std::fs::write(tmp.path().join(file), "").unwrap();
    }
    tmp
}

fn spec(file: &str, cwd: Option<&str>) -> ContractSpec {
    ContractSpec {
        file: file.to_string(),
        test: "t".to_string(),
        cwd: cwd.map(str::to_string),
        ..contract()
    }
}

fn resolve(
    tmp: &TempDir,
    working_dir: &str,
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    recorded: Option<&str>,
) -> Result<ContractRun> {
    let root = tmp.path();
    resolve_run(contract, adapter, root, &root.join(working_dir), recorded)
}

/// The run's `cwd` and command.
fn cwd_and_command(run: &ContractRun) -> (&str, &str) {
    (run.cwd.as_str(), run.command.as_str())
}

#[test]
fn a_runner_config_below_the_working_dir_is_the_run_dir() {
    let tmp = worktree(&["apps/web"], &["bunfig.toml", "apps/web/bunfig.toml"]);
    let contract = spec("apps/web/src/a.contract.test.ts", None);

    let run = resolve(&tmp, ".", &contract, Some(&BUNFIG), None).unwrap();

    assert_eq!(
        cwd_and_command(&run),
        ("apps/web", "run src/a.contract.test.ts t")
    );
    assert_eq!(run.dir, tmp.path().join("apps/web").canonicalize().unwrap());
}

#[test]
fn a_runner_config_at_the_root_is_the_run_dir() {
    let tmp = worktree(&["apps/web"], &["bunfig.toml"]);
    let contract = spec("web/src/a.contract.test.ts", None);

    let run = resolve(&tmp, "apps", &contract, Some(&BUNFIG), None).unwrap();

    assert_eq!(
        cwd_and_command(&run),
        (".", "run apps/web/src/a.contract.test.ts t")
    );
    assert_eq!(run.dir, tmp.path().canonicalize().unwrap());
}

#[test]
fn without_a_runner_config_the_working_dir_is_the_run_dir() {
    let tmp = worktree(&["apps/web"], &[]);
    let contract = spec("web/src/a.contract.test.ts", None);

    let run = resolve(&tmp, "apps", &contract, Some(&BUNFIG), None).unwrap();
    assert_eq!(
        cwd_and_command(&run),
        ("apps", "run web/src/a.contract.test.ts t")
    );

    let unmarked = resolve(&tmp, "apps", &contract, Some(&Marked(&[])), None).unwrap();
    assert_eq!(unmarked.cwd, "apps");
    let no_adapter = resolve(&tmp, "apps", &contract, None, None).unwrap();
    assert_eq!(cwd_and_command(&no_adapter), ("apps", "t"));
}

#[test]
fn an_extension_marker_matches_any_file_with_that_extension() {
    let tmp = worktree(&["svc/tests"], &["svc/Svc.csproj"]);
    let contract = spec("svc/tests/ContractTests.cs", None);

    let run = resolve(&tmp, ".", &contract, Some(&Marked(&["*.csproj"])), None).unwrap();
    assert_eq!(
        cwd_and_command(&run),
        ("svc", "run tests/ContractTests.cs t")
    );

    let none = resolve(&tmp, ".", &contract, Some(&Marked(&["*.fsproj"])), None).unwrap();
    assert_eq!(none.cwd, ".");
}

#[test]
fn a_symlinked_marker_marks_no_run_dir() {
    let tmp = worktree(
        &["svc/tests", "real"],
        &["real/bunfig.toml", "real/Svc.csproj"],
    );
    for name in ["bunfig.toml", "Svc.csproj"] {
        std::os::unix::fs::symlink(
            tmp.path().join("real").join(name),
            tmp.path().join("svc").join(name),
        )
        .unwrap();
    }
    let contract = spec("svc/tests/ContractTests.cs", None);

    for markers in [&BUNFIG, &Marked(&["*.csproj"])] {
        let run = resolve(&tmp, ".", &contract, Some(markers), None).unwrap();
        assert_eq!(run.cwd, ".", "{:?}", markers.run_root_markers());
    }
}

#[test]
fn an_explicit_cwd_beats_a_recorded_one_and_detection() {
    let tmp = worktree(&["apps/web"], &["apps/web/bunfig.toml"]);
    let contract = spec("apps/web/src/a.contract.test.ts", Some("apps"));

    let run = resolve(&tmp, ".", &contract, Some(&BUNFIG), None).unwrap();
    assert_eq!(
        cwd_and_command(&run),
        ("apps", "run web/src/a.contract.test.ts t")
    );

    let recorded = resolve(&tmp, ".", &contract, Some(&BUNFIG), Some(".")).unwrap();
    assert_eq!(recorded, run);
}

#[test]
fn a_recorded_cwd_beats_detection() {
    let tmp = worktree(&["apps/web"], &["apps/web/bunfig.toml"]);
    let contract = spec("apps/web/src/a.contract.test.ts", None);

    let recorded = resolve(&tmp, ".", &contract, Some(&BUNFIG), Some(".")).unwrap();

    assert_eq!(
        cwd_and_command(&recorded),
        (".", "run apps/web/src/a.contract.test.ts t")
    );
}

#[test]
fn a_file_outside_its_run_dir_is_refused() {
    let tmp = worktree(&["apps/web", "apps/api"], &[]);
    let contract = spec("apps/web/src/a.contract.test.ts", Some("apps/api"));

    let error = resolve(&tmp, ".", &contract, Some(&BUNFIG), None).unwrap_err();
    assert!(format!("{error:#}").contains("not beneath"), "{error:#}");

    let detected = spec("apps/web/src/a.contract.test.ts", None);
    let escaping = resolve(&tmp, ".", &detected, None, Some("../elsewhere")).unwrap_err();
    assert!(format!("{escaping:#}").contains("cwd"), "{escaping:#}");
}

#[test]
fn a_missing_run_dir_is_refused() {
    let tmp = worktree(&["apps"], &[]);
    let contract = spec("apps/web/src/a.contract.test.ts", Some("apps/web"));

    let error = resolve(&tmp, ".", &contract, None, None).unwrap_err();

    assert!(format!("{error:#}").contains("does not exist"), "{error:#}");
}

#[test]
fn a_named_runner_builds_its_single_test_command() {
    let tmp = worktree(&["tests"], &[]);
    let contract = contract();
    let adapter = super::super::resolve_adapter(&contract, tmp.path()).unwrap();
    assert_eq!(adapter.name(), "cargo-test");

    let run = resolve(&tmp, ".", &contract, Some(adapter), None).unwrap();

    assert!(run.command.contains("tests::rejects_x"), "{}", run.command);
}
