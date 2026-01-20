//! Change detection for stages and sessions

use std::collections::{HashMap, HashSet};

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};

use super::config::MonitorConfig;
use super::context::{context_health, context_usage_percent, ContextHealth};
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::heartbeat::{HeartbeatStatus, HeartbeatWatcher};

/// Detection state for tracking changes
pub struct Detection {
    pub last_stage_states: HashMap<String, StageStatus>,
    pub last_session_states: HashMap<String, SessionStatus>,
    pub last_context_levels: HashMap<String, ContextHealth>,
    /// Track sessions that have been reported as hung to avoid duplicate events
    pub reported_hung_sessions: HashSet<String>,
}

impl Detection {
    pub fn new() -> Self {
        Self {
            last_stage_states: HashMap::new(),
            last_session_states: HashMap::new(),
            last_context_levels: HashMap::new(),
            reported_hung_sessions: HashSet::new(),
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
                // Check if session is still alive (PID check)
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

                        // Check if the stage has already been marked as Completed
                        // This prevents false crash reports when the session exits normally
                        // after completing its work
                        if let Some(stage_id) = &session.stage_id {
                            if let Some(stage) = stages.iter().find(|s| &s.id == stage_id) {
                                if matches!(stage.status, StageStatus::Completed) {
                                    // Stage completed successfully, treat as normal completion
                                    self.last_session_states
                                        .insert(session.id.clone(), SessionStatus::Completed);
                                    continue;
                                }
                            }
                        }

                        // Regular session crashed - generate crash report
                        let reason = if session.pid.is_some() {
                            "Process no longer running"
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
                        // Auto-summarize memory at warning threshold (60%)
                        if let Err(e) = handlers.handle_context_warning(session) {
                            eprintln!(
                                "Failed to auto-summarize memory for session '{}': {}",
                                session.id, e
                            );
                        }

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

    /// Detect heartbeat-based events (heartbeat updates, hung sessions)
    pub fn detect_heartbeat_events(
        &mut self,
        sessions: &[Session],
        heartbeat_watcher: &mut HeartbeatWatcher,
        config: &MonitorConfig,
        handlers: &Handlers,
    ) -> Vec<MonitorEvent> {
        let mut events = Vec::new();

        // Poll heartbeat files for updates
        if let Ok(updates) = heartbeat_watcher.poll(&config.work_dir) {
            for update in updates {
                // Emit heartbeat received event
                events.push(MonitorEvent::HeartbeatReceived {
                    stage_id: update.heartbeat.stage_id.clone(),
                    session_id: update.heartbeat.session_id.clone(),
                    context_percent: update.heartbeat.context_percent,
                    last_tool: update.heartbeat.last_tool.clone(),
                });

                // If we previously reported this session as hung, clear that flag
                // since we got a fresh heartbeat
                self.reported_hung_sessions
                    .remove(&update.heartbeat.session_id);
            }
        }

        // Check each running session for hung status
        for session in sessions {
            if session.status != SessionStatus::Running {
                continue;
            }

            let stage_id = match &session.stage_id {
                Some(id) => id,
                None => continue,
            };

            // Check heartbeat status for this stage
            let heartbeat_status = heartbeat_watcher.check_session_hung(stage_id);

            match heartbeat_status {
                HeartbeatStatus::Hung {
                    stale_duration_secs,
                } => {
                    // Only report if we haven't already and the session is still alive
                    if !self.reported_hung_sessions.contains(&session.id) {
                        // Verify PID is still alive before declaring hung
                        // (if PID is dead, it's a crash not a hang)
                        if let Ok(Some(is_alive)) = handlers.check_session_alive(session) {
                            if is_alive {
                                // Session is alive but not sending heartbeats - it's hung
                                let last_activity = heartbeat_watcher
                                    .get_heartbeat(stage_id)
                                    .and_then(|hb| hb.activity.clone());

                                events.push(MonitorEvent::SessionHung {
                                    session_id: session.id.clone(),
                                    stage_id: Some(stage_id.clone()),
                                    stale_duration_secs,
                                    last_activity,
                                });

                                self.reported_hung_sessions.insert(session.id.clone());
                            }
                            // If not alive, the crash detection in detect_session_changes handles it
                        }
                    }
                }
                HeartbeatStatus::Healthy => {
                    // Session is healthy, clear any hung report
                    self.reported_hung_sessions.remove(&session.id);
                }
                HeartbeatStatus::NoHeartbeat => {
                    // No heartbeat yet - session may not have started heartbeat protocol
                    // This is normal for new sessions or sessions before hooks are set up
                }
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
