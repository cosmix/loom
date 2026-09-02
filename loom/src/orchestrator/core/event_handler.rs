//! Event handling - processing monitor events and session lifecycle

use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use std::path::PathBuf;

use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::MonitorEvent;

use super::clear_status_line;
use super::persistence::Persistence;
use super::Orchestrator;

mod handoff_state;
mod human_review;
mod recover_hung;
mod stage_takedown;
mod stalled_judge;

use handoff_state::mark_needs_handoff;
use human_review::announce_needs_human_review;
use recover_hung::HungReport;

fn requeue_after_handoff(stage: &mut Stage) -> Result<()> {
    stage.try_mark_queued()
}

/// Whether a backstop event still names the session its stage is executing.
///
/// A corpse's event must not take down its successor. `Detection`'s fire-once
/// latch is in-memory only, so a record left on disk at or past the ceiling
/// re-fires `BudgetExceeded` on the first tick after every daemon restart —
/// naming whatever session was active when the ceiling was first crossed,
/// which by then may no longer be the stage's agent. Without this check that
/// replayed event kills whatever session `active_sessions` holds for the stage
/// now, not the one it names.
fn event_targets_current_session(stage: &Stage, session_id: &str) -> bool {
    if stage.session.as_deref() == Some(session_id) {
        return true;
    }
    tracing::debug!(
        stage_id = %stage.id,
        exceeded_session = %session_id,
        active_session = ?stage.session,
        "Ignoring a handoff event from a session that is not the \
         stage's active session"
    );
    false
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

    /// Handle merge session completion
    fn on_merge_session_completed(&mut self, session_id: &str, stage_id: &str) -> Result<()>;

    /// Handle the daemon backstop firing (force handoff)
    fn on_budget_exceeded(
        &mut self,
        session_id: &str,
        stage_id: &str,
        context_tokens: u32,
        ceiling_tokens: u32,
    ) -> Result<()>;
}

