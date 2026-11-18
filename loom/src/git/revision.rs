//! Repository root and HEAD commit queries.

use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context};

use crate::git::runner::run_git;
use crate::git::worktree::WorktreeGit;

/// The top of the work tree that contains `dir`.
///
/// Fails, naming `dir`, when `dir` is not inside a git work tree; there is no
/// fallback to `dir` itself. Git's output is kept as printed but for its one
/// trailing newline, and must be UTF-8 and non-empty.
pub(crate) fn toplevel(dir: &Path) -> anyhow::Result<PathBuf> {
    let output = run_git(&["rev-parse", "--show-toplevel"], dir)?;
    ensure!(
        output.status.success(),
        "{} is not inside a git work tree: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let printed = String::from_utf8(output.stdout)
        .with_context(|| format!("git printed a non-UTF-8 top level for {}", dir.display()))?;
    let root = printed.strip_suffix('\n').unwrap_or(&printed);
    ensure!(
        !root.is_empty(),
        "git printed an empty top level for {}",
        dir.display()
    );
    Ok(PathBuf::from(root))
}

/// HEAD's commit id, or `None` on an unborn branch.
///
/// Every other failure is an error naming git's stderr, as is an output that
/// is not a 40- or 64-character hexadecimal object id.
pub(crate) fn head_commit(git: &WorktreeGit) -> anyhow::Result<Option<String>> {
    let output = git.run(&["rev-parse", "-q", "--verify", "HEAD^{commit}"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        let id = stdout.strip_suffix('\n').unwrap_or(&stdout);
        ensure!(
            is_object_id(id),
            "git rev-parse HEAD printed no commit id: {id:?}"
        );
        return Ok(Some(id.to_owned()));
    }
    // `-q --verify` exits 1 printing nothing when HEAD names no commit yet, and also
    // when a branch ref names a missing or non-commit object.
    if output.status.code() == Some(1) && stdout.is_empty() {
        return unborn_or_unreadable(git);
    }
    bail!(
        "git rev-parse HEAD failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// Tells an unborn branch from a HEAD whose object is missing or not a commit:
/// an unpeeled `--verify HEAD` still resolves to an id in the second case.
fn unborn_or_unreadable(git: &WorktreeGit) -> anyhow::Result<Option<String>> {
    let output = git.run(&["rev-parse", "-q", "--verify", "HEAD"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        let id = stdout.trim();
        bail!("HEAD names {id}, which is not a readable commit object");
    }
    if output.status.code() == Some(1) && stdout.is_empty() {
        return Ok(None);
    }
    bail!(
        "git rev-parse HEAD failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// [`head_commit`] that requires a commit.
pub(crate) fn require_head_commit(git: &WorktreeGit) -> anyhow::Result<String> {
    head_commit(git)?.context("HEAD has no commit")
}

/// Whether HEAD names a commit.
pub(crate) fn has_commits(git: &WorktreeGit) -> anyhow::Result<bool> {
    Ok(head_commit(git)?.is_some())
}

fn is_object_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "revision_tests.rs"]
mod tests;
