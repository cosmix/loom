use super::*;
use crate::fs::permissions::scratch_home::ScratchHome;
use crate::test_git::{git, TestRepo};
use serial_test::serial;

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
fn recreates_an_empty_remnant_without_pruning_other_registrations() {
    let repo = repo();
    let path = repo.path().join(".worktrees/s");
    std::fs::create_dir_all(&path).unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let other = outside.path().join("other");
    git(
        repo.path(),
        &["worktree", "add", "--detach", other.to_str().unwrap()],
    );
    git(&other, &["commit", "--allow-empty", "-m", "other work"]);
    std::fs::remove_dir_all(&other).unwrap();
    let _home = ScratchHome::set();
    assert!(get_or_create_worktree("s", repo.path(), None, None).is_ok());
    assert!(path.join(".git").exists());
    assert!(repo.path().join(".git/worktrees/other/HEAD").exists());
}

#[test]
fn listing_errors_preserve_directory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".worktrees/s");
    std::fs::create_dir_all(&path).unwrap();
    let empty_error = get_or_create_worktree("s", root.path(), None, None).unwrap_err();
    assert!(!format!("{empty_error:#}").contains("Failed to prune"));
    assert!(path.is_dir());
    std::fs::write(path.join(".git"), "invalid").unwrap();
    let listing_error = get_or_create_worktree("s", root.path(), None, None).unwrap_err();
    assert!(!format!("{listing_error:#}").contains("Failed to prune"));
    assert!(path.join(".git").exists());
}

#[test]
#[serial]
fn a_registered_empty_remnant_is_refused_and_kept() {
    let repo = repo();
    let _home = ScratchHome::set();
    let created = get_or_create_worktree("s", repo.path(), None, None).unwrap();
    for entry in std::fs::read_dir(&created.path).unwrap() {
        let entry_path = entry.unwrap().path();
        if entry_path.is_dir() && !entry_path.is_symlink() {
            std::fs::remove_dir_all(&entry_path).unwrap();
        } else {
            std::fs::remove_file(&entry_path).unwrap();
        }
    }
    let error = get_or_create_worktree("s", repo.path(), None, None).unwrap_err();
    assert!(
        format!("{error:#}").contains("still registered"),
        "{error:#}"
    );
    assert!(created.path.is_dir());
    assert!(repo.path().join(".git/worktrees/s").exists());
}

#[test]
#[serial]
fn a_failed_scaffold_removes_the_registration_and_a_retry_scaffolds() {
    let repo = repo();
    std::fs::create_dir_all(repo.path().join(".loom/work")).unwrap();
    let _home = ScratchHome::set();
    super::fail_next_scaffold();
    let error = get_or_create_worktree("s", repo.path(), None, None).unwrap_err();
    assert!(
        format!("{error:#}").contains("simulated scaffold failure"),
        "{error:#}"
    );
    let path = repo.path().join(".worktrees/s");
    assert!(!path.exists());
    let entries = list_worktrees(repo.path()).unwrap();
    assert!(!entries.iter().any(|e| e.path.ends_with(".worktrees/s")));
    let retried = get_or_create_worktree("s", repo.path(), None, None).unwrap();
    let link = std::fs::symlink_metadata(retried.path.join(".loom/work")).unwrap();
    assert!(link.file_type().is_symlink());
}

// APFS rejects file names that are not valid UTF-8 (EILSEQ), so the fixture
// cannot exist on macOS.
#[cfg(not(target_os = "macos"))]
#[test]
fn listing_preserves_real_git_paths_with_newline_and_non_utf8() {
    use crate::test_git::git_command;
    use std::os::unix::ffi::OsStringExt;
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
