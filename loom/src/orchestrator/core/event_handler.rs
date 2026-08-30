//! Event handling - processing monitor events and session lifecycle

use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::path::PathBuf;

use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::parked::hung_warning;
use crate::orchestrator::monitor::MonitorEvent;

use super::clear_status_line;
use super::persistence::Persistence;
use super::Orchestrator;

mod stage_takedown;

fn mark_needs_handoff(stage: &mut Stage, now: DateTime<Utc>) -> Result<()> {
    stage.accumulate_attempt_time(now);
    stage.try_mark_needs_handoff()
}

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
        "Ignoring a budget-exceeded event from a session that is not the \
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
        for event in events {
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
        self.hand_off_and_requeue(stage_id, "handoff")
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
                clear_status_line();
                // ADVISORY ONLY: nothing is killed and nothing is retried. The
                // wording, and the parked/stuck split, live in monitor::parked.
                let warning = hung_warning(
                    &session_id,
                    stage_id.as_deref(),
                    stale_duration_secs,
                    timeout_secs,
                    last_activity.as_deref(),
                    finished_without_completing,
                );
                eprintln!("{warning}");
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
            MonitorEvent::StageNeedsHumanReview {
                stage_id,
                review_reason,
            } => {
                clear_status_line();
                let reason_str = review_reason.as_deref().unwrap_or("No reason provided");
                eprintln!(
                    "{} Stage '{}' needs human review: {}",
                    "REVIEW NEEDED:".magenta().bold(),
                    stage_id,
                    reason_str
                );
                crate::orchestrator::notify::notify_needs_human_review(
                    &stage_id,
                    review_reason.as_deref(),
                );
            }
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
    /// Take a live session off its stage and queue the stage for a successor.
    ///
    /// Marks the stage `NeedsHandoff`, takes the agent down, drops its signal
    /// file and re-queues. The takedown is the load-bearing step: re-queueing
    /// puts the stage back on the ready list, so an agent that is still writing
    /// the worktree would get a second agent spawned on top of it at the next
    /// poll. The stage is therefore re-queued ONLY once nothing is left running
    /// for it. When something survives, the stage stays `NeedsHandoff`: that is
    /// visible in `loom status` and recoverable by hand, where two agents in one
    /// worktree are silent corruption.
    fn hand_off_and_requeue(&mut self, stage_id: &str, cause: &str) -> Result<()> {
        let handoff_at = Utc::now();
        self.update_stage(stage_id, |stage| mark_needs_handoff(stage, handoff_at))?;

        // This ends processes: `stage_takedown.rs` signals each of the stage's
        // agents through `kill_session` and returns only those still alive after.
        let survivors = self.take_down_stage_agents(stage_id);
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
        self.update_stage(stage_id, requeue_after_handoff)?;
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

        let stage = self.load_stage(stage_id)?;

        if !event_targets_current_session(&stage, session_id) {
            return Ok(());
        }

        self.retire_exceeded_session(session_id, &stage)?;
        self.hand_off_and_requeue(stage_id, "the context ceiling backstop")
    }

    /// Write the outgoing agent's handoff and mark its record spent, before the
    /// takedown takes it away.
    ///
    /// Does nothing unless the daemon's entry for the stage IS the session the
    /// event named: `active_sessions` is keyed by stage, so the entry can
    /// belong to a successor that has no business being handed off or marked.
    fn retire_exceeded_session(&mut self, session_id: &str, stage: &Stage) -> Result<()> {
        let Some(session) = self
            .active_sessions
            .get(&stage.id)
            .filter(|s| s.id == session_id)
            .cloned()
        else {
            return Ok(());
        };

        // Generate the handoff BEFORE the kill, while the session record still
        // describes a running agent.
        let handoff_path = self
            .monitor
            .handlers()
            .handle_context_critical(&session, stage)?;
        eprintln!("Generated handoff at: {}", handoff_path.display());

        if let Some(session_mut) = self.active_sessions.get_mut(&stage.id) {
            session_mut.try_mark_context_exhausted()?;
            let session_to_save = session_mut.clone();
            // session_mut goes out of scope here, ending the mutable borrow
            self.save_session(&session_to_save)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod governor_tests;
#[cfg(test)]
mod tests;
