use crate::fs::work_dir::WorkDir;
use crate::git::worktree::{stage_worktree_path, WorktreeGit};
use crate::models::stage::Stage;
use crate::verify::{integrity, review::fingerprint};
use anyhow::{Context, Result};
use std::path::Path;
pub(crate) fn integrity_events(
    work_dir: &Path,
    stage: &Stage,
) -> Result<Vec<integrity::IntegrityEvent>> {
    let (repo, target_branch) = stage_worktree_and_target(work_dir, stage)?;
    let changes = fingerprint::compute_local(&repo, &target_branch)
        .context("failed to list the worktree's changes for the test-integrity check")?;
    Ok(integrity::scan_changes(&repo, &changes, &stage.ratchet_files)?.events)
}

/// Git in the stage's worktree and the target branch its changes are
/// measured against, from this state directory alone: the worktree under its
/// repository root by the stage's validated worktree id, canonicalized, git
/// pinned to its registered git directory in that repository, and the target
/// from its configuration.
pub(crate) fn stage_worktree_and_target(
    work_dir: &Path,
    stage: &Stage,
) -> Result<(WorktreeGit, String)> {
    let workspace = WorkDir::new(work_dir)?;
    let repo_root = workspace
        .repo_root()
        .context("cannot resolve the repository root of the state directory")?;
    let worktree = stage_worktree_path(stage, repo_root)?
        .canonicalize()
        .with_context(|| format!("stage '{}' has no worktree", stage.id))?;
    let repo = WorktreeGit::pinned(repo_root, &worktree)?;
    let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
    Ok((repo, target_branch))
}
