//! Change detection for stages and sessions

use std::collections::{HashMap, HashSet};
use std::path::Path;

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
use super::heartbeat::{remove_heartbeat, HeartbeatStatus, HeartbeatWatcher};
use super::soft_signals;
use super::tool_analysis;

/// Remove the on-disk heartbeat file for a session's stage when the session
/// reaches a terminal status (crash/completion). Heartbeat files are keyed by
/// stage ID, so leaving a dead session's heartbeat behind lets it later flag a
/// fresh session that reuses the same stage as hung. Best-effort: a failure to
/// remove is logged but never blocks detection.
fn cleanup_heartbeat_for_session(work_dir: &Path, session: &Session) {
    if let Some(stage_id) = &session.stage_id {
        if let Err(e) = remove_heartbeat(work_dir, stage_id) {
            tracing::warn!(
                "Failed to remove heartbeat for stage '{}' (session '{}'): {}",
                stage_id,
                session.id,
                e
            );
        }
    }
}

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
    /// Track whether each session was already flagged as possibly stuck this
    /// daemon session (in-memory dedup; persistent dedup uses soft-signals.jsonl).
    pub last_stuck_detected: HashMap<String, bool>,
}

impl Detection {
    pub fn new() -> Self {
        Self {
            last_stage_states: HashMap::new(),
            last_session_states: HashMap::new(),
            last_context_levels: HashMap::new(),
            reported_hung_sessions: HashSet::new(),
            last_budget_exceeded: HashMap::new(),
            last_stuck_detected: HashMap::new(),
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

        // Read all active soft signals ONCE per tick (rather than re-parsing
        // soft-signals.jsonl per session below). The file is append-only, so a
        // single read here is cheaper than N reads in the stuck-detection loop.
        let stuck_now = std::time::SystemTime::now();
        let active_soft_signals =
            soft_signals::read_active(handlers.work_dir(), stuck_now).unwrap_or_default();

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

                                // Persist session status to file immediately
                                handlers.persist_session_status(session, SessionStatus::Completed);
                                cleanup_heartbeat_for_session(handlers.work_dir(), session);

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
                                if matches!(
                                    stage.status,
                                    StageStatus::Completed
                                        | StageStatus::MergeConflict
                                        | StageStatus::MergeBlocked
                                ) {
                                    // Stage completed successfully, treat as normal completion
                                    // Persist session status to file immediately
                                    handlers
                                        .persist_session_status(session, SessionStatus::Completed);
                                    cleanup_heartbeat_for_session(handlers.work_dir(), session);

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

                        // Persist session status to file immediately
                        handlers.persist_session_status(session, SessionStatus::Crashed);

                        // Remove the now-dead session's heartbeat so it can't
                        // later flag a fresh session reusing this stage as hung.
                        cleanup_heartbeat_for_session(handlers.work_dir(), session);

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

            // Stuck detection: only for running sessions that have a stage.
            // Runs every tick (like the budget check), but emits at most once
            // per DECAY_WINDOW_SECS window per session through two dedup layers:
            //   1. In-memory: last_stuck_detected HashMap
            //   2. Persistent: soft-signals.jsonl (survives daemon restarts)
            if session.status == SessionStatus::Running {
                if let Some(stage_id) = &session.stage_id {
                    let work_dir = handlers.work_dir();

                    let was_stuck = self
                        .last_stuck_detected
                        .get(&session.id)
                        .copied()
                        .unwrap_or(false);

                    // Reuse the per-tick active-signals snapshot rather than
                    // re-reading soft-signals.jsonl for every session.
                    let has_active_signal =
                        soft_signals::filter_for_session(&active_soft_signals, &session.id)
                            .iter()
                            .any(|s| matches!(s, soft_signals::SoftSignal::PossiblyStuck { .. }));

                    if !was_stuck && !has_active_signal {
                        match tool_analysis::analyze_session(work_dir, &session.id) {
                            Ok(analysis) if analysis.is_possibly_stuck() => {
                                let emitted_at = chrono::Utc::now().to_rfc3339();
                                let expires_at = (chrono::Utc::now()
                                    + chrono::Duration::seconds(
                                        soft_signals::DECAY_WINDOW_SECS as i64,
                                    ))
                                .to_rfc3339();
                                let sig = soft_signals::SoftSignal::PossiblyStuck {
                                    session_id: session.id.clone(),
                                    stage_id: stage_id.clone(),
                                    recent_events: analysis.recent_events,
                                    failure_count: analysis.recent_failure_count,
                                    failure_ratio: analysis.recent_failure_ratio,
                                    emitted_at,
                                    expires_at,
                                };
                                if let Err(e) = soft_signals::append(work_dir, &sig) {
                                    tracing::warn!("Failed to append soft signal: {}", e);
                                }
                                events.push(MonitorEvent::PossiblyStuck {
                                    session_id: session.id.clone(),
                                    stage_id: stage_id.clone(),
                                    recent_events: analysis.recent_events,
                                    failure_count: analysis.recent_failure_count,
                                    failure_ratio: analysis.recent_failure_ratio,
                                });
                                self.last_stuck_detected.insert(session.id.clone(), true);
                            }
                            Ok(_) => {
                                // Not stuck (or not enough data); reset in-memory flag.
                                self.last_stuck_detected.insert(session.id.clone(), false);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Tool analysis failed for session {}: {}",
                                    session.id,
                                    e
                                );
                            }
                        }
                    }
                }
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

            // Check heartbeat status for this stage. Pass the session ID so a
            // stale heartbeat left by a previous session for the same stage
            // does not flag this fresh session as hung (treated as NoHeartbeat).
            let heartbeat_status = heartbeat_watcher.check_session_hung(stage_id, &session.id);

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
