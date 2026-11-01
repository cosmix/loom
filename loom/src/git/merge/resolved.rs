//! The check run on a stage worktree after its merge resolver reports done:
//! the resolver merged the target into the stage branch there and committed,
//! so `merge_stage` can now land the stage without a conflict.
//!
//! Every git command runs through [`WorktreeGit::pinned`]: the agent
//! controls the worktree's `.git` file, and the daemon must not run git as
//! that file directs.

use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::worktree::WorktreeGit;

/// Verify that the resolver left the worktree of `stage_id` ready to merge
/// into its target branch (the target is not inspected: `merge_stage` merges
/// whatever it holds, so none is passed). `Err` is the reason, worded for the
/// resolver and the operator.
///
/// The worktree must be registered, sit on `loom/<stage_id>`, have no merge
/// in progress, no unmerged paths and no tracked change, and its HEAD must
/// still contain `completed_commit`, the stage's own work: a rebase, squash or
/// reset would drop it and the ancestry proof after the merge would follow the
/// rewritten history. Untracked files are allowed: a sandbox leaves stub files
/// behind. A target that moved since the resolver merged it is not refused:
/// `merge_stage` merges the new commits or reports a conflict.
///
/// A worktree whose `config.worktree` holds a key git does not write itself is
/// refused by [`WorktreeGit`] before any command runs.
pub fn check_resolved_worktree(
    repo_root: &Path,
    stage_id: &str,
    completed_commit: Option<&str>,
) -> Result<(), String> {
    let commit = completed_commit.ok_or_else(|| {
        format!(
            "no completed commit is recorded for stage '{stage_id}'; loom cannot prove its \
             work survived the resolution"
        )
    })?;
    let path = repo_root.join(".worktrees").join(stage_id);
    let git = WorktreeGit::pinned(repo_root, &path).map_err(|error| {
        format!(
            "the stage worktree {} cannot be checked: {error:#}",
            path.display()
        )
    })?;
    check_branch_and_index(&git, stage_id)?;
    check_work_survived(&git, commit)
}

/// Whether the stage branch holds a merge commit made after `commit`, the
/// stage's recorded completed commit. This is the proof that a resolver
/// merged something; a clean tree on the right branch proves nothing of the
/// kind. The target's tip is not tested, since the target may have moved since
/// the resolver merged. A failed git command is `Err`.
pub fn has_merge_commit_since(
    repo_root: &Path,
    stage_id: &str,
    commit: &str,
) -> Result<bool, String> {
    let path = repo_root.join(".worktrees").join(stage_id);
    let git = WorktreeGit::pinned(repo_root, &path).map_err(|error| {
        format!(
            "the stage worktree {} cannot be checked: {error:#}",
            path.display()
        )
    })?;
    let range = format!("{commit}..HEAD");
    let merges = query(&git, &["rev-list", "--merges", "--max-count=1", &range])?;
    Ok(!merges.is_empty())
}

