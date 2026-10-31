//! Merge conflict resolver spawning for CLI path
//!
//! When the daemon is not running, this module handles spawning
//! a merge resolution session directly from the CLI.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::daemon::DaemonServer;
use crate::fs::session_files::mark_session_terminal_reason;
use crate::git::branch::branch_name_for_stage;
use crate::git::merge::control_path_violation;
use crate::models::session::{Session, SessionBackendKind, SessionExitReason, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::signals::{
    find_live_merge_session_for_stage_unlocked, generate_merge_signal, publish_merge_session,
    with_merge_resolver_lock, write_ahead_merge_session,
};
use crate::orchestrator::terminal::backend::{merge_resolver_worktree, SessionBackend};

use super::progressive_complete::route_held_to_review;

/// Result of attempting to spawn a merge resolver session.
pub enum MergeResolverResult {
    /// Daemon is running and will handle merge resolution automatically.
    DaemonManaged,
    /// A merge resolver session was spawned with the given session ID.
    Spawned(String),
    /// A live merge resolver session is already running for this stage.
    AlreadyRunning { session_id: String },
}

/// Spawn a merge conflict resolver session from the CLI.
///
/// When the daemon is not running, this spawns a session through the configured backend
/// to resolve merge conflicts. If the daemon IS running, it returns early
/// since the daemon handles merge resolution automatically.
///
/// # Arguments
/// * `stage` - The stage with merge conflicts (must be in MergeConflict or MergeBlocked status)
/// * `conflicting_files` - List of files with conflicts
/// * `merge_point` - The target branch to merge into
/// * `repo_root` - Path to the main repository root, where `.worktrees/` lives
/// * `work_dir` - Path to the state directory
pub fn spawn_merge_resolver(
    stage: &Stage,
    conflicting_files: &[String],
    merge_point: &str,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<MergeResolverResult> {
    // Reset and resolver admission share this lock; always acquire it before
    // the resolver lock, and re-read the stage after a possibly long wait.
    let _admission = crate::fs::stage_admission::lock(work_dir, &stage.id)?;
    let stage = crate::verify::transitions::load_stage(&stage.id, work_dir)?;
    // Validate stage is in an appropriate status for merge resolution
    if !matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    ) {
        bail!(
            "Cannot spawn merge resolver for stage '{}' in status '{}' (expected MergeConflict or MergeBlocked)",
            stage.id,
            stage.status
        );
    }

    // A typed block is not a conflict: no resolver can clear it.
    if let (StageStatus::MergeBlocked, Some(block)) = (&stage.status, &stage.merge.block) {
        bail!(
            "stage '{id}' is blocked ({block}), not in conflict: it needs no resolver; loom \
             retries the merge once that changes, or run `loom stage merge {id}`",
            id = stage.id
        );
    }

    // If daemon is running, it handles merge resolution automatically
    if DaemonServer::is_running(work_dir) {
        return Ok(MergeResolverResult::DaemonManaged);
    }

    with_merge_resolver_lock(work_dir, &stage.id, || {
        if let Some(session_id) = find_live_merge_session_for_stage_unlocked(&stage.id, work_dir)? {
            return Ok(MergeResolverResult::AlreadyRunning { session_id });
        }
        refuse_control_path_branch(&stage, merge_point, repo_root, work_dir)?;
        spawn_resolver_session(&stage, conflicting_files, merge_point, repo_root, work_dir)
    })
}

/// Refuse to hand a branch that touches a control path to a resolver session:
/// the stage goes to human review and the spawn fails. A diff that cannot be
/// computed fails the spawn too.
fn refuse_control_path_branch(
    stage: &Stage,
    merge_point: &str,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<()> {
    let branch = branch_name_for_stage(&stage.id);
    let violation = control_path_violation(repo_root, merge_point, &branch).with_context(|| {
        format!(
            "Cannot spawn a merge resolver for stage '{}': the control-path check failed",
            stage.id
        )
    })?;
    let Some(reason) = violation else {
        return Ok(());
    };
    route_held_to_review(&stage.id, work_dir, &reason)?;
    bail!(
        "Not spawning a merge resolver for stage '{}': the branch touches a control path",
        stage.id
    )
}

/// Write the merge signal and spawn the resolver in the stage worktree.
fn spawn_resolver_session(
    stage: &Stage,
    conflicting_files: &[String],
    merge_point: &str,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<MergeResolverResult> {
    let backend = SessionBackend::from_config(work_dir.to_path_buf())
        .context("Failed to construct session backend for merge resolver")?;

    // The resolver works in the stage worktree; without it the operator
    // recreates the worktree or resolves by hand. Checked before the signal
    // is written so a missing worktree leaves nothing behind.
    let worktree = merge_resolver_worktree(repo_root, &stage.id).with_context(|| {
        format!(
            "Cannot spawn a merge resolver for stage '{}': recreate its worktree or resolve the \
             conflict by hand",
            stage.id
        )
    })?;

    let source_branch = branch_name_for_stage(&stage.id);
    let mut session = Session::new_merge(source_branch.clone(), merge_point.to_string());
    let session_id = session.id.clone();

    let signal_path = generate_merge_signal(
        &session,
        stage,
        &source_branch,
        merge_point,
        conflicting_files,
        work_dir,
    )
    .context("Failed to generate merge signal")?;

    write_ahead_merge_session(&mut session, &stage.id, backend.resolve_lane(), work_dir)?;
    let spawned_session =
        launch_resolver(&backend, stage, &worktree, session, &signal_path, work_dir)?;
    publish_merge_session(&spawned_session, work_dir)
        .context("Failed to publish merge resolver session")?;
    Ok(MergeResolverResult::Spawned(session_id))
}

fn launch_resolver(
    backend: &SessionBackend,
    stage: &Stage,
    worktree: &crate::models::worktree::Worktree,
    session: Session,
    signal_path: &Path,
    work_dir: &Path,
) -> Result<Session> {
    #[cfg(test)]
    let injected =
        crate::orchestrator::signals::merge_test_backend::spawn(work_dir, session.clone());
    #[cfg(not(test))]
    let injected: Option<Result<Session>> = None;
    match injected.unwrap_or_else(|| {
        backend.spawn_merge_session_in_worktree(stage, worktree, session.clone(), signal_path)
    }) {
        Ok(spawned) => Ok(spawned),
        Err(error) => {
            let killed = backend.kill_session(&session).is_ok();
            let gone = match session.backend {
                SessionBackendKind::Tmux => {
                    killed && matches!(backend.is_session_alive(&session), Ok(false))
                }
                SessionBackendKind::Native => matches!(
                    crate::orchestrator::terminal::native::session_process_status(
                        work_dir, &session
                    ),
                    crate::orchestrator::terminal::native::SessionProcessStatus::Dead
                ),
            };
            settle_failed_launch(work_dir, &session, gone)?;
            return Err(error.context(if gone {
                "Failed to spawn merge resolver session"
            } else {
                "Failed to spawn merge resolver session; it may still be running"
            }));
        }
    }
}

fn settle_failed_launch(work_dir: &Path, session: &Session, gone: bool) -> Result<()> {
    if !gone {
        return Ok(());
    }
    mark_session_terminal_reason(
        work_dir,
        &session.id,
        SessionStatus::Crashed,
        SessionExitReason::Crashed,
    )?;
    crate::orchestrator::signals::remove_signal(&session.id, work_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn cli_resolver_rechecks_stage_after_reset_admission() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        let mut stage = Stage::new("alpha".into(), None);
        stage.id = "alpha".into();
        stage.status = StageStatus::MergeConflict;
        crate::verify::transitions::save_stage(&stage, work).unwrap();
        let admission = crate::fs::stage_admission::lock(work, "alpha").unwrap();
        let (started, ready) = mpsc::channel();
        let (finished, result) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                started.send(()).unwrap();
                let error = spawn_merge_resolver(&stage, &[], "main", work, work)
                    .err()
                    .expect("reset stage cannot launch a resolver");
                finished.send(error.to_string()).unwrap();
            });
            ready.recv().unwrap();
            assert!(result.recv_timeout(Duration::from_millis(50)).is_err());
            crate::verify::transitions::update_stage("alpha", work, |current| {
                current.status = StageStatus::WaitingForDeps;
                Ok(())
            })
            .unwrap();
            drop(admission);
            assert!(result.recv().unwrap().contains("expected MergeConflict"));
        });
    }

    #[test]
    fn failed_launch_retains_uncertain_evidence_until_proven_gone() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        let mut stage = Stage::new("alpha".into(), None);
        stage.id = "alpha".into();
        let mut session = Session::new_merge("loom/alpha".into(), "main".into());
        generate_merge_signal(&session, &stage, "loom/alpha", "main", &[], work).unwrap();
        write_ahead_merge_session(&mut session, "alpha", SessionBackendKind::Tmux, work).unwrap();

        settle_failed_launch(work, &session, false).unwrap();
        assert_eq!(
            crate::orchestrator::signals::find_live_merge_session_for_stage("alpha", work).unwrap(),
            Some(session.id.clone())
        );
        settle_failed_launch(work, &session, true).unwrap();
        assert_eq!(
            crate::orchestrator::signals::find_live_merge_session_for_stage("alpha", work).unwrap(),
            None
        );
        let stored = crate::fs::session_files::load_session_exact(work, &session.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, SessionStatus::Crashed);
    }
}
