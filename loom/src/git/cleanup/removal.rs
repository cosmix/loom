//! Safety policy for manual stage-resource removal.

use super::base::base_branch_exists;
use super::batch::cleanup_after_merge;
use super::branch::branch_exists_strict;
use super::config::{CleanupConfig, CleanupResult};
use super::worktree::worktree_directory_exists;
use crate::git::branch::{branch_name_for_stage, is_ancestor_of};
use crate::git::runner::run_git_checked;
use crate::git::worktree::is_worktree_scaffold_path;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

struct StageResources {
    worktree_path: PathBuf,
    worktree_exists: bool,
    branch_name: String,
    branch_exists: bool,
    base_branch_name: String,
    base_branch_present: bool,
}

/// Exact confirmation required for destructive removal of a stage.
pub fn destructive_removal_confirmation(stage_id: &str) -> String {
    format!("delete-unmerged-work:{stage_id}")
}

/// Remove stage resources only after proving all retained commits reached the target.
pub fn cleanup_verified_stage(
    stage_id: &str,
    completed_commit: &str,
    target_branch: &str,
    repo_root: &Path,
) -> Result<CleanupResult> {
    let resources = inspect_resources(stage_id, repo_root)?;
    require_resources(stage_id, &resources)?;
    require_clean_worktree(&resources)?;
    require_merged_history(completed_commit, target_branch, repo_root, &resources)?;

    let config = CleanupConfig {
        force_worktree_removal: false,
        force_branch_deletion: true,
        prune_worktrees: true,
        verbose: false,
    };
    cleanup_after_merge(stage_id, repo_root, &config)
}

/// Destructively remove stage resources after an exact, stage-specific confirmation.
///
/// This operation only removes Git resources. It deliberately makes no claim
/// that the stage was merged and must not be followed by a merge-state update.
pub fn cleanup_destructive_stage(
    stage_id: &str,
    confirmation: &str,
    repo_root: &Path,
) -> Result<CleanupResult> {
    let expected = destructive_removal_confirmation(stage_id);
    if confirmation != expected {
        bail!(
            "Destructive removal requires exact confirmation '{expected}'; stage state was unchanged"
        );
    }
    let resources = inspect_resources(stage_id, repo_root)?;
    require_resources(stage_id, &resources)?;
    let config = CleanupConfig {
        force_worktree_removal: true,
        force_branch_deletion: true,
        prune_worktrees: true,
        verbose: false,
    };
    cleanup_after_merge(stage_id, repo_root, &config)
}

fn inspect_resources(stage_id: &str, repo_root: &Path) -> Result<StageResources> {
    crate::validation::validate_id(stage_id)
        .with_context(|| format!("Invalid stage ID '{stage_id}' for worktree removal"))?;
    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    let worktree_exists = worktree_directory_exists(&worktree_path)?;
    let branch_name = branch_name_for_stage(stage_id);
    let branch_exists = branch_exists_strict(&branch_name, repo_root)?;
    let base_branch_name = format!("loom/_base/{stage_id}");
    let base_branch_present = base_branch_exists(stage_id, repo_root)?;
    Ok(StageResources {
        worktree_path,
        worktree_exists,
        branch_name,
        branch_exists,
        base_branch_name,
        base_branch_present,
    })
}

fn require_resources(stage_id: &str, resources: &StageResources) -> Result<()> {
    if !resources.worktree_exists && !resources.branch_exists {
        bail!(
            "No worktree or branch exists for stage '{stage_id}'; refusing to infer that it was merged"
        );
    }
    Ok(())
}

fn require_clean_worktree(resources: &StageResources) -> Result<()> {
    if !resources.worktree_exists {
        return Ok(());
    }
    let changes = run_git_checked(
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
        ],
        &resources.worktree_path,
    )
    .context("Failed to verify worktree cleanliness")?;
    let unsafe_changes: Vec<&str> = changes
        .lines()
        .filter(|line| !is_expected_scaffold(line))
        .collect();
    if !unsafe_changes.is_empty() {
        bail!(
            "Worktree {} has uncommitted changes; refusing normal removal:\n{}",
            resources.worktree_path.display(),
            unsafe_changes.join("\n")
        );
    }
    Ok(())
}

fn is_expected_scaffold(status_line: &str) -> bool {
    if !status_line.starts_with("!! ") || status_line.len() <= 3 {
        return false;
    }
    is_worktree_scaffold_path(&status_line[3..])
}

fn require_merged_history(
    completed_commit: &str,
    target_branch: &str,
    repo_root: &Path,
    resources: &StageResources,
) -> Result<()> {
    if completed_commit.trim().is_empty() {
        bail!("Stage has no retained completed commit; refusing normal removal");
    }
    require_ancestor(
        completed_commit,
        target_branch,
        repo_root,
        "completed commit",
    )?;
    if resources.branch_exists {
        require_ancestor(
            &resources.branch_name,
            target_branch,
            repo_root,
            "stage branch head",
        )?;
    } else if resources.worktree_exists {
        require_ancestor(
            "HEAD",
            target_branch,
            &resources.worktree_path,
            "worktree HEAD",
        )?;
    }
    if resources.base_branch_present {
        require_ancestor(
            &resources.base_branch_name,
            target_branch,
            repo_root,
            "stage base branch head",
        )?;
    }
    Ok(())
}

fn require_ancestor(commit: &str, target: &str, repo_root: &Path, label: &str) -> Result<()> {
    if !is_ancestor_of(commit, target, repo_root)
        .with_context(|| format!("Failed to verify {label} against '{target}'"))?
    {
        bail!("The {label} '{commit}' is not retained by target branch '{target}'");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
