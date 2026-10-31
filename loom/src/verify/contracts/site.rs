//! Where a stage's files live on disk.
//!
//! The freeze handler (`daemon/server/contracts.rs`) and contract dispute
//! application (`orchestrator/adjudication/apply_contract.rs`) both need the
//! stage's worktree root and, beneath it, the working directory its
//! contract paths are relative to, resolved the same way.

use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

use crate::fs::work_dir::WorkDir;
use crate::git::worktree::stage_worktree_path;
use crate::models::stage::Stage;

/// `stage`'s worktree root and, beneath it, its working directory: resolve
/// `work_dir`'s repository root, validate the stage's worktree id,
/// canonicalize the worktree path, then join and canonicalize
/// `stage.working_dir`, confirming it stays inside the worktree.
pub fn stage_site(work_dir: &Path, stage: &Stage) -> Result<(PathBuf, PathBuf)> {
    let workspace = WorkDir::new(work_dir)?;
    let repo_root = workspace
        .repo_root()
        .context("cannot resolve the repository root of the state directory")?;
    let worktree_root = stage_worktree_path(stage, repo_root)?
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

/// A stage and the locations its contract files are relative to.
pub(crate) struct ContractSite {
    pub(crate) work_dir: PathBuf,
    pub(crate) stage: Stage,
    pub(crate) worktree_root: PathBuf,
    pub(crate) working_dir: PathBuf,
}

impl ContractSite {
    pub(crate) fn resolved(
        work_dir: PathBuf,
        stage: Stage,
        worktree_root: PathBuf,
        working_dir: PathBuf,
    ) -> Self {
        Self {
            work_dir,
            stage,
            worktree_root,
            working_dir,
        }
    }

    /// `working_dir` relative to the worktree root; empty when they coincide.
    pub(crate) fn working_dir_prefix(&self) -> PathBuf {
        let declared = self.stage.working_dir.as_deref().unwrap_or(".");
        Path::new(declared)
            .components()
            .filter(|component| matches!(component, Component::Normal(_)))
            .collect()
    }
}
