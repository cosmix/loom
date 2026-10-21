use super::complete_force::{unmerged_tip, ForcedLanding};
use super::*;

pub(super) fn dispatch_route(
    stage: &mut Stage,
    force_unsafe: bool,
    assume_merged: bool,
    work_dir: &Path,
) -> Result<bool> {
    let (route, repo_root) =
        resolve_completion_route(stage, force_unsafe, assume_merged, work_dir)?;

    if matches!(route, CompleteConflictRoute::Proceed) {
        return Ok(false);
    }
    apply_route(route, stage, &repo_root, work_dir)?;
    Ok(true)
}

fn apply_route(
    route: CompleteConflictRoute,
    stage: &mut Stage,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<()> {
    match route {
        CompleteConflictRoute::Proceed => {}
        CompleteConflictRoute::ForceUnsafeAssumeMergedVerified { derived_commit } => {
            stage.completed_commit = derived_commit.or(stage.completed_commit.take());
            handle_force_unsafe_completion(stage, ForcedLanding::AssumeMerged, work_dir)?;
        }
        CompleteConflictRoute::ForceUnsafeAllowedStaleFlag => {
            let landing = ForcedLanding::Unmerged(unmerged_tip(stage, repo_root, work_dir));
            handle_force_unsafe_completion(stage, landing, work_dir)?;
        }
        CompleteConflictRoute::DaemonManaged { stage_id } => {
            print_daemon_managed_notice(&stage_id);
        }
        CompleteConflictRoute::SpawnResolver {
            conflicting_files,
            target_branch,
        } => {
            spawn_resolver_for_route(
                stage,
                &conflicting_files,
                &target_branch,
                repo_root,
                work_dir,
            )?;
        }
        CompleteConflictRoute::RevertAndSpawnResolver {
            conflicting_files,
            target_branch,
        } => {
            revert_phantom_merge(stage, work_dir)?;
            spawn_resolver_for_route(
                stage,
                &conflicting_files,
                &target_branch,
                repo_root,
                work_dir,
            )?;
        }
        CompleteConflictRoute::Refuse { message } => bail!("{message}"),
    }
    Ok(())
}

fn revert_phantom_merge(stage: &mut Stage, work_dir: &Path) -> Result<()> {
    mark_conflict(stage);
    update_stage(&stage.id, work_dir, |current| {
        mark_conflict(current);
        Ok(())
    })?;
    Ok(())
}

fn mark_conflict(stage: &mut Stage) {
    stage.force_status_with_reason(
        StageStatus::MergeConflict,
        "phantom-merge revert: active merge detected for stage in non-conflict status",
    );
    stage.merged = false;
    stage.merge_conflict = true;
}
