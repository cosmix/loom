use super::*;

pub(super) fn dispatch_route(
    stage: &mut Stage,
    force_unsafe: bool,
    assume_merged: bool,
    sandboxed: bool,
    work_dir: &Path,
) -> Result<bool> {
    let (route, repo_root) =
        resolve_completion_route(stage, force_unsafe, assume_merged, work_dir)?;

    // A sandboxed wrapper may perform verification only. Resolver spawning,
    // phantom-merge repair, and administrative completion all mutate trusted
    // state before the verification marker and therefore belong to the host
    // orchestrator, not this route.
    if sandboxed && !matches!(&route, CompleteConflictRoute::Proceed) {
        bail!("sandboxed completion cannot perform conflict or recovery operations");
    }

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
    let stage_id = stage.id.clone();
    match route {
        CompleteConflictRoute::Proceed => {}
        CompleteConflictRoute::ForceUnsafeAssumeMergedVerified { derived_commit } => {
            if let Some(commit) = derived_commit {
                stage.completed_commit = Some(commit);
            }
            handle_force_unsafe_completion(stage.clone(), &stage_id, true, work_dir)?;
        }
        CompleteConflictRoute::ForceUnsafeAllowedStaleFlag => {
            handle_force_unsafe_completion(stage.clone(), &stage_id, false, work_dir)?;
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
