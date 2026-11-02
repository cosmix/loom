use super::*;
use crate::git::branch::pinned_uncommitted_changes;
use crate::git::worktree::capture_main_identity;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
use std::fs;
use tempfile::TempDir;

fn anchor(root: &Path) {
    let state = root.join(".loom/work");
    fs::create_dir_all(&state).unwrap();
    capture_main_identity(root, &state).unwrap();
}

#[test]
fn startup_preserves_linked_checkout_identity_after_pointer_forgery() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    let linked = contract_worktree(&root, "operator");
    anchor(&linked);
    assert_eq!(
        ensure_pinned_repo_ready(&linked).unwrap(),
        RepoBootstrapResult::default()
    );
    assert!(!pinned_uncommitted_changes(&linked).unwrap().0);

    let marker = plant_foreign_git_dir(&linked);
    assert_eq!(
        ensure_pinned_repo_ready(&linked).unwrap(),
        RepoBootstrapResult::default()
    );
    let (dirty, summary) = pinned_uncommitted_changes(&linked).unwrap();
    assert!(dirty);
    assert!(summary.contains("README.md"), "{summary}");
    assert!(
        !marker.exists(),
        "startup executed foreign repository configuration"
    );

    WorktreeGit::discovered(&linked)
        .run(&["status", "--porcelain"])
        .unwrap();
    assert!(
        marker.exists(),
        "foreign filter positive control did not execute"
    );
}

#[test]
fn startup_bootstrap_preserves_preexisting_staged_content() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let git = WorktreeGit::discovered(root);
    git.run_checked(&["init"]).unwrap();
    git.run_checked(&["config", "user.name", "Test"]).unwrap();
    git.run_checked(&["config", "user.email", "test@example.com"])
        .unwrap();
    git.run_checked(&["config", "commit.gpgsign", "false"])
        .unwrap();
    fs::write(root.join("pending.txt"), "pending\n").unwrap();
    git.run_checked(&["add", "pending.txt"]).unwrap();
    anchor(root);

    let result = ensure_pinned_repo_ready(root).unwrap();
    assert!(result.created_initial_commit);
    assert!(!result.initialized_repo);
    let pinned = WorktreeGit::pinned_main(root).unwrap();
    assert_eq!(
        pinned
            .run_checked(&["ls-tree", "--name-only", "HEAD"])
            .unwrap(),
        "README.md\n"
    );
    let (dirty, summary) = pinned_uncommitted_changes(root).unwrap();
    assert!(dirty);
    assert_eq!(summary, "Staged: pending.txt\n");
    assert_eq!(
        fs::read_to_string(root.join("pending.txt")).unwrap(),
        "pending\n"
    );
}

#[test]
fn startup_refuses_missing_anchor_without_repairing_repository() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/config"), "unreadable configuration\n").unwrap();
    fs::create_dir_all(root.join(".loom/work")).unwrap();
    assert!(ensure_pinned_repo_ready(root).is_err());
    assert!(pinned_uncommitted_changes(root).is_err());
    assert_eq!(
        fs::read_to_string(root.join(".git/config")).unwrap(),
        "unreadable configuration\n"
    );
    assert!(!root.join("README.md").exists());
}

#[test]
fn startup_status_ignores_untracked_files_but_reports_tracked_changes() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    contract_worktree(&root, "stage");
    fs::write(root.join("scratch.txt"), "scratch\n").unwrap();
    assert_eq!(
        pinned_uncommitted_changes(&root).unwrap(),
        (false, String::new())
    );
    fs::write(root.join("README.md"), "changed\n").unwrap();
    assert_eq!(
        pinned_uncommitted_changes(&root).unwrap(),
        (true, "Modified: README.md\n".to_string())
    );
}