impl EventHandler for Orchestrator {
    fn handle_events(&mut self, events: Vec<MonitorEvent>) -> Result<()> {
        // Apply fresh heartbeat facts before destructive session events from
        // the same poll. The monitor preserves its public event order, but a
        // fresh high reading must reach disk before BudgetExceeded writes the
        // handoff and terminalizes the record.
        let (heartbeat_events, other_events): (Vec<_>, Vec<_>) = events
            .into_iter()
            .partition(|event| matches!(event, MonitorEvent::HeartbeatReceived { .. }));
        for event in heartbeat_events.into_iter().chain(other_events) {
            // O-4: one failing event must not drop the rest of the batch or
            // kill the daemon. Each event is handled in isolation; a handler
            // error is logged and the loop moves on to the next event.
            if let Err(e) = self.handle_one_event(event) {
                clear_status_line();
                tracing::error!(error = %e, "Failed to handle monitor event; continuing with next event");
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
        if self.begin_handoff(stage_id, session_id)?.is_none() {
            return Ok(());
        }
        self.finish_handoff_and_requeue(stage_id, session_id, "handoff")
    }

    fn on_merge_session_completed(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        // Implementation in merge_handler.rs
        self.handle_merge_session_completed(session_id, stage_id)
    }

    fn on_budget_exceeded(
        &mut self,
        session_id: &str,
        stage_id: &str,
        context_tokens: u32,
        ceiling_tokens: u32,
    ) -> Result<()> {
        // Implementation in event_handler.rs
        self.handle_budget_exceeded(session_id, stage_id, context_tokens, ceiling_tokens)
    }
}

impl Orchestrator {
    /// Handle a single monitor event. Errors are isolated per event by the
    /// caller (`handle_events`) so one bad event cannot abort the batch or the
    /// daemon (O-4).
    fn handle_one_event(&mut self, event: MonitorEvent) -> Result<()> {
        match event {
            MonitorEvent::StageCompleted { stage_id } => {
                self.on_stage_completed(&stage_id)?;
            }
            MonitorEvent::StageBlocked { stage_id, reason } => {
                clear_status_line();
                eprintln!("Stage '{stage_id}' blocked: {reason}");
                self.graph.mark_status(&stage_id, StageStatus::Blocked)?;
            }
            MonitorEvent::SessionContextWarning {
                session_id,
                context_tokens,
                ceiling_tokens,
            } => {
                clear_status_line();
                eprintln!(
                    "Warning: Session '{session_id}' context at {context_tokens} \
                     of {ceiling_tokens} tokens"
                );
            }
            MonitorEvent::SessionContextCritical {
                session_id,
                context_tokens,
                ceiling_tokens,
            } => {
                clear_status_line();
                eprintln!(
                    "Critical: Session '{session_id}' context at {context_tokens} \
                     of {ceiling_tokens} tokens"
                );
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
            MonitorEvent::SessionHung {
                session_id,
                stage_id,
                stale_duration_secs,
                timeout_secs,
                last_activity,
                finished_without_completing,
            } => {
                // Advisory on the first report; a silence deep enough to be
                // evidence of a dead agent is recovered. Both live in
                // `recover_hung`, the wording in `monitor::parked`.
                self.on_session_hung(HungReport {
                    session_id: &session_id,
                    stage_id: stage_id.as_deref(),
                    stale_duration_secs,
                    timeout_secs,
                    last_activity: last_activity.as_deref(),
                    finished_without_completing,
                })?;
            }
            MonitorEvent::HeartbeatReceived {
                stage_id,
                session_id,
                context_tokens,
                transcript_path,
                last_tool: _,
            } => {
                self.apply_heartbeat(&stage_id, &session_id, context_tokens, transcript_path)?;
            }
            MonitorEvent::BudgetExceeded {
                session_id,
                stage_id,
                context_tokens,
                ceiling_tokens,
            } => {
                self.on_budget_exceeded(&session_id, &stage_id, context_tokens, ceiling_tokens)?;
            }
            MonitorEvent::AdjudicatorStalled {
                session_id,
                stage_id,
                stale_duration_secs,
                timeout_secs,
            } => {
                self.on_adjudicator_stalled(
                    &session_id,
                    &stage_id,
                    stale_duration_secs,
                    timeout_secs,
                )?;
            }
            MonitorEvent::StageNeedsHumanReview {
                stage_id,
                review_reason,
            } => announce_needs_human_review(&stage_id, review_reason.as_deref()),
        }
        Ok(())
    }
}

/// Helper to check if a stage is in the ready list of the graph
#[cfg(test)]
fn graph_has_ready_stage(graph: &crate::plan::ExecutionGraph, stage_id: &str) -> bool {
    graph.ready_stages().iter().any(|n| n.id == stage_id)
}

impl Orchestrator {
    /// Atomically verify that an asynchronous event still targets the stage's
    /// assigned session and move that stage to `NeedsHandoff`. Keeping the
    /// identity check inside the locked update closes the gap where a successor
    /// could be assigned between an earlier load and the destructive takedown.
    fn begin_handoff(&mut self, stage_id: &str, session_id: &str) -> Result<Option<Stage>> {
        let handoff_at = Utc::now();
        let mut is_current = false;
        let stage = self.update_stage(stage_id, |stage| {
            if !event_targets_current_session(stage, session_id)
                || !matches!(
                    stage.status,
                    StageStatus::Executing | StageStatus::NeedsHandoff
                )
            {
                return Ok(());
            }
            is_current = true;
            mark_needs_handoff(stage, handoff_at)
        })?;
        Ok(is_current.then_some(stage))
    }

    /// Complete a handoff whose identity was already locked and marked by
    /// [`Self::begin_handoff`]. The takedown is load-bearing: re-queue only
    /// after every prior writer is confirmed gone and its terminal record is
    /// durable. A survivor or any uncertainty leaves the stage visibly in
    /// `NeedsHandoff` instead of risking two agents in one worktree.
    fn finish_handoff_and_requeue(
        &mut self,
        stage_id: &str,
        session_id: &str,
        cause: &str,
    ) -> Result<()> {
        // This ends processes: `stage_takedown.rs` signals each of the stage's
        // agents through `kill_session` and returns only those still alive after.
        let survivors = self.take_down_stage_agents(stage_id, session_id)?;
        if !survivors.is_empty() {
            eprintln!(
                "Stage '{stage_id}' stays in NeedsHandoff after {cause}: session(s) {} are still \
                 alive after the kill attempt, so re-queueing would put a second agent in the \
                 same worktree. Take them down with \
                 'loom stage reset {stage_id} --kill-session'.",
                survivors.join(", ")
            );
            return Ok(());
        }

        // Re-queue the stage so the next poll cycle picks it up
        let mut still_current = false;
        self.update_stage(stage_id, |stage| {
            if !event_targets_current_session(stage, session_id)
                || stage.status != StageStatus::NeedsHandoff
            {
                return Ok(());
            }
            still_current = true;
            requeue_after_handoff(stage)
        })?;
        if !still_current {
            return Ok(());
        }
        self.graph.mark_queued(stage_id)?;

        eprintln!("Stage '{stage_id}' re-queued for continuation after {cause}");

        Ok(())
    }

    /// Handle the daemon's ceiling backstop firing for a session.
    ///
    /// The agent's own hook governs at 100% of the stage ceiling; this path
    /// only runs at `DAEMON_CEILING_MULTIPLIER` times that, i.e. when the agent
    /// ignored its own governance. So the session is not asked to stop — it is
    /// handed off and killed.
    pub(super) fn handle_budget_exceeded(
        &mut self,
        session_id: &str,
        stage_id: &str,
        context_tokens: u32,
        ceiling_tokens: u32,
    ) -> Result<()> {
        clear_status_line();
        eprintln!(
            "{} Session '{}' at {} tokens, past the daemon backstop for its {}-token ceiling",
            "CONTEXT CEILING EXCEEDED:".red().bold(),
            session_id,
            context_tokens,
            ceiling_tokens
        );

        let Some(stage) = self.begin_handoff(stage_id, session_id)? else {
            return Ok(());
        };

        self.retire_exceeded_session(session_id, &stage, context_tokens)?;
        self.finish_handoff_and_requeue(stage_id, session_id, "the context ceiling backstop")
    }
}

#[cfg(test)]
mod governor_retry_tests;
#[cfg(test)]
mod governor_tests;
#[cfg(test)]
mod recover_hung_tests;
#[cfg(test)]
mod stalled_judge_tests;
#[cfg(test)]
mod takedown_identity_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod verdict_retirement_tests;
