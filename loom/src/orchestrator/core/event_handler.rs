//! Event handling - processing monitor events and session lifecycle

use anyhow::Result;
use std::path::PathBuf;

use crate::orchestrator::monitor::MonitorEvent;

use super::clear_status_line;
use super::persistence::Persistence;
use super::Orchestrator;

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

    /// Handle merge session completion
    fn on_merge_session_completed(&mut self, session_id: &str, stage_id: &str) -> Result<()>;
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
                MonitorEvent::MergeSessionCompleted {
                    session_id,
                    stage_id,
                } => {
                    self.on_merge_session_completed(&session_id, &stage_id)?;
                }
                MonitorEvent::CheckpointCreated {
                    session_id,
                    task_id,
                    verification_passed,
                    warnings,
                    stage_complete,
                } => {
                    clear_status_line();
                    if !verification_passed && !warnings.is_empty() {
                        eprintln!(
                            "Checkpoint '{task_id}' (session {session_id}) created with {} warnings",
                            warnings.len()
                        );
                    } else if stage_complete {
                        eprintln!(
                            "Checkpoint '{task_id}' (session {session_id}) completed - all tasks done!"
                        );
                    } else {
                        eprintln!(
                            "Checkpoint '{task_id}' (session {session_id}) completed successfully"
                        );
                    }
                }
                MonitorEvent::SessionHung {
                    session_id,
                    stage_id,
                    stale_duration_secs,
                    last_activity,
                } => {
                    clear_status_line();
                    let stage_info = stage_id
                        .as_ref()
                        .map(|s| format!(" (stage '{s}')"))
                        .unwrap_or_default();
                    let activity_info = last_activity
                        .as_ref()
                        .map(|a| format!(", last: {a}"))
                        .unwrap_or_default();
                    eprintln!(
                        "Warning: Session '{session_id}'{stage_info} appears hung (no heartbeat for {stale_duration_secs}s{activity_info})"
                    );
                }
                MonitorEvent::HeartbeatReceived {
                    stage_id: _,
                    session_id: _,
                    context_percent: _,
                    last_tool: _,
                } => {
                    // Heartbeat events are silent - just used for internal tracking
                }
                MonitorEvent::RecoveryInitiated {
                    stage_id,
                    session_id,
                    recovery_type,
                } => {
                    clear_status_line();
                    eprintln!(
                        "Recovery initiated for stage '{stage_id}' (session '{session_id}', type: {recovery_type:?})"
                    );
                }
                MonitorEvent::StageEscalated {
                    stage_id,
                    failure_count,
                    reason,
                } => {
                    clear_status_line();
                    eprintln!(
                        "Stage '{stage_id}' escalated after {failure_count} failures: {reason}"
                    );
                }
                MonitorEvent::ContextRefreshNeeded {
                    stage_id,
                    session_id,
                    context_percent,
                } => {
                    clear_status_line();
                    eprintln!(
                        "Context refresh needed for stage '{stage_id}' (session '{session_id}', context at {context_percent:.1}%)"
                    );
                }
            }
        }
        Ok(())
    }

    fn on_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        // Implementation in completion_handler.rs
        self.handle_stage_completed(stage_id)
    }

    fn on_session_crashed(
        &mut self,
        session_id: &str,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    ) -> Result<()> {
        // Implementation in crash_handler.rs
        self.handle_session_crashed(session_id, stage_id, crash_report_path)
    }

    fn on_needs_handoff(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        clear_status_line();
        eprintln!("Session '{session_id}' needs handoff for stage '{stage_id}'");

        let mut stage = self.load_stage(stage_id)?;
        stage.try_mark_needs_handoff()?;
        self.save_stage(&stage)?;

        Ok(())
    }

    fn on_merge_session_completed(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        // Implementation in merge_handler.rs
        self.handle_merge_session_completed(session_id, stage_id)
    }
}