/// HEAD on `loom/<stage_id>`, no merge in progress, no unmerged path, no
/// tracked change.
fn check_branch_and_index(git: &WorktreeGit, stage_id: &str) -> Result<(), String> {
    let branch = branch_name_for_stage(stage_id);
    let head_ref = query_optional(git, &["symbolic-ref", "-q", "HEAD"])?.unwrap_or_default();
    if head_ref != format!("refs/heads/{branch}") {
        return Err(format!(
            "the worktree is not on {branch} (HEAD is {}); switch back to it",
            if head_ref.is_empty() {
                "detached"
            } else {
                &head_ref
            }
        ));
    }
    if query_optional(git, &["rev-parse", "-q", "--verify", "MERGE_HEAD"])?.is_some() {
        return Err(
            "a merge is still in progress (MERGE_HEAD exists); commit it first".to_string(),
        );
    }
    let unmerged = query(git, &["ls-files", "-u"])?;
    if !unmerged.is_empty() {
        return Err(format!(
            "unmerged paths remain: {}",
            unmerged_paths(&unmerged)
        ));
    }
    // `--no-optional-locks`: the resolver may be working in this worktree, and
    // the daemon must not take its index lock for a refresh.
    let changes = query(
        git,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "--untracked-files=no",
        ],
    )?;
    if !changes.is_empty() {
        return Err(format!(
            "tracked files have uncommitted changes; commit them: {}",
            changes.lines().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(())
}

/// `commit`, the stage's own work, must be an ancestor of the worktree's HEAD.
fn check_work_survived(git: &WorktreeGit, commit: &str) -> Result<(), String> {
    if commit.is_empty() || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "the recorded completed commit '{commit}' is not a commit id"
        ));
    }
    let output = git
        .run(&["merge-base", "--is-ancestor", commit, "HEAD"])
        .map_err(|error| format!("git merge-base failed in the stage worktree: {error:#}"))?;
    match output.status.code() {
        Some(0) => Ok(()),
        Some(1) => Err(format!(
            "the stage's own commit {commit} is no longer in the branch history; do not \
             rebase, reset, squash or amend: restore the branch to include it, merge the \
             target in, resolve, and commit"
        )),
        _ => Err(format!(
            "git merge-base failed in the stage worktree: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

/// Run a mandatory inspection, rejecting any unsuccessful exit.
fn query(git: &WorktreeGit, args: &[&str]) -> Result<String, String> {
    run_query(git, args, false).map(|value| value.expect("successful query has output"))
}

/// Git uses exit 1 for an absent symbolic ref or verified ref with `-q`.
fn query_optional(git: &WorktreeGit, args: &[&str]) -> Result<Option<String>, String> {
    run_query(git, args, true)
}

fn run_query(
    git: &WorktreeGit,
    args: &[&str],
    allow_absent: bool,
) -> Result<Option<String>, String> {
    let output = git.run(args).map_err(|error| {
        format!(
            "git {} failed in the stage worktree: {error:#}",
            args.join(" ")
        )
    })?;
    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )),
        Some(1) if allow_absent && output.stderr.is_empty() => Ok(None),
        _ => Err(format!(
            "git {} failed in the stage worktree ({}): {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

/// The paths of `git ls-files -u` output, once each.
fn unmerged_paths(listing: &str) -> String {
    let mut paths: Vec<&str> = Vec::new();
    for path in listing.lines().filter_map(|line| line.split('\t').nth(1)) {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::merge::test_support::{commit_file, git_ok, git_out, init_repo};
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// A repository with a linked worktree for stage `s` on `loom/s`, which
    /// holds one commit of its own so merging `main` makes a merge commit.
    /// The third value is that commit, the stage's `completed_commit`.
    fn repo_with_worktree() -> (TempDir, PathBuf, String) {
        let repo = init_repo();
        let root = repo.path();
        let wt = root.join(".worktrees").join("s");
        git_ok(
            root,
            &["worktree", "add", "-b", "loom/s", wt.to_str().unwrap()],
        );
        commit_file(&wt, "stage.txt", "stage", "stage work");
        let commit = git_out(&wt, &["rev-parse", "HEAD"]);
        (repo, wt, commit)
    }

    fn check(repo: &TempDir, commit: &str) -> Result<(), String> {
        check_resolved_worktree(repo.path(), "s", Some(commit))
    }

    fn refusal(repo: &TempDir, commit: &str) -> String {
        check(repo, commit).expect_err("the check must refuse")
    }

    #[test]
    fn a_merged_clean_worktree_passes_and_untracked_files_are_allowed() {
        let (repo, wt, commit) = repo_with_worktree();
        commit_file(repo.path(), "m.txt", "main", "main work");
        git_ok(&wt, &["merge", "main", "-m", "merge main"]);
        assert_eq!(check(&repo, &commit), Ok(()));
        std::fs::write(wt.join("stub.txt"), "sandbox stub").unwrap();
        assert_eq!(check(&repo, &commit), Ok(()));
    }

    #[test]
    fn a_merge_commit_since_the_completed_commit_is_found_and_a_plain_commit_is_not() {
        let (repo, wt, commit) = repo_with_worktree();
        commit_file(&wt, "more.txt", "more", "plain work");
        assert_eq!(has_merge_commit_since(repo.path(), "s", &commit), Ok(false));
        commit_file(repo.path(), "m.txt", "main", "main work");
        git_ok(&wt, &["merge", "main", "-m", "merge main"]);
        assert_eq!(has_merge_commit_since(repo.path(), "s", &commit), Ok(true));
        assert!(has_merge_commit_since(repo.path(), "s", "not-a-commit").is_err());
    }

    #[test]
    fn a_target_that_moved_after_the_resolution_is_not_refused() {
        let (repo, wt, commit) = repo_with_worktree();
        commit_file(repo.path(), "m.txt", "main", "main work");
        git_ok(&wt, &["merge", "main", "-m", "merge main"]);
        commit_file(repo.path(), "later.txt", "later", "unrelated work");
        assert_eq!(check(&repo, &commit), Ok(()));
    }

    #[test]
    fn a_rebase_that_drops_the_completed_commit_is_refused() {
        let (repo, wt, commit) = repo_with_worktree();
        commit_file(repo.path(), "m.txt", "main", "main work");
        git_ok(&wt, &["rebase", "main"]);
        assert!(refusal(&repo, &commit).contains("no longer in the branch history"));
    }

    #[test]
    fn an_amend_that_rewrites_the_completed_commit_is_refused() {
        let (repo, wt, commit) = repo_with_worktree();
        git_ok(&wt, &["commit", "--amend", "-m", "squashed"]);
        assert!(refusal(&repo, &commit).contains("no longer in the branch history"));
    }

    #[test]
    fn a_reset_to_the_target_is_refused() {
        let (repo, wt, commit) = repo_with_worktree();
        git_ok(&wt, &["reset", "--hard", "main"]);
        assert!(refusal(&repo, &commit).contains("no longer in the branch history"));
    }

    #[test]
    fn a_missing_completed_commit_is_refused() {
        let (repo, _wt, _commit) = repo_with_worktree();
        let reason = check_resolved_worktree(repo.path(), "s", None).unwrap_err();
        assert_eq!(
            reason,
            "no completed commit is recorded for stage 's'; loom cannot prove its work \
             survived the resolution"
        );
    }

    #[test]
    fn a_completed_commit_that_is_not_a_commit_id_is_refused() {
        let (repo, _wt, _commit) = repo_with_worktree();
        assert!(refusal(&repo, "--all").contains("is not a commit id"));
    }

    #[test]
    fn sparse_keys_in_config_worktree_pass_and_other_keys_are_refused() {
        let (repo, wt, commit) = repo_with_worktree();
        git_ok(
            repo.path(),
            &["config", "extensions.worktreeConfig", "true"],
        );
        let admin = git_out(&wt, &["rev-parse", "--absolute-git-dir"]);
        let file = Path::new(&admin).join("config.worktree");
        std::fs::write(&file, "[core]\n\tsparseCheckout = true\n").unwrap();
        assert_eq!(check(&repo, &commit), Ok(()));
        std::fs::write(&file, "[filter \"x\"]\n\tclean = true\n").unwrap();
        assert!(refusal(&repo, &commit).contains("filter.x.clean"));
    }

    #[test]
    fn an_unregistered_worktree_is_refused() {
        let repo = init_repo();
        std::fs::create_dir_all(repo.path().join(".worktrees").join("s")).unwrap();
        assert!(refusal(&repo, "abc123").contains("cannot be checked"));
    }

    #[test]
    fn a_worktree_on_another_branch_is_refused() {
        let (repo, wt, commit) = repo_with_worktree();
        git_ok(&wt, &["checkout", "-b", "other"]);
        assert!(refusal(&repo, &commit).contains("not on loom/s"));
    }

    #[test]
    fn a_merge_in_progress_is_refused_with_its_unmerged_paths() {
        let (repo, wt, commit) = repo_with_worktree();
        commit_file(&wt, "a.txt", "stage side", "stage edit");
        commit_file(repo.path(), "a.txt", "main side", "main edit");
        let merge = crate::git::runner::run_git(&["merge", "main"], &wt).unwrap();
        assert!(!merge.status.success(), "the merge must conflict");
        assert!(refusal(&repo, &commit).contains("MERGE_HEAD"));
        let git_dir = git_out(&wt, &["rev-parse", "--absolute-git-dir"]);
        std::fs::remove_file(Path::new(&git_dir).join("MERGE_HEAD")).unwrap();
        assert!(refusal(&repo, &commit).contains("unmerged paths remain: a.txt"));
    }

    #[test]
    fn an_uncommitted_tracked_change_is_refused() {
        let (repo, wt, commit) = repo_with_worktree();
        std::fs::write(wt.join("a.txt"), "edited").unwrap();
        assert!(refusal(&repo, &commit).contains("uncommitted changes"));
    }

    #[test]
    fn a_corrupt_index_is_refused_with_git_error() {
        let (repo, wt, commit) = repo_with_worktree();
        let admin = git_out(&wt, &["rev-parse", "--absolute-git-dir"]);
        std::fs::write(Path::new(&admin).join("index"), b"corrupt index").unwrap();
        let reason = refusal(&repo, &commit);
        assert!(reason.contains("git ls-files -u failed"), "{reason}");
        assert!(reason.contains("index"), "{reason}");
    }
}
