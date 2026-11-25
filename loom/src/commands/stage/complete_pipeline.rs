use super::super::progressive_complete::{branch_work, warn_unconfirmed, BranchWork, Landing};
use super::complete_force::landing_note;
use super::*;
use crate::handoff::completion::content::VerificationSnapshot;

/// Run verification phase (goal-backward verification)
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
        daemon_gates: false,
        fail_closed: false,
        expected_policy: None,
        emit: complete_verification::print_event,
        work_dir: phase.work_dir,
    })?;
    let snapshot = phase.snapshot.context("missing verification snapshot")?;
    snapshot.validate(phase.stage, phase.work_dir)?;
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
    complete_with_merge(stage, &repo_root, work_dir, Landing::Session)?;
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
    let note = landing_note(stage_id, DaemonServer::is_running(work_dir));
    println!("{}", landing_line(note, snapshot.is_some()));
    Ok(())
}

/// What `complete_without_verification` prints about landing: the landing note
/// when a commit was recorded, else that the stage cannot merge.
fn landing_line(note: String, recorded: bool) -> String {
    if recorded {
        note
    } else {
        "  → No commit recorded for landing: the stage cannot merge until a commit is recorded."
            .to_string()
    }
}

// An explicit operator skip still names the exact committed content authorized
// for landing. Automatic recovery must never manufacture this authorization.
// The stage then lands through the daemon's merge gate, which counts commits
// again, so an unconfirmed ahead-count is reported and the tip still recorded.
fn no_verify_snapshot(
    stage: &Stage,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<Option<VerificationSnapshot>> {
    let stage_id = &stage.id;
    match branch_work(stage, repo_root, work_dir)? {
        BranchWork::Missing { branch } => {
            eprintln!(
                "Warning: branch '{branch}' does not exist; completing stage '{stage_id}' \
                 with no commit recorded for landing."
            );
            Ok(None)
        }
        BranchWork::Committed(snapshot) => Ok(Some(snapshot)),
        BranchWork::Unconfirmed { snapshot, error } => {
            warn_unconfirmed(&error);
            Ok(Some(snapshot))
        }
        BranchWork::Empty { branch, target } => anyhow::bail!(
            "Refusing to --no-verify-complete stage '{stage_id}': branch \
             '{branch}' has zero commits beyond '{target}'. \
             The agent never committed any work for this stage, so \
             completing now would create a phantom merge (merged=true \
             against the unchanged base). Either redo the stage so the \
             agent commits real work, run `loom stage reset --kill-session \
             {stage_id}` to kill the session and re-queue, or, if the stage \
             is intentionally empty, run `loom stage complete {stage_id} \
             --force-unsafe --assume-merged`."
        ),
    }
}

pub(super) fn complete_stage(
    mut stage: Stage,
    session_id: Option<String>,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    work_dir: &Path,
) -> Result<()> {
    crate::commands::stage::native_gate::ensure_no_native_workers(work_dir, &stage.id)?;
    if force_unsafe && no_verify {
        eprintln!("Note: --force-unsafe runs no checks, so --no-verify adds nothing.");
    }
    // Only the operator's shell reaches this pipeline (a session's
    // completion is a host job).
    if super::complete_dispatch::dispatch_route(&mut stage, force_unsafe, assume_merged, work_dir)?
    {
        return Ok(());
    }
    complete_normal_stage(stage, session_id, no_verify, work_dir)
}

fn complete_normal_stage(
    mut stage: Stage,
    session_id: Option<String>,
    no_verify: bool,
    work_dir: &Path,
) -> Result<()> {
    let stage_id = stage.id.clone();
    let session_id = session_id
        .or_else(|| stage.session.clone())
        .or_else(|| find_session_for_stage(&stage_id, work_dir));
    let execution_paths = resolve_stage_execution_paths(&stage)?;
    let StageExecutionPaths {
        worktree_root,
        acceptance_dir,
    } = &execution_paths;
    fold_back_main_approvals(worktree_root);
    let snapshot = if no_verify {
        None
    } else {
        Some(crate::handoff::completion::content::VerificationSnapshot::capture(&stage, work_dir)?)
    };
    verify_acceptance(&stage, no_verify, &execution_paths, work_dir)?;
    run_verification_phase(VerificationPhase {
        snapshot: snapshot.as_ref(),
        stage: &mut stage,
        stage_id: &stage_id,
        no_verify,
        acceptance_dir,
        worktree_root,
        session_id: session_id.as_deref(),
        work_dir,
    })?;
    Ok(())
}

fn verify_acceptance(
    stage: &Stage,
    no_verify: bool,
    paths: &StageExecutionPaths,
    work_dir: &Path,
) -> Result<()> {
    let result = run_acceptance_phase(stage, &stage.id, no_verify, paths, work_dir)?;
    ensure_acceptance_passed(result, &stage.id)
}

#[cfg(test)]
mod landing_tests {
    use super::*;

    #[test]
    fn an_unverified_completion_names_how_a_recorded_commit_lands() {
        let note = landing_note("s1", false);

        let recorded = landing_line(note.clone(), true);
        let none = landing_line(note.clone(), false);

        assert_eq!(recorded, note);
        assert!(recorded.contains("`loom run` merges it"), "{recorded}");
        assert_eq!(
            none,
            "  → No commit recorded for landing: the stage cannot merge until a commit is recorded."
        );
    }
}

#[cfg(test)]
mod native_gate_tests {
    use super::*;
    use crate::models::stage::StageStatus;
    use crate::verify::transitions::{create_stage, load_stage};

    #[test]
    fn operator_completion_refuses_active_worker() {
        let temp = tempfile::TempDir::new().unwrap();
        let work = temp.path();
        let mut stage = Stage::new("s1".into(), None);
        stage.id = "s1".into();
        stage.status = StageStatus::Executing;
        create_stage(&stage, work).unwrap();
        crate::commands::stage::native_gate::tests::seed_active_worker(work, "s1", "unit");
        let error = complete_stage(stage, None, false, true, false, work).unwrap_err();
        assert!(format!("{error:#}").contains("running native Codex workers"));
        assert_eq!(
            load_stage("s1", work).unwrap().status,
            StageStatus::Executing
        );
    }
}
