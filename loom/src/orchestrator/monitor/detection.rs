//! Change detection for stages and sessions

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::fs::work_dir::ContextConfig;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
// `check_session_alive` below routes through the `LivenessService`
// attached to the monitor's handlers. Imported for documentation and
// to make the wiring discoverable via grep.
#[allow(unused_imports)]
use crate::orchestrator::liveness::LivenessService;

use super::ceiling::resolve_ceiling_tokens;
use super::config::MonitorConfig;
use super::context::{context_health, ContextHealth};
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::handoff_watch::HandoffWatch;
use super::heartbeat::{HeartbeatStatus, HeartbeatWatcher};
use super::hung_latch::hung_event;

/// Detection state for tracking changes
pub struct Detection {
    pub last_stage_states: HashMap<String, StageStatus>,
    pub last_session_states: HashMap<String, SessionStatus>,
    pub last_context_levels: HashMap<String, ContextHealth>,
    /// Track sessions that have been reported as hung to avoid duplicate events
    pub reported_hung_sessions: HashSet<String>,
    /// Sessions whose silence has already been reported a second time, at the
    /// escalation line. See [`super::hung_latch`].
    pub(super) escalated_hung_sessions: HashSet<String>,
    /// Red handoffs successfully written or found during this daemon run.
    /// A failed write remains absent so an unchanged-Red poll retries it.
    pub red_handoff_ready: HashSet<String>,
    /// Current running assignments whose ceiling backstop is currently
    /// exceeded. The latch lets budget-owned retries suppress the generic
    /// handoff event while that exact session still owns the stage.
    pub last_budget_exceeded: HashMap<String, bool>,
    /// Handoff documents asking for a takedown their sandboxed author could
    /// not record in the stage file. See [`super::handoff_watch`].
    handoff_watch: HandoffWatch,
}

impl Detection {
    pub fn new() -> Self {
        Self {
            last_stage_states: HashMap::new(),
            last_session_states: HashMap::new(),
            last_context_levels: HashMap::new(),
            reported_hung_sessions: HashSet::new(),
            escalated_hung_sessions: HashSet::new(),
            red_handoff_ready: HashSet::new(),
            last_budget_exceeded: HashMap::new(),
            handoff_watch: HandoffWatch::default(),
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
                    StageStatus::NeedsHandoff => {}
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
            events.extend(self.needs_handoff_event(stage));
        }

        events
    }

    /// Detect session status changes, context bands, and backstop crossings.
    pub fn detect_session_changes(
        &mut self,
        sessions: &[Session],
        stages: &[Stage],
        handlers: &Handlers,
    ) -> Vec<MonitorEvent> {
        let mut events = Vec::new();

        self.clear_inactive_latches(sessions);

        for session in sessions {
            let status = self.detect_session_status(session, stages, handlers);
            events.extend(status.events);
            if !self.judgeable(session, stages, status.terminal) {
                continue;
            }
            // A ceiling handoff whose stage transition the sandbox refused
            // leaves the request in the document alone. Checked before the
            // ceiling resolves: this recovery is about a stopped agent, not
            // about how much context it had.
            if let Some(event) =
                self.handoff_watch
                    .needs_handoff_from_document(session, stages, handlers.work_dir())
            {
                events.push(event);
            }

            // A ceiling this snapshot cannot vouch for is no ceiling at all.
            let Some(ceiling) = resolve_ceiling_tokens(session, stages, handlers) else {
                continue;
            };

            events.extend(self.detect_context_health(session, stages, handlers, ceiling));
            if let Some(event) =
                self.detect_backstop_crossing(session, stages, ceiling, handlers.context_config())
            {
                events.push(event);
            }
        }

        events
    }

