use super::*;
use crate::fs::permissions::scratch_home::ScratchHome;
use crate::test_git::{git, git_command, TestRepo};
use serial_test::serial;
use std::os::unix::ffi::OsStringExt;

fn repo() -> TestRepo {
    let repo = TestRepo::builder().no_seed().create();
    git(repo.path(), &["commit", "--allow-empty", "-m", "initial"]);
    repo
}

#[test]
fn preserves_unregistered_worktree_contents() {
    let repo = repo();
    let path = repo.path().join(".worktrees/s");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("unique"), "keep me").unwrap();
    for with_git in [false, true] {
        if with_git {
            std::fs::write(path.join(".git"), "invalid").unwrap();
        }
        let error = get_or_create_worktree("s", repo.path(), None, None).unwrap_err();
        assert!(format!("{error:#}").contains("preserved"));
        assert_eq!(
            std::fs::read_to_string(path.join("unique")).unwrap(),
            "keep me"
        );
    }
}

#[test]
#[serial]
fn recreates_empty_remnant_after_prune() {
    let repo = repo();
    let path = repo.path().join(".worktrees/s");
    std::fs::create_dir_all(&path).unwrap();
    let _home = ScratchHome::set();
    assert!(get_or_create_worktree("s", repo.path(), None, None).is_ok());
    assert!(path.join(".git").exists());
}

#[test]
fn prune_and_listing_errors_preserve_directory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".worktrees/s");
    std::fs::create_dir_all(&path).unwrap();
    let prune_error = get_or_create_worktree("s", root.path(), None, None).unwrap_err();
    assert!(format!("{prune_error:#}").contains("Failed to prune stale worktree registrations"));
    assert!(path.is_dir());
    std::fs::write(path.join(".git"), "invalid").unwrap();
    let listing_error = get_or_create_worktree("s", root.path(), None, None).unwrap_err();
    assert!(!format!("{listing_error:#}").contains("Failed to prune"));
    assert!(path.join(".git").exists());
}

#[test]
fn listing_preserves_real_git_paths_with_newline_and_non_utf8() {
    let repo = repo();
    let name = std::ffi::OsString::from_vec(b"linked\n\xff".to_vec());
    let path = repo.path().join(name);
    let added = git_command(repo.path())
        .args(["worktree", "add", "--detach"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(added.status.success(), "{added:?}");
    let entries = list_worktrees(repo.path()).unwrap();
    assert!(entries.iter().any(|entry| entry.path == path));
    assert!(is_valid_git_worktree(&path, repo.path()).unwrap());
}
