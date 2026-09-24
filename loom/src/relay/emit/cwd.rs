//! Confines a relay request's working directory to the requesting session's
//! own checkout: the worktree for a `Stage` or `Contract` session, the main
//! project root for every other kind
//! (`doc/plans/PLAN-loom-state-confinement.md` section 6, guard 3).

use super::RelayContext;
use crate::fs::work_dir::WorkDir;
use crate::models::session::SessionType;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Canonicalize `cwd` and require it sits inside this session's checkout.
pub(super) fn require_within_checkout(
    context: &RelayContext,
    session_type: SessionType,
    cwd: &Path,
) -> Result<()> {
    let canonical_cwd = cwd
        .canonicalize()
        .with_context(|| format!("failed to canonicalize cwd {}", cwd.display()))?;
    let boundary = checkout_boundary(context, session_type)?;
    if !canonical_cwd.starts_with(&boundary) {
        bail!(
            "cwd {} is outside this session's checkout ({}); refusing to relay",
            canonical_cwd.display(),
            boundary.display()
        );
    }
    Ok(())
}

/// The canonical directory `cwd` must sit inside: the worktree for a `Stage`
/// or `Contract` session, the main project root (derived from `LOOM_WORK_DIR`
/// via [`WorkDir::main_project_root`]) otherwise.
fn checkout_boundary(context: &RelayContext, session_type: SessionType) -> Result<PathBuf> {
    if matches!(session_type, SessionType::Stage | SessionType::Contract) {
        let worktree_path = context
            .worktree_path
            .as_deref()
            .with_context(|| format!("LOOM_WORKTREE_PATH is unset for a {session_type} session"))?;
        return worktree_path.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize worktree path {}",
                worktree_path.display()
            )
        });
    }
    let work_dir = context
        .work_dir
        .as_deref()
        .context("LOOM_WORK_DIR is unset")?;
    let project_root = WorkDir::new(work_dir)?
        .main_project_root()
        .with_context(|| {
            format!(
                "could not resolve the main project root from {}",
                work_dir.display()
            )
        })?;
    project_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize main project root {}",
            project_root.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn context_with(worktree_path: Option<PathBuf>, work_dir: Option<PathBuf>) -> RelayContext {
        RelayContext {
            session_id: "session-1".to_string(),
            scratch_dir: PathBuf::from("/unused"),
            stage_id: None,
            session_type: None,
            worktree_path,
            work_dir,
        }
    }

    #[test]
    fn stage_cwd_inside_the_worktree_is_accepted() {
        let worktree = TempDir::new().unwrap();
        let context = context_with(Some(worktree.path().to_path_buf()), None);
        require_within_checkout(&context, SessionType::Stage, worktree.path()).unwrap();
    }

    #[test]
    fn stage_cwd_outside_the_worktree_is_refused() {
        let worktree = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let context = context_with(Some(worktree.path().to_path_buf()), None);
        assert!(require_within_checkout(&context, SessionType::Stage, outside.path()).is_err());
    }

    /// A contract session runs in the stage worktree, so the rest of the
    /// project is outside its checkout even though it contains the worktree.
    #[test]
    fn contract_cwd_is_confined_to_the_worktree_not_the_project_root() {
        let project = TempDir::new().unwrap();
        let worktree = project.path().join(".worktrees").join("alpha");
        std::fs::create_dir_all(&worktree).unwrap();
        let work_dir = project.path().join(".loom").join("work");
        let context = context_with(Some(worktree.clone()), Some(work_dir));
        require_within_checkout(&context, SessionType::Contract, &worktree).unwrap();
        assert!(require_within_checkout(&context, SessionType::Contract, project.path()).is_err());
    }

    #[test]
    fn non_stage_cwd_outside_the_project_root_is_refused() {
        let project = TempDir::new().unwrap();
        let work_dir = project.path().join(".loom").join("work");
        let outside = TempDir::new().unwrap();
        let context = context_with(None, Some(work_dir));
        assert!(require_within_checkout(&context, SessionType::Knowledge, outside.path()).is_err());
    }

    #[test]
    fn non_stage_cwd_inside_the_project_root_is_accepted() {
        let project = TempDir::new().unwrap();
        let work_dir = project.path().join(".loom").join("work");
        let context = context_with(None, Some(work_dir));
        require_within_checkout(&context, SessionType::Adjudication, project.path()).unwrap();
    }
}
