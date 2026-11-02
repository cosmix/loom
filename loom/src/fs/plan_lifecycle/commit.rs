//! Post-completion commit: keep the default branch clean after a plan is
//! marked done.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use crate::fs::{resolve_target_branch_from_config, work_dir::WorkDir};
use crate::git::branch::branch_ref;
use crate::git::merge::lock::MergeLock;
use crate::git::merge::operator_operation;
use crate::git::runner::{run_git_checked, run_git_with_env};
use crate::git::signing;
use crate::git::target_guard;

/// Commit the plan's own IN_PROGRESS → DONE rename after plan completion.
///
/// Only the two plan paths are staged. Stage changes reach the target branch
/// as merge commits, so any other tracked modification in the checkout is the
/// operator's work in progress and must stay uncommitted.
pub(super) fn commit_post_completion_changes(
    work_dir: &WorkDir,
    old_plan_path: &Path,
    new_plan_path: &Path,
) -> Result<()> {
    let repo_root = work_dir
        .project_root()
        .context("Failed to determine project root")?;

    let _lock = MergeLock::acquire(work_dir.root(), Duration::from_secs(10))?;
    let target = resolve_target_branch_from_config(work_dir.root(), repo_root)?;
    let reference = branch_ref(&target);
    require_branch(repo_root, &reference)?;
    let before = run_git_checked(&["rev-parse", "--verify", "HEAD"], repo_root)?;
    let old = old_plan_path.to_str().context("plan path is not UTF-8")?;
    let new = new_plan_path.to_str().context("plan path is not UTF-8")?;
    let tree = completion_tree(work_dir.root(), repo_root, &before, old, new)?;
    if tree == run_git_checked(&["rev-parse", &format!("{before}^{{tree}}")], repo_root)? {
        return Ok(());
    }

    // Commit with a descriptive message
    let plan_name = new_plan_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plan");
    let message = format!("chore(loom): mark plan complete — {plan_name}");
    let after = signing::commit_tree(repo_root, &tree, &[&before], &message)?;
    move_completion_ref(repo_root, &reference, &before, &after, &message)?;
    if let Err(error) = synchronize_plan_index(repo_root, old_plan_path, new_plan_path) {
        tracing::warn!(%error, "completion commit succeeded but its index entries could not be synchronized");
    }
    attest_completion_commit(
        work_dir,
        repo_root,
        old_plan_path,
        new_plan_path,
        &reference,
        &before,
        &after,
    );

    println!(
        "  {} Committed post-completion changes to default branch",
        "✓".green().bold(),
    );

    Ok(())
}

fn synchronize_plan_index(repo: &Path, old: &Path, new: &Path) -> Result<()> {
    let relative = |path: &Path| -> Result<String> {
        Ok(path
            .strip_prefix(repo)
            .unwrap_or(path)
            .to_str()
            .context("plan path is not UTF-8")?
            .to_string())
    };
    let (old, new) = (relative(old)?, relative(new)?);
    let literal = [("GIT_LITERAL_PATHSPECS", OsStr::new("1"))];
    index_git(
        repo,
        &["update-index", "--force-remove", "--", &old],
        &literal,
    )?;
    index_git(repo, &["add", "--", &new], &literal)?;
    Ok(())
}

fn completion_tree(work: &Path, repo: &Path, before: &str, old: &str, new: &str) -> Result<String> {
    let temporary = tempfile::Builder::new()
        .prefix("completion-index-")
        .tempdir_in(work)?;
    let index = temporary.path().join("index");
    let env = [
        ("GIT_INDEX_FILE", index.as_os_str()),
        ("GIT_LITERAL_PATHSPECS", OsStr::new("1")),
    ];
    index_git(repo, &["read-tree", before], &env)?;
    index_git(repo, &["add", "-A", "--", old, new], &env)?;
    index_git(repo, &["write-tree"], &env)
}

fn move_completion_ref(
    repo_root: &Path,
    reference: &str,
    before: &str,
    after: &str,
    message: &str,
) -> Result<()> {
    require_branch(repo_root, reference)?;
    run_git_checked(
        &[
            "update-ref",
            "--no-deref",
            "-m",
            message,
            reference,
            after,
            before,
        ],
        repo_root,
    )
    .map(|_| ())
}

