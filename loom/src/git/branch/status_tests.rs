//! Tests for working tree status checks, against real temporary repositories.

use super::*;
use std::process::Command;
use tempfile::TempDir;

fn init_test_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Create initial commit
    std::fs::write(repo_path.join("file1.txt"), "content1").unwrap();
    Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    temp_dir
}

#[test]
fn test_has_uncommitted_changes_clean_repo() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    assert!(!has_uncommitted_changes(repo_path).unwrap());
}

#[test]
fn test_has_uncommitted_changes_staged_file() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
    Command::new("git")
        .args(["add", "file2.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    assert!(has_uncommitted_changes(repo_path).unwrap());
}

#[test]
fn test_has_uncommitted_changes_modified_file() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("file1.txt"), "modified content").unwrap();

    assert!(has_uncommitted_changes(repo_path).unwrap());
}

#[test]
fn test_has_uncommitted_changes_untracked_only() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("untracked.txt"), "untracked content").unwrap();

    // Untracked files should NOT be considered uncommitted changes
    assert!(!has_uncommitted_changes(repo_path).unwrap());
}

#[test]
fn test_list_working_tree_changes_clean_repo() {
    let temp_dir = init_test_repo();

    assert!(list_working_tree_changes(temp_dir.path())
        .unwrap()
        .is_empty());
}

#[test]
fn test_list_working_tree_changes_includes_untracked() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("new_module.rs"), "fn feature() {}").unwrap();

    // An agent's brand-new file is untracked but is real work — unlike
    // has_uncommitted_changes, this must see it.
    assert_eq!(
        list_working_tree_changes(repo_path).unwrap(),
        vec!["new_module.rs".to_string()]
    );
    assert!(!has_uncommitted_changes(repo_path).unwrap());
}

#[test]
fn test_list_working_tree_changes_includes_modified() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("file1.txt"), "modified content").unwrap();

    assert_eq!(
        list_working_tree_changes(repo_path).unwrap(),
        vec!["file1.txt".to_string()]
    );
}

#[test]
fn test_list_working_tree_changes_reports_rename_destination() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    Command::new("git")
        .args(["mv", "file1.txt", "renamed.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    assert_eq!(
        list_working_tree_changes(repo_path).unwrap(),
        vec!["renamed.txt".to_string()]
    );
}

#[test]
fn test_list_working_tree_changes_omits_ignored_files() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join(".gitignore"), "generated.txt\n").unwrap();
    Command::new("git")
        .args(["add", ".gitignore"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "Add gitignore"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    std::fs::write(repo_path.join("generated.txt"), "build artifact").unwrap();

    assert!(list_working_tree_changes(repo_path).unwrap().is_empty());
}

/// Nothing an unprivileged process can make is a device node: a FIFO is
/// kept, like a regular file, a directory and a symlink.
#[test]
fn device_nodes_are_character_and_block_devices_only() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    nix::unistd::mkfifo(&root.join("fifo"), nix::sys::stat::Mode::S_IRWXU).unwrap();
    std::fs::write(root.join("file"), "x").unwrap();
    std::fs::create_dir(root.join("dir")).unwrap();
    std::os::unix::fs::symlink("/dev/null", root.join("link")).unwrap();

    assert!(is_device_node(Path::new("/dev"), "null"));
    for path in ["fifo", "file", "dir", "dir/", "link", "missing"] {
        assert!(!is_device_node(root, path), "{path}");
    }
}

/// Inside the sandbox git lists a `/dev/null` mount point as an untracked
/// regular file, so the filter is checked against the output git gives
/// there, with `/dev/null` itself standing in for the mount.
#[test]
fn test_list_working_tree_changes_omits_untracked_device_nodes() {
    let porcelain = "?? null\n?? new_module.rs\n M file1.txt\n";
    assert_eq!(
        working_tree_changes(Path::new("/dev"), porcelain),
        vec!["new_module.rs".to_string(), "file1.txt".to_string()]
    );
}

#[test]
fn test_get_uncommitted_changes_summary_clean_repo() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    let summary = get_uncommitted_changes_summary(repo_path).unwrap();
    assert!(summary.is_empty());
}

#[test]
fn test_get_uncommitted_changes_summary_staged_file() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
    Command::new("git")
        .args(["add", "file2.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let summary = get_uncommitted_changes_summary(repo_path).unwrap();
    assert!(summary.contains("Staged:"));
    assert!(summary.contains("file2.txt"));
}

#[test]
fn test_get_uncommitted_changes_summary_modified_file() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("file1.txt"), "modified content").unwrap();

    let summary = get_uncommitted_changes_summary(repo_path).unwrap();
    assert!(summary.contains("Modified:"));
    assert!(summary.contains("file1.txt"));
}

#[test]
fn test_get_uncommitted_changes_summary_both_staged_and_modified() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("file2.txt"), "content2").unwrap();
    Command::new("git")
        .args(["add", "file2.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    std::fs::write(repo_path.join("file1.txt"), "modified content").unwrap();

    let summary = get_uncommitted_changes_summary(repo_path).unwrap();
    assert!(summary.contains("Staged:"));
    assert!(summary.contains("file2.txt"));
    assert!(summary.contains("Modified:"));
    assert!(summary.contains("file1.txt"));
}

#[test]
fn test_get_uncommitted_changes_summary_untracked_only() {
    let temp_dir = init_test_repo();
    let repo_path = temp_dir.path();

    std::fs::write(repo_path.join("untracked.txt"), "untracked content").unwrap();

    let summary = get_uncommitted_changes_summary(repo_path).unwrap();
    assert!(summary.is_empty());
}
