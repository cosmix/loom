//! Shared repository coordinates, changed inputs, and coverage reporting.
use crate::context::source_graph::FileCoverage;
use crate::context::worktree_graph::{build_for_worktree, WorktreeGraph};
use crate::git::runner::{run_git_checked, run_git_records};
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(crate) fn repository_root(directory: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(run_git_checked(
        &["rev-parse", "--show-toplevel"],
        directory,
    )?))
}

/// Diff against the merge base includes committed, staged and unstaged changes;
/// untracked files join the same repository-relative set.
pub(crate) fn changed_files(root: &Path, base: &str, added_only: bool) -> Result<BTreeSet<String>> {
    let merge_base = run_git_checked(&["merge-base", base, "HEAD"], root)?;
    let filter = if added_only {
        "--diff-filter=A"
    } else {
        "--diff-filter=ACMRT"
    };
    let diff = run_git_records(
        &["diff", "--name-only", "-z", filter, &merge_base, "--"],
        root,
    )?;
    let untracked = run_git_records(&["ls-files", "--others", "--exclude-standard", "-z"], root)?;
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
