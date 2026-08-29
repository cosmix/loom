//! Change detection for stages and sessions

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::models::constants::DEFAULT_CONTEXT_BUDGET;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
// `check_session_alive` below routes through the `LivenessService`
// attached to the monitor's handlers. Imported for documentation and
// to make the wiring discoverable via grep.
#[allow(unused_imports)]
use crate::orchestrator::liveness::LivenessService;

use super::config::MonitorConfig;
use super::context::{context_health, context_usage_percent, ContextHealth};
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::heartbeat::{HeartbeatStatus, HeartbeatWatcher};
use super::parked::stage_looks_finished;

/// Detection state for tracking changes
pub struct Detection {
    pub last_stage_states: HashMap<String, StageStatus>,
    pub last_session_states: HashMap<String, SessionStatus>,
    pub last_context_levels: HashMap<String, ContextHealth>,
    /// Track sessions that have been reported as hung to avoid duplicate events
    pub reported_hung_sessions: HashSet<String>,
    /// Track whether each session's budget was exceeded on the previous tick,
    /// so BudgetExceeded is emitted only on the first crossing (not every tick).
    pub last_budget_exceeded: HashMap<String, bool>,
}

impl Detection {
    pub fn new() -> Self {
        Self {
            last_stage_states: HashMap::new(),
            last_session_states: HashMap::new(),
            last_context_levels: HashMap::new(),
            reported_hung_sessions: HashSet::new(),
            last_budget_exceeded: HashMap::new(),
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
                    StageStatus::NeedsHumanReview => {
                        events.push(MonitorEvent::StageNeedsHumanReview {
                            stage_id: stage.id.clone(),
                            review_reason: stage.review_reason.clone(),
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
            let status = self.detect_session_status(session, stages, handlers);
            events.extend(status.events);
            if status.terminal {
                continue;
            }

            let current_context_health =
                context_health(session.context_tokens, session.context_limit);
            let previous_context_health = self.last_context_levels.get(&session.id).copied();

            if previous_context_health != Some(current_context_health) {
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

            // Budget check runs every tick (independent of coarse health-bucket changes).
            // A stage with a per-stage budget (e.g. 70%) can be exceeded while the
            // session stays in the same coarse bucket (e.g. Red = 65%+), so we must
            // not gate this check on a bucket transition.  We emit BudgetExceeded only
            // on the first tick where usage crosses the threshold to avoid flooding.
            if let Some(stage_id) = &session.stage_id {
                if let Some(stage) = stages.iter().find(|s| &s.id == stage_id) {
                    let budget_percent = stage
                        .context_budget
                        .unwrap_or(DEFAULT_CONTEXT_BUDGET as u32)
                        as f32;
                    let usage_percent =
                        context_usage_percent(session.context_tokens, session.context_limit);

                    let was_exceeded = self
                        .last_budget_exceeded
                        .get(&session.id)
                        .copied()
                        .unwrap_or(false);
                    let is_exceeded = usage_percent > budget_percent;

                    if is_exceeded && !was_exceeded {
                        events.push(MonitorEvent::BudgetExceeded {
                            session_id: session.id.clone(),
                            stage_id: stage_id.clone(),
                            usage_percent,
                            budget_percent,
                        });
                    }

                    self.last_budget_exceeded
                        .insert(session.id.clone(), is_exceeded);
                }
            }
        }

        events
    }

    /// Detect heartbeat-based events (heartbeat updates, silent sessions).
    ///
    /// The silence check is deterministic: it compares the poll loop's own tick
    /// against the timestamp recorded in the heartbeat file. Nothing sleeps here
    /// and nothing shells out, so the check cannot itself wedge the loop.
    ///
    /// `stages` supplies the per-stage response budget
    /// ([`Stage::effective_subagent_timeout_secs`]); a session whose stage is not
    /// in the list falls back to the monitor-wide default.
    ///
    /// [`Stage::effective_subagent_timeout_secs`]: crate::models::stage::Stage::effective_subagent_timeout_secs
    pub fn detect_heartbeat_events(
        &mut self,
        sessions: &[Session],
        stages: &[Stage],
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

            // Resolve this stage's response budget. One watcher serves every
            // stage, so the threshold is passed per check rather than held on
            // the watcher.
            let timeout_secs = stages
                .iter()
                .find(|s| s.id == *stage_id)
                .map(|s| s.effective_subagent_timeout_secs())
                .unwrap_or_else(|| config.hung_timeout.as_secs());

            // Check heartbeat status for this stage. Pass the session ID so a
            // stale heartbeat left by a previous session for the same stage
            // does not flag this fresh session as hung (treated as NoHeartbeat).
            let heartbeat_status = heartbeat_watcher.check_session_hung(
                stage_id,
                &session.id,
                Duration::from_secs(timeout_secs),
            );

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
                                events.push(hung_event(
                                    session,
                                    stage_id,
                                    stages,
                                    heartbeat_watcher,
                                    stale_duration_secs,
                                    timeout_secs,
                                ));

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

/// Build the `SessionHung` event for a session whose heartbeat has gone stale.
///
/// Split out of [`Detection::detect_heartbeat_events`] so the classification
/// has somewhere to live without growing that function, and so the two git
/// probes behind [`stage_looks_finished`] are visibly paid once per hung
/// report rather than once per poll.
fn hung_event(
    session: &Session,
    stage_id: &str,
    stages: &[Stage],
    heartbeat_watcher: &HeartbeatWatcher,
    stale_duration_secs: u64,
    timeout_secs: u64,
) -> MonitorEvent {
    let last_activity = heartbeat_watcher
        .get_heartbeat(stage_id)
        .and_then(|hb| hb.activity.clone());

    let finished_without_completing = stages
        .iter()
        .find(|s| s.id == stage_id)
        .is_some_and(|stage| stage_looks_finished(session, stage));

    MonitorEvent::SessionHung {
        session_id: session.id.clone(),
        stage_id: Some(stage_id.to_string()),
        stale_duration_secs,
        timeout_secs,
        last_activity,
        finished_without_completing,
    }
}
