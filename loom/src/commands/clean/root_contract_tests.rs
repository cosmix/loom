//! Contracts: `loom clean` acts on the repository root from any directory
//! below it, and refuses to run from a linked stage worktree.

use super::execute;
use crate::fs::permissions::scratch_home::ScratchHome;
use crate::git::target_guard::test_support::{git, repo};
use serial_test::serial;
use std::path::{Path, PathBuf};

/// Enters `dir` and returns to the previous directory on drop.
struct CwdGuard(PathBuf);

impl CwdGuard {
    fn enter(dir: &Path) -> Self {
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self(original)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

#[test]
#[serial]
fn all_from_a_subdirectory_cleans_the_repository_root() {
    let _home = ScratchHome::set();
    let repo = repo();
    let subdir = repo.root.join("src");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::create_dir_all(repo.work.join("memory")).unwrap();
    std::fs::write(repo.work.join("memory/stage.md"), "journal").unwrap();
    let stage_dir = repo.root.join(".worktrees/stage-a");
    std::fs::create_dir_all(&stage_dir).unwrap();
    std::fs::write(stage_dir.join("keep.txt"), "work").unwrap();
    let cwd = CwdGuard::enter(&subdir);

    let result = execute(true, false, false, false);
    drop(cwd);

    result.expect("clean --all from a subdirectory must succeed");
    assert!(!repo.work.exists(), "the state directory was left behind");
    assert!(!stage_dir.exists(), "the stage worktree directory was left");
    let archive_root = repo.root.join(".loom").join("memory").join("archive");
    let archives: Vec<PathBuf> = std::fs::read_dir(&archive_root)
        .unwrap_or_else(|e| panic!("no archive at {}: {e}", archive_root.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(archives.len(), 1, "{archives:?}");
    assert!(
        archives[0].join("memory/stage.md").is_file(),
        "{archives:?}"
    );
    assert!(
        !subdir.join(".loom").exists(),
        "clean wrote state under the subdirectory"
    );
}

#[test]
#[serial]
fn clean_from_a_stage_worktree_is_refused() {
    let _home = ScratchHome::set();
    let repo = repo();
    git(
        &repo.root,
        &["worktree", "add", "-q", ".worktrees/s1", "-b", "loom/s1"],
    );
    let worktree = repo.root.join(".worktrees/s1");
    let worktree_loom = worktree.join(".loom");
    std::fs::create_dir_all(&worktree_loom).unwrap();
    let link = worktree_loom.join("work");
    std::os::unix::fs::symlink(&repo.work, &link).unwrap();
    let cwd = CwdGuard::enter(&worktree);

    let result = execute(false, false, false, true);
    drop(cwd);

    let error = format!(
        "{:#}",
        result.expect_err("clean from a worktree must refuse")
    );
    assert!(error.contains("linked worktree"), "{error}");
    assert!(repo.work.is_dir(), "the main state directory was removed");
    assert!(
        std::fs::symlink_metadata(&link).is_ok(),
        "the worktree's state symlink was removed"
    );
}
