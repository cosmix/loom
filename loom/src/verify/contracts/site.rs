//! Where a stage's files live on disk.
//!
//! The freeze handler (`daemon/server/contracts.rs`) and contract dispute
//! application (`orchestrator/adjudication/apply_contract.rs`) both need the
//! stage's worktree root and, beneath it, the working directory its
//! contract paths are relative to, resolved the same way.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::fs::work_dir::WorkDir;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

/// `stage`'s worktree root and, beneath it, its working directory: resolve
/// `work_dir`'s repository root, validate the stage's worktree id,
/// canonicalize the worktree path, then join and canonicalize
/// `stage.working_dir`, confirming it stays inside the worktree.
pub fn stage_site(work_dir: &Path, stage: &Stage) -> Result<(PathBuf, PathBuf)> {
    let workspace = WorkDir::new(work_dir)?;
    let repo_root = workspace
        .repo_root()
        .context("cannot resolve the repository root of the state directory")?;
    let worktree_id = stage.worktree.as_deref().unwrap_or(&stage.id);
    crate::validation::validate_id(worktree_id).context("invalid worktree id")?;
    let worktree_root = Worktree::worktree_path(repo_root, worktree_id)
        .canonicalize()
        .with_context(|| format!("stage '{}' has no worktree", stage.id))?;
    let working_dir = worktree_root
        .join(stage.working_dir.as_deref().unwrap_or("."))
        .canonicalize()
        .context("the stage's working directory does not exist")?;
    if !working_dir.starts_with(&worktree_root) {
        bail!("the stage's working directory is outside its worktree");
    }
    Ok((worktree_root, working_dir))
}
