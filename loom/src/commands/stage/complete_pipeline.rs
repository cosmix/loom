use super::*;
use crate::handoff::completion::content::VerificationSnapshot;

/// Run verification phase (goal-backward verification and change impact comparison)
///
/// If verifications pass, performs progressive merge. If --no-verify is used, skips all checks.
pub(super) struct VerificationPhase<'a> {
    pub snapshot: Option<&'a crate::handoff::completion::content::VerificationSnapshot>,
    pub stage: &'a mut Stage,
    pub stage_id: &'a str,
    pub no_verify: bool,
    pub acceptance_dir: &'a Option<PathBuf>,
    pub worktree_root: &'a Option<PathBuf>,
    pub session_id: Option<&'a str>,
    pub control_session: Option<&'a str>,
    pub work_dir: &'a Path,
}

pub(super) fn run_verification_phase(phase: VerificationPhase<'_>) -> Result<()> {
    if phase.no_verify {
        return complete_without_verification(phase.stage, phase.stage_id, phase.work_dir);
    }
    complete_verification::run(&complete_verification::VerificationChecks {
        stage: phase.stage,
        stage_id: phase.stage_id,
        acceptance_dir: phase.acceptance_dir.as_deref(),
        worktree_root: phase.worktree_root.as_deref(),
        control_session: phase.control_session,
        work_dir: phase.work_dir,
    })?;
    let snapshot = phase.snapshot.context("missing verification snapshot")?;
    snapshot.validate(phase.stage, phase.work_dir)?;
    if let Some(control) = phase.control_session {
        return emit_control_completion_evidence(phase.stage, phase.stage_id, control, snapshot);
    }
    merge_verified_stage(phase, snapshot)
}

fn merge_verified_stage(
    phase: VerificationPhase<'_>,
    snapshot: &crate::handoff::completion::content::VerificationSnapshot,
) -> Result<()> {
    let VerificationPhase {
        stage,
        stage_id,
        session_id,
        work_dir,
        ..
    } = phase;
    if let Some(sid) = session_id {
        cleanup_session_resources(stage_id, sid, work_dir);
    }
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
    snapshot.validate(stage, work_dir)?;
    update_stage(stage_id, work_dir, |current| {
        snapshot.validate_locked(current, work_dir)?;
        current.completed_commit = Some(snapshot.commit.clone());
        Ok(())
    })?;
    stage.completed_commit = Some(snapshot.commit.clone());
    complete_with_merge(stage, &repo_root, work_dir, crate::git::MergeGate::Enforce)?;
    Ok(())
}

fn complete_without_verification(stage: &mut Stage, stage_id: &str, work_dir: &Path) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
    let snapshot = no_verify_snapshot(stage, &repo_root, work_dir)?;
    *stage = update_stage(stage_id, work_dir, |current| {
        if let Some(snapshot) = &snapshot {
            snapshot.validate_locked(current, work_dir)?;
            current.completed_commit = Some(snapshot.commit.clone());
        }
        current.try_complete(None)
    })?;
    println!("Stage '{stage_id}' completed (skipped verification).");
    println!("The orchestrator will handle merge and dependent triggering.");
    Ok(())
}

// An explicit operator skip still names the exact committed content authorized
// for landing. Automatic recovery must never manufacture this authorization.
fn no_verify_snapshot(
    stage: &Stage,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<Option<VerificationSnapshot>> {
    let stage_id = &stage.id;
    let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
    let stage_branch = crate::git::branch::branch_name_for_stage(stage_id);
    let branch_exists =
        crate::git::branch::branch_exists(&stage_branch, repo_root).unwrap_or(false);
    if branch_exists {
        match crate::git::branch::commits_ahead_of(&stage_branch, &target_branch, repo_root) {
            Ok(0) => {
                anyhow::bail!(
                    "Refusing to --no-verify-complete stage '{stage_id}': branch \
                     '{stage_branch}' has zero commits beyond '{target_branch}'. \
                     The agent never committed any work for this stage, so \
                     completing now would create a phantom merge (merged=true \
                     against the unchanged base). Either redo the stage so the \
                     agent commits real work, run `loom stage reset --kill-session \
                     {stage_id}` to kill the session and re-queue, or use \
                     `loom stage complete --force-unsafe` if you genuinely intend \
                     to mark an empty stage complete."
                );
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "Warning: failed to count commits ahead of '{target_branch}' on \
                     '{stage_branch}': {e}. Proceeding with --no-verify completion."
                );
            }
        }
    }
    branch_exists
        .then(|| VerificationSnapshot::capture(stage, work_dir))
        .transpose()
}

fn emit_control_completion_evidence(
    stage: &Stage,
    stage_id: &str,
    control_session: &str,
    snapshot: &crate::handoff::completion::content::VerificationSnapshot,
) -> Result<()> {
    print_sandboxed_completion_pending_notice(stage_id);
    super::super::completion_producer::emit_verified_evidence(stage, control_session, snapshot)
}

pub(super) fn complete_stage(
    mut stage: Stage,
    session_id: Option<String>,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    control_session: Option<String>,
    work_dir: &Path,
) -> Result<()> {
    if super::complete_dispatch::dispatch_route(
        &mut stage,
        force_unsafe,
        assume_merged,
        control_session.is_some(),
        work_dir,
    )? {
        return Ok(());
    }
    complete_normal_stage(stage, session_id, no_verify, control_session, work_dir)
}

fn complete_normal_stage(
    mut stage: Stage,
    session_id: Option<String>,
    no_verify: bool,
    control_session: Option<String>,
    work_dir: &Path,
) -> Result<()> {
    let stage_id = stage.id.clone();
    let session_id = session_id
        .or_else(|| stage.session.clone())
        .or_else(|| find_session_for_stage(&stage_id, work_dir));
    let execution_paths = resolve_stage_execution_paths(&stage)?;
    let working_dir: Option<PathBuf> = execution_paths.worktree_root;
    let acceptance_dir: Option<PathBuf> = execution_paths.acceptance_dir;
    if control_session.is_none() {
        sync_worktree_permissions(&working_dir, &acceptance_dir);
    }
    let snapshot = if no_verify {
        None
    } else {
        Some(crate::handoff::completion::content::VerificationSnapshot::capture(&stage, work_dir)?)
    };
    verify_acceptance(&stage, no_verify, acceptance_dir.as_deref(), work_dir)?;
    run_verification_phase(VerificationPhase {
        snapshot: snapshot.as_ref(),
        stage: &mut stage,
        stage_id: &stage_id,
        no_verify,
        acceptance_dir: &acceptance_dir,
        worktree_root: &working_dir,
        session_id: session_id.as_deref(),
        control_session: control_session.as_deref(),
        work_dir,
    })?;
    Ok(())
}

fn verify_acceptance(
    stage: &Stage,
    no_verify: bool,
    dir: Option<&Path>,
    work_dir: &Path,
) -> Result<()> {
    let result = run_acceptance_phase(stage, &stage.id, no_verify, dir, work_dir)?;
    ensure_acceptance_passed(result, &stage.id)
}
