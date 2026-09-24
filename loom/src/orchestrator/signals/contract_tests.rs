//! Tests for the contract signal.

use super::*;
use crate::models::stage::StageType;

fn contract(id: &str, file: &str, test: &str, runner: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        test: test.to_string(),
        runner: Some(runner.to_string()),
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
    assert!(content.contains("- `no-follow`: `cargo test fs::rejects -- --exact`"));
    assert!(content.contains("- `shell-check`: `check`"));
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
