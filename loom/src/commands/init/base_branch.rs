//! The branch `loom init` records as the plan's merge target.

use crate::git::branch::current_branch;
use crate::git::revision::has_commits;
use crate::git::worktree::WorktreeGit;
use anyhow::{bail, Context, Result};
use std::path::Path;

/// The branch checked out in `repo_root`. Refuses a detached HEAD: stages
/// merge into a named branch, and `HEAD` is not one. Refuses a branch with no
/// commit, which stage worktrees cannot branch from.
pub(super) fn checked_out_branch(repo_root: &Path) -> Result<String> {
    let branch = current_branch(repo_root).context("Failed to get current git branch")?;
    if branch == "HEAD" {
        bail!("loom init needs a checked-out branch to merge stages into; HEAD is detached");
    }
    if !has_commits(&WorktreeGit::discovered(repo_root))? {
        bail!("loom init needs at least one commit on {branch}");
    }
    Ok(branch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::runner::run_git_checked;

    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        run_git_checked(&["init", "-q", "-b", "main"], root).unwrap();
        run_git_checked(
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "init",
            ],
            root,
        )
        .unwrap();
        tmp
    }

    #[test]
    fn records_the_checked_out_branch() {
        let tmp = repo();
        assert_eq!(checked_out_branch(tmp.path()).unwrap(), "main");
    }

    #[test]
    fn refuses_a_branch_with_no_commit() {
        let tmp = tempfile::TempDir::new().unwrap();
        run_git_checked(&["init", "-q", "-b", "main"], tmp.path()).unwrap();
        let error = checked_out_branch(tmp.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("loom init needs at least one commit on main"),
            "{error}"
        );
    }

    #[test]
    fn surfaces_corrupt_head_objects() {
        let tmp = repo();
        let root = tmp.path();
        let head = run_git_checked(&["rev-parse", "HEAD"], root).unwrap();
        let head = head.trim();
        let object = root.join(".git/objects").join(&head[..2]).join(&head[2..]);
        std::fs::remove_file(&object).unwrap();
        std::fs::write(object, "corrupt\n").unwrap();

        let error = checked_out_branch(root).unwrap_err();
        assert!(
            error.to_string().contains("git rev-parse HEAD failed"),
            "{error}"
        );
    }

    #[test]
    fn refuses_a_detached_head() {
        let tmp = repo();
        run_git_checked(&["checkout", "-q", "--detach"], tmp.path()).unwrap();
        let error = checked_out_branch(tmp.path()).unwrap_err();
        assert!(error.to_string().contains("HEAD is detached"));
    }
}
