//! Shared repository coordinates, changed inputs, and coverage reporting.
use crate::context::source_graph::FileCoverage;
use crate::context::worktree_graph::{build_for_worktree, WorktreeGraph};
use crate::verify::review::fingerprint::checkout_git;
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The root of the checkout holding `directory`. These checks run on the host
/// child, outside the stage's sandbox, so git is pinned to the stage's
/// registered git directory (`checkout_git`) rather than the worktree's own
/// `.git` file, which a stage can repoint at a configuration that runs a
/// command (a clean filter on the re-hash `git diff` does).
pub(crate) fn repository_root(directory: &Path) -> Result<PathBuf> {
    Ok(checkout_git(directory)?.work_tree().to_path_buf())
}

/// Diff against the merge base includes committed, staged and unstaged changes;
/// untracked files join the same repository-relative set.
pub(crate) fn changed_files(root: &Path, base: &str, added_only: bool) -> Result<BTreeSet<String>> {
    let git = checkout_git(root)?;
    let merge_base = git.run_checked(&["merge-base", base, "HEAD"])?;
    let filter = if added_only {
        "--diff-filter=A"
    } else {
        "--diff-filter=ACMRT"
    };
    let diff = git.run_checked(&["diff", "--name-only", "-z", filter, merge_base.trim(), "--"])?;
    let untracked = git.run_checked(&["ls-files", "--others", "--exclude-standard", "-z"])?;
    Ok(diff
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

pub(crate) fn graph(root: &Path) -> Result<WorktreeGraph> {
    let built = build_for_worktree(root)?;
    if let Some(reason) = &built.degraded {
        eprintln!("Source analysis: {reason}");
    }
    for (path, entry) in &built.graph.files {
        if crate::context::extract::dialect::dialect_for_path(Path::new(path)).is_some()
            && !matches!(entry.coverage, FileCoverage::Full)
        {
            eprintln!(
                "Source analysis incomplete for {path}: {:?}",
                entry.coverage
            );
        }
    }
    Ok(built)
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
