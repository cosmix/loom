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
fn git_dir(repo: &WorktreeGit) -> PathBuf {
    let output = repo.run(&["rev-parse", "--absolute-git-dir"]).unwrap();
    assert!(output.status.success(), "{output:?}");
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

fn registered_git_dir(repo: &Path) -> PathBuf {
    repo.join(".git/worktrees")
        .join(STAGE)
        .canonicalize()
        .unwrap()
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
