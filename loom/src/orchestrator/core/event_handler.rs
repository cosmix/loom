//! Event handling - processing monitor events and session lifecycle

use anyhow::Result;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::models::stage::StageStatus;
use crate::orchestrator::monitor::MonitorEvent;
use crate::orchestrator::signals::remove_signal;
use crate::orchestrator::spawner::kill_session;

use super::persistence::Persistence;
use super::Orchestrator;

/// Clear the current line (status line) before printing a message.
/// This prevents output from being mangled when the status line is being updated.
fn clear_status_line() {
    // \r moves cursor to start of line, \x1B[K clears from cursor to end of line
    print!("\r\x1B[K");
    let _ = io::stdout().flush();
}

/// Trait for handling monitor events
pub(super) trait EventHandler: Persistence {
    /// Handle monitor events
    fn handle_events(&mut self, events: Vec<MonitorEvent>) -> Result<()>;

    /// Handle stage completion
    fn on_stage_completed(&mut self, stage_id: &str) -> Result<()>;

    /// Handle session crash
    fn on_session_crashed(
        &mut self,
        session_id: &str,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    ) -> Result<()>;

    /// Handle context exhaustion (needs handoff)
    fn on_needs_handoff(&mut self, session_id: &str, stage_id: &str) -> Result<()>;
}

impl EventHandler for Orchestrator {
    fn handle_events(&mut self, events: Vec<MonitorEvent>) -> Result<()> {
        for event in events {
            match event {
                MonitorEvent::StageCompleted { stage_id } => {
                    self.on_stage_completed(&stage_id)?;
                }
                MonitorEvent::StageBlocked { stage_id, reason } => {
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' blocked: {reason}");
                    self.graph.mark_blocked(&stage_id)?;
                }
                MonitorEvent::SessionContextWarning {
                    session_id,
                    usage_percent,
                } => {
                    clear_status_line();
                    eprintln!("Warning: Session '{session_id}' context at {usage_percent:.1}%");
                }
                MonitorEvent::SessionContextCritical {
                    session_id,
                    usage_percent,
                } => {
                    clear_status_line();
                    eprintln!("Critical: Session '{session_id}' context at {usage_percent:.1}%");
                }
                MonitorEvent::SessionCrashed {
                    session_id,
                    stage_id,
                    crash_report_path,
                } => {
                    self.on_session_crashed(&session_id, stage_id, crash_report_path)?;
                }
                MonitorEvent::SessionNeedsHandoff {
                    session_id,
                    stage_id,
                } => {
                    self.on_needs_handoff(&session_id, &stage_id)?;
                }
                MonitorEvent::StageWaitingForInput {
                    stage_id,
                    session_id,
                } => {
                    clear_status_line();
                    if let Some(sid) = session_id {
                        eprintln!("Stage '{stage_id}' (session '{sid}') is waiting for user input");
                    } else {
                        eprintln!("Stage '{stage_id}' is waiting for user input");
                    }
                }
                MonitorEvent::StageResumedExecution { stage_id } => {
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' resumed execution after user input");
                }
            }
        }
        Ok(())
    }

    fn on_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        self.graph.mark_completed(stage_id)?;

        if let Some(session) = self.active_sessions.remove(stage_id) {
            remove_signal(&session.id, &self.config.work_dir)?;
            let _ = kill_session(&session);
        }

        self.active_worktrees.remove(stage_id);

        Ok(())
    }

    fn on_session_crashed(
        &mut self,
        session_id: &str,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    ) -> Result<()> {
        // Check if we've already reported this crash to avoid duplicate messages
        if self.reported_crashes.contains(session_id) {
            return Ok(());
        }
        self.reported_crashes.insert(session_id.to_string());

        if let Some(sid) = stage_id {
            self.active_sessions.remove(&sid);

            let mut stage = self.load_stage(&sid)?;

            // Don't override terminal states - stage may have completed before tmux died
            if matches!(stage.status, StageStatus::Completed | StageStatus::Verified) {
                // Stage already completed successfully, just clean up
                return Ok(());
            }

            clear_status_line();
            eprintln!("Session '{session_id}' crashed for stage '{sid}'");

            if let Some(path) = crash_report_path {
                eprintln!("Crash report generated: {}", path.display());
                stage.close_reason = Some(format!(
                    "Session crashed - see crash report at {}",
                    path.display()
                ));
            } else {
                stage.close_reason = Some("Session crashed".to_string());
            }

            stage.status = StageStatus::Blocked;
            self.save_stage(&stage)?;

            self.graph.mark_blocked(&sid)?;
        } else {
            clear_status_line();
            eprintln!("Session '{session_id}' crashed (no stage association)");
            if let Some(path) = crash_report_path {
                eprintln!("Crash report generated: {}", path.display());
            }
        }

        Ok(())
    }

    fn on_needs_handoff(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        clear_status_line();
        eprintln!("Session '{session_id}' needs handoff for stage '{stage_id}'");

        let mut stage = self.load_stage(stage_id)?;
        stage.try_mark_needs_handoff()?;
        self.save_stage(&stage)?;

        Ok(())
    }
}
