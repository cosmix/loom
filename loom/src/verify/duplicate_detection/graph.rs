//! Shared repository coordinates, changed inputs, and coverage reporting.
use crate::context::source_graph::FileCoverage;
use crate::context::worktree_graph::{build_for_worktree, WorktreeGraph};
use crate::git::changes::{changed_names, merge_base, untracked_names, DiffNames};
use crate::git::worktree::is_worktree_scaffold_path;
use crate::verify::review::fingerprint::checkout_git;
use crate::verify::tool_artifacts::is_tool_artifact;
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

/// The repository-relative paths changed since the merge base of `base` and
/// `HEAD`: committed, staged and unstaged changes (only additions when
/// `added_only`, otherwise additions, copies, modifications, renames and type
/// changes), plus untracked files git does not ignore. Rename detection is
/// off, so a moved file is listed as added at its new path. Untracked sandbox
/// artifacts and worktree scaffolding are left out.
pub(crate) fn changed_files(root: &Path, base: &str, added_only: bool) -> Result<BTreeSet<String>> {
    let git = checkout_git(root)?;
    let fork = merge_base(&git, base, "HEAD")?;
    let query = DiffNames {
        from: &fork,
        to: None,
        filter: Some(if added_only { "A" } else { "ACMRT" }),
        all_submodules: false,
        pathspecs: &[],
    };
    let mut paths = changed_names(&git, &query)?;
    paths.extend(
        untracked_names(&git, &[])?
            .into_iter()
            .filter(|path| !is_tool_artifact(git.work_tree(), path)),
    );
    paths.retain(|path| !is_worktree_scaffold_path(path));
    Ok(paths)
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
