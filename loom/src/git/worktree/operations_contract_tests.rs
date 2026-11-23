//! Contracts for creating a stage worktree: repairing one stage's remnant
//! touches no other registration, and a failed scaffold leaves nothing a
//! retry would mistake for a finished worktree.

use super::operations::fail_next_scaffold;
use super::{get_or_create_worktree, list_worktrees};
use crate::fs::permissions::scratch_home::ScratchHome;
use crate::test_git::{git, TestRepo};
use serial_test::serial;
use std::path::Path;

fn repo() -> TestRepo {
    let repo = TestRepo::builder().no_seed().create();
    git(repo.path(), &["commit", "--allow-empty", "-m", "initial"]);
    repo
}

#[test]
#[serial]
fn repairing_an_empty_remnant_keeps_an_unrelated_registration() {
    let repo = repo();
    let root = repo.path();
    let outside = tempfile::TempDir::new().unwrap();
    let other = outside.path().join("other");
    git(
        root,
        &["worktree", "add", "--detach", other.to_str().unwrap()],
    );
    git(&other, &["commit", "--allow-empty", "-m", "other work"]);
    std::fs::remove_dir_all(&other).unwrap();
    std::fs::create_dir_all(root.join(".worktrees/s")).unwrap();
    let _home = ScratchHome::set();

    let created = get_or_create_worktree("s", root, None, None);

    assert!(created.is_ok(), "{created:?}");
    assert!(root.join(".git/worktrees/other/HEAD").exists());
}

fn is_stage_entry(path: &Path) -> bool {
    path.ends_with(".worktrees/s")
}

#[test]
#[serial]
fn a_failed_scaffold_leaves_no_registration_a_retry_accepts() {
    let repo = repo();
    let root = repo.path();
    let state = crate::fs::work_dir::WorkDir::new(root)
        .unwrap()
        .root()
        .to_path_buf();
    std::fs::create_dir_all(&state).unwrap();
    let relative = state
        .canonicalize()
        .unwrap()
        .strip_prefix(root.canonicalize().unwrap())
        .unwrap()
        .to_path_buf();
    let _home = ScratchHome::set();
    fail_next_scaffold();

    let failed = get_or_create_worktree("s", root, None, None);

    assert!(
        failed.is_err(),
        "the failed scaffold was reported as {failed:?}"
    );
    assert!(!root.join(".worktrees/s").exists());
    let entries = list_worktrees(root).unwrap();
    assert!(
        !entries.iter().any(|entry| is_stage_entry(&entry.path)),
        "{entries:?}"
    );
    let worktree = get_or_create_worktree("s", root, None, None).unwrap();
    let link = std::fs::symlink_metadata(worktree.path.join(&relative)).unwrap();
    assert!(link.file_type().is_symlink(), "{link:?}");
}
