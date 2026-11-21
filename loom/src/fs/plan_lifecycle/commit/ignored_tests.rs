//! The post-completion commit in a project whose `.gitignore` may ignore the
//! finished plan: the removal of the in-progress plan is committed either way,
//! the finished plan only when git does not ignore it.

use super::*;
use crate::git::target_guard::test_support::{commit_file, git, repo, Repo};

const OLD: &str = "doc/plans/IN_PROGRESS-PLAN-x.md";
const NEW: &str = "doc/plans/DONE-PLAN-x.md";

/// A repository with `.gitignore` committed, holding `ignore` when given, and
/// the in-progress plan committed when `tracked`, else only written.
fn plan_repo(ignore: Option<&str>, tracked: bool) -> Repo {
    let repo = repo();
    if let Some(pattern) = ignore {
        commit_file(&repo.root, ".gitignore", &format!("{pattern}\n"));
    }
    if tracked {
        commit_file(&repo.root, OLD, "plan\n");
    } else {
        std::fs::create_dir_all(repo.root.join("doc/plans")).unwrap();
        std::fs::write(repo.root.join(OLD), "plan\n").unwrap();
    }
    repo
}

/// Rename the plan and commit, as `rename_completed_plan` does.
fn complete(repo: &Repo) {
    std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
    let work_dir = WorkDir::new(&repo.root).unwrap();
    commit_post_completion_changes(&work_dir, &repo.root.join(OLD), &repo.root.join(NEW)).unwrap();
}

fn head_changes(repo: &Repo) -> String {
    git(&repo.root, &["show", "--name-status", "--format=", "HEAD"])
}

fn plan_status(repo: &Repo) -> String {
    git(&repo.root, &["status", "--porcelain", "--", OLD, NEW])
}

#[test]
fn an_ignored_done_plan_commits_the_removal_alone() {
    let repo = plan_repo(Some("doc/plans/DONE-*"), true);
    let before = git(&repo.root, &["rev-parse", "HEAD"]);

    complete(&repo);

    assert_ne!(git(&repo.root, &["rev-parse", "HEAD"]), before);
    assert_eq!(head_changes(&repo), format!("D\t{OLD}"));
    assert_eq!(plan_status(&repo), "");
    assert!(repo.root.join(NEW).is_file());
}

#[test]
fn a_done_plan_git_does_not_ignore_commits_both_sides_of_the_rename() {
    let repo = plan_repo(Some("target/"), true);

    complete(&repo);

    let changes = head_changes(&repo);
    assert!(changes.starts_with('R'), "{changes}");
    assert!(changes.contains(OLD) && changes.contains(NEW), "{changes}");
    assert_eq!(plan_status(&repo), "");
}

/// `check-ignore` exits 128 when `GIT_DIR` names no repository; that is an
/// error, never a verdict on the path.
#[test]
fn a_failing_check_ignore_is_an_error() {
    let repo = plan_repo(None, true);
    let missing = repo.root.join("no-such-git-dir");
    let env = [("GIT_DIR", missing.as_os_str())];

    let error = is_ignored(&repo.root, NEW, &env).unwrap_err().to_string();

    assert!(error.contains("git check-ignore"), "{error}");
}

#[test]
fn an_untracked_plan_finished_into_an_ignored_path_commits_nothing() {
    let repo = plan_repo(Some("doc/plans/DONE-*"), false);
    let before = git(&repo.root, &["rev-parse", "HEAD"]);

    complete(&repo);

    assert_eq!(git(&repo.root, &["rev-parse", "HEAD"]), before);
    assert_eq!(plan_status(&repo), "");
    assert_eq!(git(&repo.root, &["diff", "--cached", "--name-only"]), "");
}
