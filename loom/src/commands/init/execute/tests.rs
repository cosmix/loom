//! `loom init --clean` against a held target.

use super::warn_on_long_socket_path;
use super::{execute, stop_daemon_and_prune};
use crate::git::target_guard::test_support::held_repo;
use crate::git::target_guard::RECORD_FILE;
use serial_test::serial;
use std::path::{Path, PathBuf};

/// Enters `dir` and returns to the previous directory on drop, a panic
/// included: `execute` works from the current directory.
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
fn clean_refuses_before_stopping_anything_while_the_target_is_held() {
    let (repo, _accepted, _moved) = held_repo(true);

    let error = stop_daemon_and_prune(&repo.root, true).unwrap_err();

    assert!(error.to_string().contains("loom target status"), "{error}");
    assert!(repo.work.join("target-guard.json").is_file());
}

#[test]
#[serial]
fn init_clean_refuses_through_execute_and_deletes_nothing() {
    let (repo, _accepted, _moved) = held_repo(true);
    let worktree_file = repo.root.join(".worktrees/stage-a/keep.txt");
    std::fs::create_dir_all(worktree_file.parent().unwrap()).unwrap();
    std::fs::write(&worktree_file, "work").unwrap();
    let state_file = repo.work.join("keep.txt");
    std::fs::write(&state_file, "state").unwrap();
    let record = std::fs::read(repo.work.join(RECORD_FILE)).unwrap();
    let _cwd = CwdGuard::enter(&repo.root);

    let error = execute(None, true, None, true).unwrap_err().to_string();

    assert!(
        error.contains("the target branch main has a move loom did not accept"),
        "{error}"
    );
    assert!(error.contains("touches .claude/settings.json"), "{error}");
    assert_eq!(std::fs::read(repo.work.join(RECORD_FILE)).unwrap(), record);
    assert!(worktree_file.is_file());
    assert!(state_file.is_file());
}

#[test]
fn a_work_root_whose_socket_path_cannot_fit_is_warned_about() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let short = root.join("w");
    let long = root.join("l".repeat(crate::daemon::SUN_PATH_MAX));
    std::fs::create_dir(&short).unwrap();
    std::fs::create_dir(&long).unwrap();

    let message = warn_on_long_socket_path(&long).expect("a long work root is warned about");

    assert!(message.contains(&long.display().to_string()));
    assert_eq!(warn_on_long_socket_path(&short), None);
}
