//! Session continuation with handoff context.
//!
//! This module handles resuming work on a stage after a session hands off due to
//! context exhaustion or other reasons. It provides functionality to:
//!
//! - Prepare continuation context (stage, handoff, worktree)
//! - Create new sessions with handoff references
//! - Generate signals that include handoff file paths for context restoration
//! - Optionally spawn tmux sessions to continue work

mod context;
mod session_io;
mod yaml_parse;

#[cfg(test)]
mod tests;

pub use context::{load_handoff_content, prepare_continuation, ContinuationContext};
pub use session_io::{save_session, session_to_markdown};
pub use yaml_parse::{extract_yaml_frontmatter, parse_stage_from_markdown};

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::models::worktree::Worktree;
use crate::orchestrator::signals::{generate_signal, DependencyStatus};
use crate::orchestrator::terminal::{create_backend, BackendType};

/// Configuration for session continuation
#[derive(Debug, Clone)]
pub struct ContinuationConfig {
    /// Backend type for spawning sessions
    pub backend_type: BackendType,
    /// Whether to automatically spawn a terminal session
    pub auto_spawn: bool,
}

impl Default for ContinuationConfig {
    fn default() -> Self {
        Self {
            backend_type: BackendType::Native,
            auto_spawn: true,
        }
    }
}

/// Continue work on a stage after a handoff
///
/// Creates a new session, generates signal file with handoff reference,
/// optionally spawns tmux session, and updates stage status.
///
/// # Arguments
/// * `stage` - The stage to continue work on
/// * `handoff_path` - Optional path to the handoff file for context restoration
/// * `worktree` - The worktree where work will continue
/// * `config` - Configuration for continuation (spawner settings, auto_spawn)
/// * `work_dir` - The .work directory path
///
/// # Returns
/// A new Session ready to continue the work
pub fn continue_session(
    stage: &Stage,
    handoff_path: Option<&Path>,
    worktree: &Worktree,
    config: &ContinuationConfig,
    work_dir: &Path,
) -> Result<Session> {
    validate_stage_for_continuation(stage)?;

    let mut session = Session::new();
    session.assign_to_stage(stage.id.clone());
    session.set_worktree_path(worktree.path.clone());

    let handoff_file = extract_handoff_filename(handoff_path);
    let dependencies_status: Vec<DependencyStatus> = Vec::new();
    let original_session_id = session.id.clone();

    let signal_path = generate_signal(
        &session,
        stage,
        worktree,
        &dependencies_status,
        handoff_file.as_deref(),
        None,
        work_dir,
    )
    .context("Failed to generate signal for continuation")?;

    if config.auto_spawn {
        let backend = create_backend(config.backend_type)
            .context("Failed to create terminal backend for continuation")?;
        session = backend
            .spawn_session(stage, worktree, session, &signal_path)
            .context("Failed to spawn session for continuation")?;
    }

    debug_assert_eq!(
        original_session_id, session.id,
        "Session ID mismatch: signal file created with '{}' but saving session with '{}'",
        original_session_id, session.id
    );

    save_session(&session, work_dir)?;

    Ok(session)
}

fn validate_stage_for_continuation(stage: &Stage) -> Result<()> {
    if !matches!(
        stage.status,
        StageStatus::NeedsHandoff | StageStatus::Queued | StageStatus::Executing
    ) {
        bail!(
            "Stage {} is in status {:?}, which cannot be continued. Expected NeedsHandoff, Ready, or Executing.",
            stage.id,
            stage.status
        );
    }
    Ok(())
}

fn extract_handoff_filename(handoff_path: Option<&Path>) -> Option<String> {
    handoff_path.and_then(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    })
}
