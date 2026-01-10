//! Event handlers for monitor events

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::handoff::{generate_handoff, HandoffContent};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::signals::read_merge_signal;
use crate::orchestrator::spawner::{generate_crash_report, CrashReport};
use crate::orchestrator::terminal::tmux::session_is_running;

use super::config::MonitorConfig;
use super::context::context_usage_percent;

/// Handler functions for monitor events
pub struct Handlers {
    config: MonitorConfig,
}

impl Handlers {
    pub fn new(config: MonitorConfig) -> Self {
        Self { config }
    }

    /// Check if a session is still alive by checking its process
    ///
    /// First checks the PID if available (works for both native and tmux sessions).
    /// Falls back to checking tmux session if PID check fails or is unavailable.
    pub fn check_session_alive(&self, session: &Session) -> Result<Option<bool>> {
        // First check PID if available (works for both native and tmux sessions)
        if let Some(pid) = session.pid {
            let output = std::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
                .context("Failed to check if process is alive")?;

            if output.status.success() {
                return Ok(Some(true));
            }
            // PID is dead, but let's also check tmux in case PID tracking was lost
        }

        // Fall back to tmux check if available
        if let Some(tmux_name) = &session.tmux_session {
            return check_tmux_session_alive(tmux_name).map(Some);
        }

        // No PID and no tmux session - cannot track liveness
        Ok(None)
    }

    /// Check if a session is a merge session (has a merge signal file)
    pub fn is_merge_session(&self, session_id: &str) -> bool {
        matches!(
            read_merge_signal(session_id, &self.config.work_dir),
            Ok(Some(_))
        )
    }

    /// Handle critical context by generating a handoff file
    ///
    /// Called when a session reaches critical context threshold.
    /// Loads session and stage data, creates handoff content, and generates the handoff file.
    pub fn handle_context_critical(&self, session: &Session, stage: &Stage) -> Result<PathBuf> {
        let context_percent = context_usage_percent(session.context_tokens, session.context_limit);

        let goals = stage
            .description
            .clone()
            .unwrap_or_else(|| format!("Work on stage: {}", stage.name));

        let content = HandoffContent::new(session.id.clone(), stage.id.clone())
            .with_context_percent(context_percent)
            .with_goals(goals)
            .with_plan_id(stage.plan_id.clone())
            .with_next_steps(vec![
                "Review handoff and continue from current state".to_string()
            ]);

        generate_handoff(session, stage, content, &self.config.work_dir)
    }

    /// Handle session crash by generating a crash report
    ///
    /// Called when a session crash is detected.
    /// Creates a CrashReport and generates the crash report file.
    pub fn handle_session_crash(&self, session: &Session, reason: &str) -> Option<PathBuf> {
        let mut report = CrashReport::new(
            session.id.clone(),
            session.stage_id.clone(),
            reason.to_string(),
        );

        // Add tmux session info if available
        if let Some(tmux_session) = &session.tmux_session {
            report = report.with_tmux_session(tmux_session.clone());
        }

        // Generate the crash report
        let crashes_dir = self.config.work_dir.join("crashes");
        let logs_dir = self.config.work_dir.join("logs");

        match generate_crash_report(&report, &crashes_dir, &logs_dir) {
            Ok(path) => {
                eprintln!("Generated crash report: {}", path.display());
                Some(path)
            }
            Err(e) => {
                eprintln!(
                    "Failed to generate crash report for session '{}': {}",
                    session.id, e
                );
                None
            }
        }
    }
}

/// Check if a tmux session is still running
fn check_tmux_session_alive(tmux_name: &str) -> Result<bool> {
    session_is_running(tmux_name)
}
