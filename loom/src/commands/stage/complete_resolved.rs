//! Full completion verification for trusted resolver landing entry points.
use super::*;
use crate::handoff::completion::content::VerificationSnapshot;

/// Preserve the original ancestry proof until every current completion gate
/// passes for the resolved commit. The caller still applies the merge gate.
pub(crate) fn verify_resolved_completion(stage: &Stage, work_dir: &Path) -> Result<Stage> {
    let repo = crate::fs::work_dir::WorkDir::new(work_dir)?
        .main_project_root()
        .context("completion repository is unavailable")?;
    crate::git::merge::check_resolved_worktree(&repo, &stage.id, stage.completed_commit.as_deref())
        .map_err(anyhow::Error::msg)?;
    let root = crate::git::worktree::stage_worktree_path(stage, &repo)?;
    let paths = StageExecutionPaths {
        acceptance_dir: super::super::acceptance_runner::resolve_acceptance_dir(
            Some(&root),
            stage.working_dir.as_deref(),
        )?,
        worktree_root: Some(root),
    };
    let snapshot = VerificationSnapshot::capture(stage, work_dir)?;
    let passed = run_acceptance_phase(stage, &stage.id, false, &paths, work_dir)?;
    ensure_acceptance_passed(passed, &stage.id)?;
    complete_verification::run(&complete_verification::VerificationChecks {
        stage,
        stage_id: &stage.id,
        acceptance_dir: paths.acceptance_dir.as_deref(),
        worktree_root: paths.worktree_root.as_deref(),
        control_session: None,
        work_dir,
    })?;
    update_stage(&stage.id, work_dir, |current| {
        anyhow::ensure!(
            matches!(
                current.status,
                StageStatus::MergeConflict | StageStatus::MergeBlocked
            ),
            "stage left merge resolution during verification"
        );
        snapshot.validate_locked(current, work_dir)?;
        current.completed_commit = Some(snapshot.commit.clone());
        Ok(())
    })
}
