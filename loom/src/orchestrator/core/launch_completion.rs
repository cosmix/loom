//! Completing a launch whose answer the daemon lost: the agent runs and its
//! wrapper wrote the PID file, but the `Running` record was never published.

use std::path::PathBuf;

use anyhow::Result;

use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::worktree::Worktree;
use crate::orchestrator::terminal::native::{read_pid_entry, NativeBackend};

use super::Orchestrator;

/// Fill in what the launch's lost answer would have published: the PID from
/// the wrapper's PID file, the worktree path when the session runs in one,
/// and the `Running` status.
pub(super) fn complete_lost_launch(
    work_dir: &std::path::Path,
    session: &mut Session,
    worktree_path: Option<PathBuf>,
) -> Result<()> {
    let identity = NativeBackend::window_title_and_pid_key(session)
        .and_then(|(_, key)| read_pid_entry(work_dir, &key));
    if let Some(identity) = identity {
        session.set_pid(identity.pid);
    }
    if let Some(path) = worktree_path {
        session.set_worktree_path(path);
    }
    session.try_mark_running()
}

impl Orchestrator {
    /// The record to track for a live current `session` found at daemon start.
    /// A `Spawning` one is completed and saved; on any failure it is returned
    /// as found, because a live session is never dropped.
    pub(super) fn settled_live_session(&self, session: &Session) -> Session {
        let mut settled = session.clone();
        if settled.status != SessionStatus::Spawning {
            return settled;
        }
        let in_worktree = matches!(
            settled.session_type,
            SessionType::Stage | SessionType::Contract | SessionType::Merge
        );
        let worktree_path = settled
            .stage_id
            .as_deref()
            .filter(|_| in_worktree)
            .map(|id| Worktree::worktree_path(&self.config.repo_root, id));
        let work_dir = &self.config.work_dir;
        let outcome = complete_lost_launch(work_dir, &mut settled, worktree_path)
            .and_then(|()| save_session(&settled, work_dir));
        match outcome {
            Ok(()) => settled,
            Err(error) => {
                tracing::warn!(session_id = %session.id, error = %format!("{error:#}"),
                    "Cannot complete a launch left Spawning; tracking the record as found");
                session.clone()
            }
        }
    }
}