    /// Emit Yellow/Red band transitions for one session, and generate a handoff
    /// the first time it enters Red. Gated on a band CHANGE so a session parked
    /// in one band does not re-emit every tick.
    fn detect_context_health(
        &mut self,
        session: &Session,
        stages: &[Stage],
        handlers: &Handlers,
        ceiling: u32,
    ) -> Vec<MonitorEvent> {
        let current = context_health(session.context_tokens, ceiling);
        let previous = self.last_context_levels.get(&session.id).copied();
        if previous == Some(current) {
            if current == ContextHealth::Red && !self.red_handoff_ready.contains(&session.id) {
                self.record_red_handoff_ready(session, stages, handlers, true);
            }
            return Vec::new();
        }
        self.last_context_levels.insert(session.id.clone(), current);
        if current != ContextHealth::Red {
            self.red_handoff_ready.remove(&session.id);
        }
        let mut events = Vec::new();
        match current {
            ContextHealth::Yellow => {
                if let Err(e) = handlers.handle_context_warning(session) {
                    eprintln!(
                        "Failed to auto-summarize memory for session '{}': {}",
                        session.id, e
                    );
                }
                events.push(MonitorEvent::SessionContextWarning {
                    session_id: session.id.clone(),
                    context_tokens: session.context_tokens,
                    ceiling_tokens: ceiling,
                });
            }
            ContextHealth::Red => {
                events.push(MonitorEvent::SessionContextCritical {
                    session_id: session.id.clone(),
                    context_tokens: session.context_tokens,
                    ceiling_tokens: ceiling,
                });

                // A cold-start Red observation may reuse the durable advisory
                // from before a daemon restart. A known Green/Yellow -> Red
                // transition is a new crossing and deserves a fresh snapshot.
                self.record_red_handoff_ready(session, stages, handlers, previous.is_none());
            }
            ContextHealth::Green => {}
        }
        events
    }

    /// The daemon's backstop: retry while the current assignment remains past
    /// [`ContextConfig::backstop_tokens`] for its stage ceiling.
    ///
    /// The agent's own hook governs at 100% of the ceiling, so reaching 125%
    /// means that governance was ignored and the daemon must take the session
    /// down itself. Deliberately NOT gated on a band change — a session is
    /// already Red at 90%, so no further transition would ever come.
    ///
    /// The multiplier is NOT applied here. At the built-in 800,000 ceiling it
    /// alone puts the backstop at the whole 1,000,000-token model window — a
    /// reading no session survives to produce, which would leave this last
    /// resort permanently unarmed. `backstop_tokens` clamps it to a fraction
    /// of the window that a session can actually reach.
    fn detect_backstop_crossing(
        &mut self,
        session: &Session,
        stages: &[Stage],
        ceiling: u32,
        context: &ContextConfig,
    ) -> Option<MonitorEvent> {
        let stage_id = session.stage_id.clone()?;
        let backstop = context.backstop_tokens(ceiling);

        let is_exceeded = ceiling > 0 && session.context_tokens > backstop;
        let current_assignment = stages.iter().any(|stage| {
            stage.id == stage_id
                && stage.session.as_deref() == Some(session.id.as_str())
                && matches!(
                    stage.status,
                    StageStatus::Executing | StageStatus::NeedsHandoff
                )
        });
        if is_exceeded && current_assignment {
            self.last_budget_exceeded.insert(session.id.clone(), true);
        } else {
            self.last_budget_exceeded.remove(&session.id);
        }

        (is_exceeded && current_assignment).then(|| MonitorEvent::BudgetExceeded {
            session_id: session.id.clone(),
            stage_id,
            context_tokens: session.context_tokens,
            ceiling_tokens: ceiling,
        })
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
                    context_tokens: update.heartbeat.context_tokens,
                    transcript_path: update.heartbeat.transcript_path.clone(),
                    last_tool: update.heartbeat.last_tool.clone(),
                });

                // If we previously reported this session as hung, clear that flag
                // since we got a fresh heartbeat
                self.clear_hung_report(&update.heartbeat.session_id);
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
                    // The first silence past the budget is reported, then one
                    // more at the escalation line; `hung_latch` owns that
                    // decision. A dead PID is a crash, and crash detection in
                    // `detect_session_changes` owns it, so only a live one is
                    // reported hung.
                    if self.hung_report_due(&session.id, stale_duration_secs, timeout_secs)
                        && matches!(handlers.check_session_alive(session), Ok(Some(true)))
                    {
                        events.push(hung_event(
                            session,
                            stage_id,
                            stages,
                            heartbeat_watcher,
                            stale_duration_secs,
                            timeout_secs,
                        ));
                        self.record_hung_report(&session.id, stale_duration_secs, timeout_secs);
                    }
                }
                HeartbeatStatus::Healthy => {
                    // Session is healthy, clear any hung report
                    self.clear_hung_report(&session.id);
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
