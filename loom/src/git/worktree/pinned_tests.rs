//! Pinned git reads the stage's registered git directory whatever the
//! worktree's `.git` file says, and refuses a worktree the repository does
//! not register.

use super::*;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
use tempfile::TempDir;

const STAGE: &str = "s1";

/// A repository at `<tmp>/repo` with the stage worktree `.worktrees/s1`.
fn repository() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, STAGE);
    (tmp, repo, worktree)
}

/// The git directory `repo` reports, as an absolute path.
/// The directory the pin names, confirmed against what git itself reads: a
/// command runs with a private directory, so `rev-parse --absolute-git-dir`
/// names that, and the real one is known by the `HEAD` git reports.
fn git_dir(pinned: &WorktreeGit) -> PathBuf {
    let real = pinned.git_dir_path().unwrap().to_path_buf();
    let head = pinned.run_checked(&["symbolic-ref", "HEAD"]).unwrap();
    let recorded = std::fs::read_to_string(real.join("HEAD")).unwrap();
    assert_eq!(format!("ref: {head}"), recorded);
    real
}

fn registered_git_dir(repo: &Path) -> PathBuf {
    repo.join(".git/worktrees")
        .join(STAGE)
        .canonicalize()
        .unwrap()
}

#[test]
fn main_checkout_uses_its_real_git_directory() {
    let (_tmp, repo, _worktree) = repository();
    let pinned = WorktreeGit::pinned_main(&repo).unwrap();
    assert_eq!(git_dir(&pinned), repo.join(".git").canonicalize().unwrap());
    assert!(pinned
        .run(&["status", "--porcelain"])
        .unwrap()
        .status
        .success());
}

#[test]
fn linked_operator_checkout_uses_its_registered_directories() {
    let (_tmp, repo, worktree) = repository();
    let pinned = WorktreeGit::pinned_main(&worktree).unwrap();
    assert_eq!(git_dir(&pinned), registered_git_dir(&repo));
    assert!(pinned
        .run(&["status", "--porcelain"])
        .unwrap()
        .status
        .success());
}

#[test]
fn main_checkout_refuses_a_git_pointer() {
    let (_tmp, _repo, worktree) = repository();
    let marker = plant_foreign_git_dir(&worktree);
    assert!(WorktreeGit::pinned_main(&worktree).is_err());
    assert!(!marker.exists());
}

#[test]
fn a_registered_worktree_runs_with_its_own_git_dir() {
    let (_tmp, repo, worktree) = repository();

    let pinned = WorktreeGit::pinned(&repo, &worktree).unwrap();

    assert_eq!(git_dir(&pinned), registered_git_dir(&repo));
    assert_eq!(pinned.work_tree(), worktree.canonicalize().unwrap());
}

/// The positive control runs last: git that follows the repointed `.git`
/// file does run the filter, so its absence before comes from the pin.
#[test]
fn a_repointed_git_file_neither_moves_nor_configures_pinned_git() {
    let (_tmp, repo, worktree) = repository();
    let marker = plant_foreign_git_dir(&worktree);

    let pinned = WorktreeGit::pinned(&repo, &worktree).unwrap();
    let status = pinned.run(&["status", "--porcelain"]).unwrap();

    assert!(status.status.success(), "{status:?}");
    assert!(!marker.exists(), "pinned git ran the stage's clean filter");
    assert_eq!(git_dir(&pinned), registered_git_dir(&repo));

    let followed = WorktreeGit::discovered(&worktree);
    followed.run(&["status", "--porcelain"]).unwrap();
    assert!(marker.exists(), "the fixture's filter never runs");
}

#[test]
fn a_directory_the_repository_does_not_register_is_refused() {
    let (tmp, repo, _worktree) = repository();
    let stranger = tmp.path().join("stranger");
    std::fs::create_dir(&stranger).unwrap();

    let error = WorktreeGit::pinned(&repo, &stranger).unwrap_err();

    let message = format!("{error:#}");
    assert!(
        message.contains("is not a registered worktree"),
        "{message}"
    );
}

/// An administrative directory whose `commondir` names another repository
/// is not this repository's worktree.
#[test]
fn a_git_dir_claiming_another_common_dir_is_refused() {
    let (tmp, repo, worktree) = repository();
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let commondir = repo.join(".git/worktrees").join(STAGE).join("commondir");
    std::fs::write(&commondir, format!("{}\n", elsewhere.display())).unwrap();

    let error = WorktreeGit::pinned(&repo, &worktree).unwrap_err();

    let message = format!("{error:#}");
    assert!(
        message.contains("does not belong to the repository"),
        "{message}"
    );
}
