//! Event handling - processing monitor events and session lifecycle

use anyhow::Result;

use crate::models::stage::StageStatus;
use crate::orchestrator::monitor::MonitorEvent;
use crate::orchestrator::signals::remove_signal;
use crate::orchestrator::spawner::kill_session;

use super::persistence::Persistence;
use super::Orchestrator;

/// Trait for handling monitor events
pub(super) trait EventHandler: Persistence {
    /// Handle monitor events
    fn handle_events(&mut self, events: Vec<MonitorEvent>) -> Result<()>;

    /// Handle stage completion
    fn on_stage_completed(&mut self, stage_id: &str) -> Result<()>;

    /// Handle session crash
    fn on_session_crashed(&mut self, session_id: &str, stage_id: Option<String>) -> Result<()>;

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
                    eprintln!("Stage '{stage_id}' blocked: {reason}");
                    self.graph.mark_blocked(&stage_id)?;
                }
                MonitorEvent::SessionContextWarning {
                    session_id,
                    usage_percent,
                } => {
                    eprintln!("Warning: Session '{session_id}' context at {usage_percent:.1}%");
                }
                MonitorEvent::SessionContextCritical {
                    session_id,
                    usage_percent,
                } => {
                    eprintln!("Critical: Session '{session_id}' context at {usage_percent:.1}%");
                }
                MonitorEvent::SessionCrashed {
                    session_id,
                    stage_id,
                } => {
                    self.on_session_crashed(&session_id, stage_id)?;
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
                    if let Some(sid) = session_id {
                        eprintln!("Stage '{stage_id}' (session '{sid}') is waiting for user input");
                    } else {
                        eprintln!("Stage '{stage_id}' is waiting for user input");
                    }
                }
                MonitorEvent::StageResumedExecution { stage_id } => {
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

    fn on_session_crashed(&mut self, session_id: &str, stage_id: Option<String>) -> Result<()> {
        if let Some(sid) = stage_id {
            self.active_sessions.remove(&sid);

            let mut stage = self.load_stage(&sid)?;

            // Don't override terminal states - stage may have completed before tmux died
            if matches!(stage.status, StageStatus::Completed | StageStatus::Verified) {
                // Stage already completed successfully, just clean up
                return Ok(());
            }

            eprintln!("Session '{session_id}' crashed for stage '{sid}'");
            stage.status = StageStatus::Blocked;
            stage.close_reason = Some("Session crashed".to_string());
            self.save_stage(&stage)?;

            self.graph.mark_blocked(&sid)?;
        } else {
            eprintln!("Session '{session_id}' crashed (no stage association)");
        }

        Ok(())
    }

    fn on_needs_handoff(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        eprintln!("Session '{session_id}' needs handoff for stage '{stage_id}'");

        let mut stage = self.load_stage(stage_id)?;
        stage.try_mark_needs_handoff()?;
        self.save_stage(&stage)?;

        Ok(())
    }
}
