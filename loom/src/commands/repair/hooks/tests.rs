//! The reference-transaction hook arm of `loom repair`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{check, REFERENCE_TRANSACTION_ISSUE};
use crate::commands::repair::{fix_issue, workspace::WorkspaceFix, RepairAction};
use crate::git::hooks::{
    install_reference_transaction_hook, is_reference_transaction_hook_installed,
};

const FOREIGN_HOOK: &str = "#!/bin/sh\necho another tool\n";

/// A directory with `.git/hooks` and no hooks in it.
fn repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".git/hooks")).unwrap();
    dir
}

fn hook_path(root: &Path) -> PathBuf {
    root.join(".git/hooks/reference-transaction")
}

/// The descriptions of the reference-transaction issues `check` reports.
fn hook_issues(root: &Path) -> Vec<String> {
    check(root)
        .into_iter()
        .map(|issue| issue.description)
        .filter(|description| description.starts_with(REFERENCE_TRANSACTION_ISSUE))
        .collect()
}

#[test]
fn a_missing_hook_is_reported_and_repair_installs_it() {
    let repo = repo();
    let root = repo.path();
    assert_eq!(
        hook_issues(root),
        ["Git reference-transaction hook not installed"]
    );

    let issue = super::reference_transaction_issue(root).unwrap();
    assert_eq!(
        issue.action,
        RepairAction::Workspace(WorkspaceFix::ReferenceTransactionHook)
    );
    assert!(fix_issue(root, &issue).unwrap());
    assert!(is_reference_transaction_hook_installed(root));
    assert!(hook_issues(root).is_empty());
    assert!(
        !fix_issue(root, &issue).unwrap(),
        "a second repair finds the hook in place"
    );
}

#[test]
fn a_foreign_hook_is_reported_and_never_overwritten() {
    let repo = repo();
    let root = repo.path();
    fs::write(hook_path(root), FOREIGN_HOOK).unwrap();
    let description = "Git reference-transaction hook belongs to another tool";
    assert_eq!(hook_issues(root), [description]);

    let issue = super::reference_transaction_issue(root).unwrap();
    assert_eq!(issue.action, RepairAction::Manual);
    assert!(!fix_issue(root, &issue).unwrap());
    let error = super::fix_reference_transaction_hook(root)
        .unwrap_err()
        .to_string();
    assert!(error.contains("belongs to another tool"), "{error}");
    assert_eq!(fs::read_to_string(hook_path(root)).unwrap(), FOREIGN_HOOK);
}

#[test]
fn a_symlink_with_a_missing_target_is_reported_as_foreign() {
    let repo = repo();
    let root = repo.path();
    std::os::unix::fs::symlink(root.join("missing"), hook_path(root)).unwrap();

    assert_eq!(
        hook_issues(root),
        ["Git reference-transaction hook belongs to another tool"]
    );
    assert!(!root.join("missing").exists());
}

/// Repair must say loom cannot act, and must not report a hook git never runs.
#[test]
fn a_hooks_path_outside_the_repository_is_reported_and_repair_writes_nothing() {
    let shared = TempDir::new().unwrap();
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    let configured = shared.path().to_str().unwrap();
    git(root, &["config", "core.hooksPath", configured]);
    install_reference_transaction_hook(root).unwrap_err();

    let issues: Vec<_> = check(root)
        .into_iter()
        .filter(|issue| issue.description.contains("hook not installed:"))
        .collect();

    assert_eq!(issues.len(), 2, "{issues:?}");
    for issue in issues {
        assert_eq!(issue.action, RepairAction::Manual, "{issue:?}");
        assert!(issue.description.contains(configured), "{issue:?}");
        assert!(
            issue.description.contains("outside this repository"),
            "{issue:?}"
        );
        assert!(!fix_issue(root, &issue).unwrap());
    }
    assert_eq!(fs::read_dir(shared.path()).unwrap().count(), 0);
}

#[test]
fn a_hooks_path_inside_the_repository_is_where_repair_installs() {
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "core.hooksPath", ".git/custom-hooks"]);

    let issue = super::reference_transaction_issue(root).unwrap();
    assert!(fix_issue(root, &issue).unwrap());

    assert!(root
        .join(".git/custom-hooks/reference-transaction")
        .is_file());
    assert!(!hook_path(root).exists());
    assert!(hook_issues(root).is_empty());
}

fn git(root: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
}

#[test]
fn a_loom_hook_raises_no_issue() {
    let repo = repo();
    install_reference_transaction_hook(repo.path()).unwrap();

    assert!(hook_issues(repo.path()).is_empty());
}

#[test]
fn outside_a_repository_there_is_no_hook_issue() {
    let dir = TempDir::new().unwrap();

    assert!(hook_issues(dir.path()).is_empty());
}