fn require_branch(repo: &Path, reference: &str) -> Result<()> {
    if let Some(operation) = operator_operation(repo)? {
        bail!("cannot commit completion during operator operation {operation}");
    }
    let actual = run_git_checked(&["symbolic-ref", "--quiet", "HEAD"], repo)?;
    if actual != reference {
        bail!("completion target is {reference}, but checkout is on {actual}");
    }
    Ok(())
}

fn index_git(repo: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Result<String> {
    let output = run_git_with_env(args, env, repo)?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Attest the post-completion commit when it touches only the plan's own
/// files: both plan paths and the plan's review document. A failure is a
/// warning; the commit then stays unattested and the guard holds it for the
/// operator.
fn attest_completion_commit(
    work_dir: &WorkDir,
    repo_root: &Path,
    old_plan_path: &Path,
    new_plan_path: &Path,
    reference: &str,
    from: &str,
    after: &str,
) {
    let plan_id = work_dir
        .load_config()
        .ok()
        .flatten()
        .and_then(|config| config.plan_id().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string());
    let review = Path::new("doc/plans").join(format!("REVIEW-{plan_id}.md"));
    let allowed = [old_plan_path, new_plan_path, review.as_path()];
    if let Err(error) =
        attest_exact_commit(work_dir.root(), repo_root, &allowed, reference, from, after)
    {
        tracing::warn!("could not attest the post-completion commit: {error:#}");
    }
}

/// Attest loom's own commit at `HEAD` of the main checkout in the target
/// guard's ledger, because loom's git runs without hooks and the
/// `reference-transaction` hook never sees it.
///
/// Attested only when `HEAD`'s branch is a guarded target and every path the
/// commit changes is one of `allowed` or under the knowledge prefix. Any other
/// path, `doc/plans/` siblings included, leaves the commit unattested: the
/// commit may carry a session's edit, and the guard then holds it for the
/// operator. `allowed` paths are relative to `repo_root` or under it.
pub(crate) fn attest_plan_commit(
    work_dir: &Path,
    repo_root: &Path,
    allowed: &[&Path],
) -> Result<()> {
    crate::git::worktree::require_main_identity(repo_root)?;
    let git = crate::git::worktree::WorktreeGit::pinned_main(repo_root)?;
    // `HEAD` is read after the commit: a read before it can be stale.
    let after = git.run_checked(&["rev-parse", "HEAD"])?.trim().to_string();
    let from = git
        .run_checked(&["rev-parse", &format!("{after}^")])?
        .trim()
        .to_string();
    let reference = branch_ref(&crate::git::branch::current_branch_with(&git)?);
    attest_exact_commit_with(
        &git, work_dir, repo_root, allowed, &reference, &from, &after,
    )
}

fn attest_exact_commit(
    work_dir: &Path,
    repo_root: &Path,
    allowed: &[&Path],
    reference: &str,
    from: &str,
    after: &str,
) -> Result<()> {
    let git = crate::git::worktree::WorktreeGit::discovered(repo_root);
    attest_exact_commit_with(&git, work_dir, repo_root, allowed, reference, from, after)
}

fn attest_exact_commit_with(
    git: &crate::git::worktree::WorktreeGit,
    work_dir: &Path,
    repo_root: &Path,
    allowed: &[&Path],
    reference: &str,
    from: &str,
    after: &str,
) -> Result<()> {
    if !target_guard::guarded_refs(work_dir)?
        .iter()
        .any(|item| item == reference)
    {
        return Ok(());
    }
    let paths = crate::git::merge::control_paths::changed_paths_with(git, from, after)?;
    if !paths.iter().all(|path| is_own(path, repo_root, allowed)) {
        tracing::info!("not attesting {after}: it changes more than the plan's own files");
        return Ok(());
    }
    target_guard::append_attestation(work_dir, reference, from, after)
}

/// Whether `path` (as git reports it) is one of `allowed` or under the
/// knowledge prefix.
fn is_own(path: &str, repo_root: &Path, allowed: &[&Path]) -> bool {
    path.starts_with(target_guard::knowledge_prefix())
        || allowed
            .iter()
            .copied()
            .any(|own| own.strip_prefix(repo_root).unwrap_or(own) == Path::new(path))
}

#[cfg(test)]
#[path = "commit/tests.rs"]
mod tests;
