//! tmux terminal backend
//!
//! Spawns Claude Code sessions in tmux sessions with stability improvements
//! based on gastown patterns (debouncing, history clearing, zombie detection).

mod helpers;
mod query;
mod session_ops;
#[cfg(test)]
mod tests;
mod types;

use anyhow::{anyhow, bail, Result};
use shell_escape::escape;
use std::borrow::Cow;
use std::path::Path;

use super::{BackendType, TerminalBackend};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

// Re-exports
pub use helpers::{
    check_tmux_available, clear_session_history, enable_pane_logging, send_keys,
};
pub use query::{
    get_tmux_session_info, is_agent_running, list_tmux_sessions, session_is_running,
};
pub use session_ops::kill_session_by_name;
pub use types::{TmuxSessionInfo, TMUX_DEBOUNCE_MS};

/// tmux terminal backend - spawns sessions in tmux
pub struct TmuxBackend {
    /// Prefix for tmux session names
    prefix: String,
}

impl TmuxBackend {
    /// Create a new tmux backend
    pub fn new() -> Result<Self> {
        check_tmux_available()?;
        Ok(Self {
            prefix: "loom".to_string(),
        })
    }

    /// Get the session name prefix
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Generate session name from stage ID
    fn session_name(&self, stage_id: &str) -> String {
        format!("{}-{}", self.prefix, stage_id)
    }
}

impl TerminalBackend for TmuxBackend {
    fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        let session_name = self.session_name(&stage.id);
        let worktree_path = worktree.path.to_str().ok_or_else(|| {
            anyhow!(
                "Worktree path contains invalid UTF-8: {}",
                worktree.path.display()
            )
        })?;

        // Check if session already exists (zombie-aware check)
        session_ops::ensure_session_fresh(&session_name)?;

        // Create tmux session in detached mode with working directory
        session_ops::create_session(&session_name, worktree_path)?;

        // Configure session for stability
        helpers::configure_session_for_stability(&session_name)?;

        // Enable pipe-pane logging
        let log_dir = signal_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("logs"))
            .unwrap_or_else(|| std::path::PathBuf::from(".work/logs"));
        let log_path = log_dir.join(format!("{}.log", stage.id));
        helpers::enable_pane_logging(&session_name, &log_path)?;

        // Build the initial prompt
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the signal file at {signal_path_str} and execute the assigned stage work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and context files to read."
        );

        // Derive work_dir from signal_path
        let work_dir = signal_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".work".to_string());

        // Set environment variables
        helpers::set_tmux_environment(&session_name, "loom_SESSION_ID", &session.id)?;
        helpers::set_tmux_environment(&session_name, "loom_STAGE_ID", &stage.id)?;
        helpers::set_tmux_environment(&session_name, "loom_WORK_DIR", &work_dir)?;

        // Build and send the claude command
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));
        let claude_command = format!("claude {escaped_prompt}");

        if let Err(e) =
            helpers::send_keys_debounced(&session_name, &claude_command, TMUX_DEBOUNCE_MS)
        {
            let _ = kill_session_by_name(&session_name);
            return Err(anyhow!("Failed to send 'claude' command: {e}"));
        }

        // Get the PID
        let pid = helpers::get_tmux_session_pid(&session_name)?;

        // Update session
        let mut session = session;
        session.set_tmux_session(session_name.clone());
        session.set_worktree_path(worktree.path.clone());
        session.assign_to_stage(stage.id.clone());
        if let Some(pid) = pid {
            session.set_pid(pid);
        }
        session.try_mark_running()?;

        Ok(session)
    }

    fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        // Use a distinct prefix for merge sessions
        let session_name = format!("{}-merge-{}", self.prefix, stage.id);
        let repo_path = repo_root.to_str().ok_or_else(|| {
            anyhow!(
                "Repository path contains invalid UTF-8: {}",
                repo_root.display()
            )
        })?;

        // Check if session already exists (zombie-aware check)
        session_ops::ensure_session_fresh(&session_name)?;

        // Create tmux session in detached mode with main repository as working directory
        session_ops::create_session(&session_name, repo_path)?;

        // Configure session for stability
        helpers::configure_session_for_stability(&session_name)?;

        // Enable pipe-pane logging
        let log_dir = signal_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("logs"))
            .unwrap_or_else(|| std::path::PathBuf::from(".work/logs"));
        let log_path = log_dir.join(format!("merge-{}.log", stage.id));
        helpers::enable_pane_logging(&session_name, &log_path)?;

        // Build the initial prompt for merge resolution
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the merge signal file at {signal_path_str} and resolve the merge conflicts. \
             This file contains the conflicting files, merge context, and resolution instructions."
        );

        // Derive work_dir from signal_path
        let work_dir = signal_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".work".to_string());

        // Set environment variables - include merge-specific variables
        helpers::set_tmux_environment(&session_name, "loom_SESSION_ID", &session.id)?;
        helpers::set_tmux_environment(&session_name, "loom_STAGE_ID", &stage.id)?;
        helpers::set_tmux_environment(&session_name, "loom_WORK_DIR", &work_dir)?;
        helpers::set_tmux_environment(&session_name, "loom_SESSION_TYPE", "merge")?;

        // Build and send the claude command
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));
        let claude_command = format!("claude {escaped_prompt}");

        if let Err(e) =
            helpers::send_keys_debounced(&session_name, &claude_command, TMUX_DEBOUNCE_MS)
        {
            let _ = kill_session_by_name(&session_name);
            return Err(anyhow!("Failed to send 'claude' command for merge: {e}"));
        }

        // Get the PID
        let pid = helpers::get_tmux_session_pid(&session_name)?;

        // Update session
        // Note: For merge sessions, we don't set worktree_path since we're in the main repo
        let mut session = session;
        session.set_tmux_session(session_name.clone());
        session.assign_to_stage(stage.id.clone());
        if let Some(pid) = pid {
            session.set_pid(pid);
        }
        session.try_mark_running()?;

        Ok(session)
    }

    fn kill_session(&self, session: &Session) -> Result<()> {
        let session_name = session
            .tmux_session
            .as_ref()
            .ok_or_else(|| anyhow!("Session has no tmux_session name"))?;

        kill_session_by_name(session_name)
    }

    fn is_session_alive(&self, session: &Session) -> Result<bool> {
        if let Some(session_name) = &session.tmux_session {
            session_is_running(session_name)
        } else {
            Ok(false)
        }
    }

    fn attach_session(&self, session: &Session) -> Result<()> {
        let session_name = session
            .tmux_session
            .as_ref()
            .ok_or_else(|| anyhow!("Session has no tmux_session name"))?;

        if session.status != SessionStatus::Running {
            bail!("Session {} is not running", session.id);
        }

        session_ops::attach_session(session_name)
    }

    fn attach_all(&self, sessions: &[Session]) -> Result<()> {
        // For tmux, we use the tiled overview from the attach module
        // This is handled separately by the attach command
        // Here we just validate that sessions exist
        for session in sessions {
            if session.status == SessionStatus::Running {
                if let Some(session_name) = &session.tmux_session {
                    if !session_is_running(session_name)? {
                        eprintln!(
                            "Warning: Session {} tmux session '{}' not found",
                            session.id, session_name
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Tmux
    }
}
