use super::*;
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::testrun::registry;

fn spec() -> ContractSpec {
    ContractSpec {
        id: "no-follow".to_string(),
        file: "tests/no_follow.rs".to_string(),
        test: "no_follow::rejects_symlink".to_string(),
        runner: Some("cargo-test".to_string()),
        cwd: None,
        scenario: "a symlink planted in the path".to_string(),
        rejects: "an open that follows symlinks".to_string(),
    }
}

fn cargo_test() -> Option<&'static dyn TestRunnerAdapter> {
    registry::by_name("cargo-test")
}

fn ran(executed: u64, failed: u64) -> RunSummary {
    RunSummary {
        executed: Some(executed),
        passed: Some(executed - failed),
        failed: Some(failed),
        ..RunSummary::default()
    }
}

#[test]
fn freeze_rejects_non_contract_changes() {
    let contracts = [spec()];
    let harness = ["tests/support/**".to_string()];
    let changed = ["tests/no_follow.rs", "tests/support/fs.rs", "src/lib.rs"].map(String::from);
    let inputs = FreezeInputs {
        contracts: &contracts,
        harness: &harness,
        changed_paths: &changed,
        existing: &|_| true,
    };

    let problems = check_changes(&inputs).expect_err("src/lib.rs is not a contract file");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("src/lib.rs"), "{problems:?}");
}

#[test]
fn freeze_rejects_passing_contract() {
    assert_eq!(classify(&ran(1, 0), Some(0)), RunOutcome::Passed);
    let error = judge_run(&spec(), cargo_test(), ran(1, 0), Some(0)).expect_err("green");
    assert!(error.contains("passes before implementation"), "{error}");
}

#[test]
fn freeze_rejects_unselected_contract() {
    let error = judge_run(&spec(), cargo_test(), ran(0, 0), Some(0)).expect_err("unselected");
    assert!(error.contains("did not select"), "{error}");
}

#[test]
fn freeze_reports_failing_and_unverified_runs() {
    let report = judge_run(&spec(), cargo_test(), ran(1, 1), Some(101)).expect("red run");
    assert_eq!(report.outcome, "failed");
    assert_eq!(report.adapter.as_deref(), Some("cargo-test"));
    assert_eq!(report.exit_code, Some(101));

    let report = judge_run(&spec(), None, RunSummary::default(), Some(2)).expect("exit 2");
    assert_eq!(report.outcome, UNVERIFIED);
    assert!(judge_run(&spec(), None, RunSummary::default(), Some(0)).is_err());
}

/// What a failed attempt leaves for the operator: a refusal's own
/// problems, not its instructions, else the whole error.
#[test]
fn a_failed_attempt_lists_its_problems_for_the_operator() {
    let problems = vec!["src/lib.rs is neither a contract file".to_string()];
    let error = refuse("s1", problems.clone(), "Revert it, then").unwrap_err();
    assert!(error.to_string().contains("contracts freeze s1` again"));
    assert_eq!(listed_problems(&error), problems);

    let error = anyhow::anyhow!("no merge base").context("cannot find the stage base");
    assert_eq!(
        listed_problems(&error),
        ["cannot find the stage base: no merge base"]
    );
    assert!(refuse("s1", Vec::new(), "unused").is_ok());
}

/// A site whose working directory holds `tests/no_follow.rs` with `source`, and
/// whose acceptance is one `rustfmt --check` over that file.
/// Callers share the environment-test lock because toolchain lookup reads HOME/PATH.
fn site_checking_formatting_of(source: &str) -> (tempfile::TempDir, ContractSite) {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    std::fs::create_dir(dir.join("tests")).unwrap();
    std::fs::write(dir.join("tests/no_follow.rs"), source).unwrap();
    let stage = Stage {
        id: "s1".to_string(),
        acceptance: vec![AcceptanceCriterion::Simple(
            "rustfmt --check tests/no_follow.rs".to_string(),
        )],
        ..Stage::default()
    };
    let site = ContractSite {
        work_dir: dir.join(".loom/work"),
        stage,
        worktree_root: dir.clone(),
        working_dir: dir,
    };
    (temp, site)
}

/// A frozen file cannot change, so the freeze is refused while a contract file
/// fails the stage's own formatter check; the refusal names the check's command,
/// and with it the file.
#[test]
#[serial_test::serial]
fn freeze_refuses_a_contract_file_that_fails_the_formatter_check() {
    let _toolchain = crate::verify::test_support::ToolchainReadRoot::from_runner();
    let (_temp, site) = site_checking_formatting_of("pub fn  add(a:u32,b:u32)->u32{a+b}\n");

    let error = check_formatting(&site, CommandConfinement::Confined).expect_err("unformatted");

    let message = error.to_string();
    assert!(message.contains("Contracts not frozen"), "{message}");
    assert!(
        message.contains("`rustfmt --check tests/no_follow.rs`"),
        "{message}"
    );
    assert!(message.contains("contracts freeze s1` again"), "{message}");
    assert_eq!(listed_problems(&error).len(), 1, "{message}");
}

#[test]
#[serial_test::serial]
fn freeze_goes_on_when_the_contract_files_are_formatted() {
    let _toolchain = crate::verify::test_support::ToolchainReadRoot::from_runner();
    let (_temp, site) =
        site_checking_formatting_of("pub fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n");

    assert!(check_formatting(&site, CommandConfinement::Confined).is_ok());
}
