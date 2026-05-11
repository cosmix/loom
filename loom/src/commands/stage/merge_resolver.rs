//! Merge conflict resolver spawning for CLI path
//!
//! When the daemon is not running, this module handles spawning
//! a merge resolution session directly from the CLI.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::daemon::DaemonServer;
use crate::git::branch::branch_name_for_stage;
use crate::git::merge::InProgressMerge;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::continuation::save_session;
use crate::orchestrator::preflight::resolve_project_backend;
use crate::orchestrator::signals::{find_live_merge_session_for_stage, generate_merge_signal};
use crate::orchestrator::terminal::dispatcher::{BackendDispatcher, BackendNeeds};

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
/// When the daemon is not running, this spawns a native terminal session
/// to resolve merge conflicts. If the daemon IS running, it returns early
/// since the daemon handles merge resolution automatically.
///
/// # Arguments
/// * `stage` - The stage with merge conflicts (must be in MergeConflict or MergeBlocked status)
/// * `conflicting_files` - List of files with conflicts
/// * `merge_point` - The target branch to merge into
/// * `repo_root` - Path to the main repository root
/// * `work_dir` - Path to the .work directory
pub fn spawn_merge_resolver(
    stage: &Stage,
    conflicting_files: &[String],
    merge_point: &str,
    in_progress: Option<InProgressMerge>,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<MergeResolverResult> {
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

    // If daemon is running, it handles merge resolution automatically
    if DaemonServer::is_running(work_dir) {
        return Ok(MergeResolverResult::DaemonManaged);
    }

    // Refuse to spawn a duplicate resolver if a live one already exists.
    // (Stale signals are cleaned up by the helper.)
    if let Some(session_id) = find_live_merge_session_for_stage(&stage.id, work_dir)? {
        return Ok(MergeResolverResult::AlreadyRunning { session_id });
    }

    // Resolve project backend and construct a dispatcher that only
    // includes the backend the merge session will actually use.
    let backend_type =
        resolve_project_backend(work_dir).context("Backend preflight failed for merge resolver")?;
    let needs = BackendNeeds::from_project_and_overrides(backend_type, &[]);
    let dispatcher = BackendDispatcher::for_plan(backend_type, needs, work_dir)
        .context("Failed to construct backend dispatcher for merge resolver")?;

    // Get the source branch name for this stage
    let source_branch = branch_name_for_stage(&stage.id);

    // Create a merge resolution session, tagged with the project backend
    // so subsequent kill/liveness routes correctly.
    let mut session = Session::new_merge(source_branch.clone(), merge_point.to_string());
    session.set_backend(backend_type);
    let session_id = session.id.clone();

    // Generate the merge signal file
    let signal_path = generate_merge_signal(
        &session,
        stage,
        &source_branch,
        merge_point,
        conflicting_files,
        in_progress.as_ref(),
        work_dir,
    )
    .context("Failed to generate merge signal")?;

    // Spawn the merge session via the dispatcher.
    let spawned_session = dispatcher
        .for_stage(backend_type)
        .spawn_merge_session(stage, session, &signal_path, repo_root)
        .context("Failed to spawn merge resolver session")?;

    // Save the session file
    save_session(&spawned_session, work_dir).context("Failed to save merge resolver session")?;

    Ok(MergeResolverResult::Spawned(session_id))
}
