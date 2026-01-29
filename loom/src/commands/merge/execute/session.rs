//! Merge session handling
//!
//! Contains logic for spawning Claude Code sessions to resolve merge conflicts.

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::default_branch;
use crate::models::session::Session;
use crate::orchestrator::signals::generate_merge_signal;
use crate::orchestrator::terminal::{create_backend, BackendType};
use crate::verify::transitions::load_stage;

/// Spawn a merge conflict resolution session
pub fn spawn_merge_resolution_session(
    stage_id: &str,
    conflicts: &[String],
    repo_root: &Path,
    work_dir: &Path,
) -> Result<String> {
    // Load stage for signal generation
    let stage = load_stage(stage_id, work_dir)?;

    // Get target branch
    let target_branch =
        default_branch(repo_root).with_context(|| "Failed to detect default branch")?;

    // Create a new merge session
    let session = Session::new();
    let source_branch = branch_name_for_stage(stage_id);

    // Generate merge signal
    let signal_path = generate_merge_signal(
        &session,
        &stage,
        &source_branch,
        &target_branch,
        conflicts,
        work_dir,
    )?;

    // Create terminal backend and spawn session
    let backend = create_backend(BackendType::Native, work_dir)?;
    let spawned_session = backend.spawn_merge_session(&stage, session, &signal_path, repo_root)?;

    Ok(spawned_session.id)
}
