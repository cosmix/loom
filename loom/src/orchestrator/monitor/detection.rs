//! Change detection for stages and sessions

use std::collections::HashMap;

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};

use super::context::{context_health, context_usage_percent, ContextHealth};
use super::events::MonitorEvent;
use super::handlers::Handlers;

/// Detection state for tracking changes
pub struct Detection {
    pub last_stage_states: HashMap<String, StageStatus>,
    pub last_session_states: HashMap<String, SessionStatus>,
    pub last_context_levels: HashMap<String, ContextHealth>,
}

impl Detection {
    pub fn new() -> Self {
        Self {
            last_stage_states: HashMap::new(),
            last_session_states: HashMap::new(),
            last_context_levels: HashMap::new(),
        }
    }

    /// Detect stage status changes
    pub fn detect_stage_changes(&mut self, stages: &[Stage]) -> Vec<MonitorEvent> {
        let mut events = Vec::new();

        for stage in stages {
            let previous_status = self.last_stage_states.get(&stage.id);
            let current_status = &stage.status;

            if previous_status != Some(current_status) {
                match current_status {
                    StageStatus::Completed => {
                        events.push(MonitorEvent::StageCompleted {
                            stage_id: stage.id.clone(),
                        });
                    }
                    StageStatus::Blocked => {
                        events.push(MonitorEvent::StageBlocked {
                            stage_id: stage.id.clone(),
                            reason: stage
                                .close_reason
                                .clone()
                                .unwrap_or_else(|| "Unknown reason".to_string()),
                        });
                    }
                    StageStatus::NeedsHandoff => {
                        if let Some(session_id) = &stage.session {
                            events.push(MonitorEvent::SessionNeedsHandoff {
                                session_id: session_id.clone(),
                                stage_id: stage.id.clone(),
                            });
                        }
                    }
                    StageStatus::WaitingForInput => {
                        events.push(MonitorEvent::StageWaitingForInput {
                            stage_id: stage.id.clone(),
                            session_id: stage.session.clone(),
                        });
                    }
                    _ => {}
                }

                // Check for transition FROM WaitingForInput TO Executing
                if previous_status == Some(&StageStatus::WaitingForInput)
                    && current_status == &StageStatus::Executing
                {
                    events.push(MonitorEvent::StageResumedExecution {
                        stage_id: stage.id.clone(),
                    });
                }

                self.last_stage_states
                    .insert(stage.id.clone(), current_status.clone());
            }
        }

        events
    }

    /// Detect session status changes and context levels
    pub fn detect_session_changes(
        &mut self,
        sessions: &[Session],
        stages: &[Stage],
        handlers: &Handlers,
    ) -> Vec<MonitorEvent> {
        let mut events = Vec::new();

        for session in sessions {
            let previous_status = self.last_session_states.get(&session.id);
            let current_status = &session.status;

            let current_context_health =
                context_health(session.context_tokens, session.context_limit);
            let previous_context_health = self.last_context_levels.get(&session.id);

            if previous_status == Some(&SessionStatus::Running)
                && current_status == &SessionStatus::Running
            {
                // Check if session is still alive (PID or tmux)
                if let Ok(Some(is_alive)) = handlers.check_session_alive(session) {
                    if !is_alive {
                        // Check if this is a merge session that completed
                        if handlers.is_merge_session(&session.id) {
                            // Merge session completed - emit completion event
                            if let Some(stage_id) = &session.stage_id {
                                events.push(MonitorEvent::MergeSessionCompleted {
                                    session_id: session.id.clone(),
                                    stage_id: stage_id.clone(),
                                });

                                self.last_session_states
                                    .insert(session.id.clone(), SessionStatus::Completed);
                                continue;
                            }
                        }

                        // Regular session crashed - generate crash report
                        let reason = if session.pid.is_some() {
                            "Process no longer running"
                        } else if session.tmux_session.is_some() {
                            "Tmux session no longer running"
                        } else {
                            "Session no longer running"
                        };

                        let crash_report_path = handlers.handle_session_crash(session, reason);

                        events.push(MonitorEvent::SessionCrashed {
                            session_id: session.id.clone(),
                            stage_id: session.stage_id.clone(),
                            crash_report_path,
                        });

                        self.last_session_states
                            .insert(session.id.clone(), SessionStatus::Crashed);
                        continue;
                    }
                }
                // If check_session_alive returns Ok(None), the session has no trackable
                // process, so we skip liveness checking
            }

            if previous_status != Some(current_status) {
                // Check for session status transitions
                if current_status == &SessionStatus::Completed {
                    // Check if this is a merge session that completed
                    if handlers.is_merge_session(&session.id) {
                        if let Some(stage_id) = &session.stage_id {
                            events.push(MonitorEvent::MergeSessionCompleted {
                                session_id: session.id.clone(),
                                stage_id: stage_id.clone(),
                            });
                        }
                    }
                } else if current_status == &SessionStatus::Crashed {
                    // Generate crash report
                    let crash_report_path =
                        handlers.handle_session_crash(session, "Session marked as crashed");

                    events.push(MonitorEvent::SessionCrashed {
                        session_id: session.id.clone(),
                        stage_id: session.stage_id.clone(),
                        crash_report_path,
                    });
                }

                self.last_session_states
                    .insert(session.id.clone(), current_status.clone());
            }

            if previous_context_health != Some(&current_context_health) {
                match current_context_health {
                    ContextHealth::Yellow => {
                        events.push(MonitorEvent::SessionContextWarning {
                            session_id: session.id.clone(),
                            usage_percent: context_usage_percent(
                                session.context_tokens,
                                session.context_limit,
                            ),
                        });
                    }
                    ContextHealth::Red => {
                        let usage_percent =
                            context_usage_percent(session.context_tokens, session.context_limit);

                        events.push(MonitorEvent::SessionContextCritical {
                            session_id: session.id.clone(),
                            usage_percent,
                        });

                        // Generate handoff file if session has an associated stage
                        if let Some(stage_id) = &session.stage_id {
                            if let Some(stage) = stages.iter().find(|s| &s.id == stage_id) {
                                if let Ok(handoff_path) =
                                    handlers.handle_context_critical(session, stage)
                                {
                                    eprintln!(
                                        "Generated handoff for session {} at {}",
                                        session.id,
                                        handoff_path.display()
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }

                self.last_context_levels
                    .insert(session.id.clone(), current_context_health);
            }
        }

        events
    }
}

impl Default for Detection {
    fn default() -> Self {
        Self::new()
    }
}
