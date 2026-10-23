//! Tests for the contract signal.

use super::*;
use crate::models::stage::StageType;
use crate::verify::contracts::test_support::{contract_worktree, liquidmon_contract};

fn contract(id: &str, file: &str, test: &str, runner: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        test: test.to_string(),
        runner: Some(runner.to_string()),
        cwd: None,
        scenario: "a symlink | a regular file".to_string(),
        rejects: "following the link".to_string(),
    }
}

fn stage() -> Stage {
    Stage {
        id: "s1".to_string(),
        name: "Stage One".to_string(),
        description: Some("Refuse to follow symlinks.".to_string()),
        plan_version: 2,
        stage_type: StageType::Standard,
        contracts: vec![
            contract(
                "no-follow",
                "src/fs_contract_tests.rs",
                "fs::rejects",
                "cargo-test",
            ),
            contract("shell-check", "check.sh", "check", "no-such-runner"),
        ],
        harness: vec!["tests/fixtures/**".to_string()],
        ..Stage::default()
    }
}

#[test]
fn contract_signal_carries_contracts_rules_and_freeze_command() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let worktree = Worktree::new(
        "s1".to_string(),
        temp.path().to_path_buf(),
        "loom/s1".to_string(),
    );
    let session = Session::new_contract("s1");

    let path = generate_contract_signal(&session, &stage(), &worktree, &work, None, &[]).unwrap();

    assert_eq!(
        path,
        work.join("signals").join(format!("{}.md", session.id))
    );
    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.starts_with("# Contract Signal:"));
    assert!(content.contains(
        "| `no-follow` | `src/fs_contract_tests.rs` | `fs::rejects` | cargo-test | \
         a symlink \\| a regular file | following the link |"
    ));
    assert!(content.contains("- `no-follow` in `.`: `cargo test fs::rejects -- --exact`"));
    assert!(content.contains("- `shell-check` in `.`: `check`"));
    assert!(content.contains("Unsupported runner (`no-such-runner` is not a known runner)"));
    assert!(content.contains("Refuse to follow symlinks."));
    assert!(content.contains("`tests/fixtures/**`"));
    assert!(content.contains(r#"Skill(skill="loom-skills", args="loom-rust")"#));
    assert!(content.contains("Implement nothing"));
    assert!(content.contains("must fail now"));
    assert!(content.contains("loom stage contracts freeze s1"));
    assert!(
        !content.contains("## Acceptance Criteria"),
        "the stage's completion is not the contract writer's job"
    );
}

#[test]
fn a_contract_in_a_sub_workspace_runs_from_its_runner_config() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    let web = temp.path().join("apps/web");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("bunfig.toml"), "[test]\n").unwrap();
    let worktree = Worktree::new(
        "s1".to_string(),
        temp.path().to_path_buf(),
        "loom/s1".to_string(),
    );
    let stage = Stage {
        contracts: vec![contract(
            "widget",
            "apps/web/src/a.contract.test.ts",
            "widget renders",
            "bun-test",
        )],
        ..stage()
    };
    let session = Session::new_contract("s1");

    let path = generate_contract_signal(&session, &stage, &worktree, &work, None, &[]).unwrap();

    let content = std::fs::read_to_string(path).unwrap();
    let line = content
        .lines()
        .find(|line| line.starts_with("- `widget`"))
        .unwrap();
    assert!(
        line.starts_with("- `widget` in `apps/web`: `bun test "),
        "{line}"
    );
    assert!(line.contains("src/a.contract.test.ts"), "{line}");
    assert!(!line.contains("apps/web/src"), "{line}");
}

#[test]
fn contract_rules_name_the_formatter_and_size_limits() {
    let mut content = String::new();
    append_rules(&mut content, "s1");

    assert!(content.contains("6. Before freezing, run the repository's formatter"));
    assert!(content.contains("`cargo fmt --all`"));
    assert!(content.contains("every test function under 50 lines and every file under 400"));
    assert!(content.contains("7. Finish with `loom stage contracts freeze s1`"));
    assert!(content.contains("then stop. If it refuses, fix what it names"));
}

#[test]
fn contract_rules_route_disputes_through_memory_notes_not_commands() {
    let mut content = String::new();
    append_rules(&mut content, "s1");

    assert!(content.contains("`loom memory note \"freeze-dispute: <evidence>\"`"));
    assert!(content.contains("`loom memory note \"freeze-dispute: answer <n>: <evidence>\"`"));
    assert!(content.contains("do not ask the operator to amend contracts, harness or criteria"));
    assert!(content.contains("`loom stage block s1 "));
    assert!(!content.contains("loom stage dispute"));
}

#[test]
fn the_signal_carries_adjudicator_feedback_only_when_a_notice_exists() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let worktree = Worktree::new("s1".to_string(), temp.path().to_path_buf(), "b".to_string());
    let stage = stage();
    assert_eq!(stage.tally.contract_disputes, 0);
    let generate = || {
        let session = Session::new_contract("s1");
        let path = generate_contract_signal(&session, &stage, &worktree, &work, None, &[]).unwrap();
        std::fs::read_to_string(path).unwrap()
    };

    let before = generate();
    assert!(!before.contains("## Adjudicator Feedback"), "{before}");

    crate::orchestrator::adjudication::feedback::write_notice(&work, "s1", "Move the fixture.\n")
        .unwrap();
    let after = generate();
    let feedback = after
        .find("## Adjudicator Feedback (from the refused freeze)")
        .unwrap();
    assert!(after.contains("Move the fixture."), "{after}");
    assert!(feedback < after.find("## Your Job").unwrap(), "{after}");
}

/// The signal of a stage over a real repository whose base holds a package.
fn signal_over_repo(contracts: Vec<ContractSpec>) -> String {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let root = contract_worktree(&repo, "s1");
    let workspace = crate::fs::work_dir::WorkDir::new(&repo).unwrap();
    workspace.adopt_existing().unwrap();
    let worktree = Worktree::new("s1".to_string(), root, "loom/s1".to_string());
    let stage = Stage {
        contracts,
        harness: Vec::new(),
        ..stage()
    };
    let session = Session::new_contract("s1");
    let path =
        generate_contract_signal(&session, &stage, &worktree, workspace.root(), None, &[]).unwrap();
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn the_signal_lists_the_scaffold_a_new_module_needs() {
    let content = signal_over_repo(vec![liquidmon_contract()]);

    assert!(content.contains("## Scaffold"), "{content}");
    assert!(
        content.contains("- In `src/lib.rs`, add `mod device_lock;`."),
        "{content}"
    );
    assert!(
        content.contains(
            "- Create `src/device_lock/mod.rs` or `src/device_lock.rs` holding only \
             `#[cfg(test)]` and `mod contract_tests;`."
        ),
        "{content}"
    );
    assert!(content.contains("These edits are not frozen"), "{content}");
}

#[test]
fn the_signal_says_when_no_scaffold_is_needed() {
    let content = signal_over_repo(vec![crate::verify::contracts::test_support::contract()]);

    assert!(
        content.contains("No scaffold edits: every contract file is already collected."),
        "{content}"
    );
}

#[test]
fn an_unresolvable_base_is_reported_not_derived_from_the_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let worktree = Worktree::new("s1".to_string(), temp.path().to_path_buf(), "b".to_string());
    let session = Session::new_contract("s1");

    let path = generate_contract_signal(&session, &stage(), &worktree, &work, None, &[]).unwrap();

    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("Scaffold unknown:"), "{content}");
    assert!(
        content.contains("the freeze refuses until the stage base resolves"),
        "{content}"
    );
}
