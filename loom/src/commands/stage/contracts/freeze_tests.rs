use super::*;
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::testrun::registry;
use crate::verify::contracts::test_support::{
    contract_worktree, liquidmon_contract, pinned, write_liquidmon,
};

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
        scaffold: &ScaffoldCheck {
            paths: Default::default(),
            problems: Vec::new(),
        },
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

/// A liquidmon-shaped stage over a real repository: `site` plus its git.
struct Liquidmon {
    _temp: tempfile::TempDir,
    site: ContractSite,
    repo: WorktreeGit,
}

fn liquidmon(base_lib: &str) -> Liquidmon {
    let temp = tempfile::TempDir::new().unwrap();
    let repo_root = temp.path().join("repo");
    let worktree = contract_worktree(&repo_root, "s1");
    if !base_lib.is_empty() {
        commit_base_lib(&worktree, base_lib);
    }
    write_liquidmon(&worktree);
    let workspace = crate::fs::work_dir::WorkDir::new(&repo_root).unwrap();
    workspace.adopt_existing().unwrap();
    let root = worktree.canonicalize().unwrap();
    let stage = Stage {
        id: "s1".to_string(),
        contracts: vec![liquidmon_contract()],
        ..Stage::default()
    };
    let site = ContractSite {
        work_dir: workspace.root().to_path_buf(),
        stage,
        worktree_root: root.clone(),
        working_dir: root,
    };
    let repo = pinned(&worktree);
    Liquidmon {
        _temp: temp,
        site,
        repo,
    }
}

/// Fork the stage from a commit whose `src/lib.rs` is `content`: commit it
/// on the stage branch and move `main` (the stage base) there.
fn commit_base_lib(worktree: &std::path::Path, content: &str) {
    std::fs::write(worktree.join("src/lib.rs"), content).unwrap();
    for args in [
        vec!["add", "src/lib.rs"],
        vec!["commit", "-q", "-m", "lib"],
        vec!["update-ref", "refs/heads/main", "HEAD"],
    ] {
        let mut git = vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ];
        git.extend(args);
        let output = crate::git::runner::run_git(&git, worktree).unwrap();
        assert!(output.status.success(), "git {git:?}: {output:?}");
    }
}

fn refusal_of(fx: &Liquidmon) -> String {
    let error = check_site_changes(&fx.site, &fx.repo).expect_err("refused");
    format!("{error:#}")
}

#[test]
fn liquidmon_scaffold_passes_freeze_check() {
    let fx = liquidmon("");

    check_site_changes(&fx.site, &fx.repo).unwrap();
}

#[test]
fn a_function_in_the_created_module_root_is_refused() {
    let fx = liquidmon("");
    let root = fx.site.working_dir.join("src/device_lock/mod.rs");
    std::fs::write(
        root,
        "#[cfg(test)]\nmod contract_tests;\npub fn leak() {}\n",
    )
    .unwrap();

    let message = refusal_of(&fx);

    assert!(message.contains("src/device_lock/mod.rs"), "{message}");
    assert!(message.contains("pub fn leak() {}"), "{message}");
}

#[test]
fn an_edit_beyond_the_declaration_is_refused() {
    let fx = liquidmon("");
    let lib = fx.site.working_dir.join("src/lib.rs");
    std::fs::write(lib, "mod device_lock;\npub fn helper() {}\n").unwrap();

    let message = refusal_of(&fx);

    assert!(message.contains("src/lib.rs"), "{message}");
    assert!(message.contains("pub fn helper() {}"), "{message}");
}

/// The base already declares `mod device_lock;` in `src/lib.rs`, so the
/// scaffold is only the new module root, which the diff accepts. The
/// writer's harness covers `src/lib.rs`, and its inner `#![cfg(not(test))]`
/// (outside every scaffold edit) is the chain check's alone to refuse.
#[test]
fn a_chain_the_diff_accepts_but_the_chain_check_refuses_is_refused() {
    let mut fx = liquidmon("mod device_lock;\n");
    fx.site.stage.harness = vec!["src/lib.rs".to_string()];
    let lib = fx.site.working_dir.join("src/lib.rs");
    std::fs::write(lib, "#![cfg(not(test))]\nmod device_lock;\n").unwrap();

    let message = refusal_of(&fx);

    assert!(message.contains("#![cfg(not(test))]"), "{message}");
    assert!(message.contains("contract 'rejects-x'"), "{message}");
    assert!(!message.contains("nor a scaffold edit"), "{message}");
    assert!(!message.contains("src/device_lock/mod.rs"), "{message}");
}
