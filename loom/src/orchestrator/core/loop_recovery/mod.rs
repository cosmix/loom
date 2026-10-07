mod park;
mod resume;

#[cfg(test)]
mod park_disposition_tests;
#[cfg(test)]
mod park_tests;

use anyhow::Result;

use crate::models::session::SessionExitReason;
use crate::models::stage::StageStatus;
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::Orchestrator;
use crate::orchestrator::monitor::MonitorEvent;

use super::{clear_status_line, event_targets_current_session, requeue_after_handoff};

pub(super) use park::{
    exhausted_reason, never_worked_reason, not_logged_in_reason, review_headline, with_pane_notes,
};

impl Orchestrator {
    pub(super) fn handle_loop_recovery_event(&mut self, event: MonitorEvent) -> Result<()> {
        match event {
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
                Ok(())
            }
            MonitorEvent::StageResumedExecution { stage_id } => {
                clear_status_line();
                eprintln!("Stage '{stage_id}' resumed execution after user input");
                Ok(())
            }
            MonitorEvent::CompletionPending {
                stage_id,
                session_id,
                fingerprint,
                repeat_count,
            } => {
                self.on_completion_blocker(&stage_id, &session_id, &fingerprint, repeat_count, None)
            }
            MonitorEvent::CompletionBlocked {
                stage_id,
                session_id,
                fingerprint,
                repeat_count,
                escalation,
            } => self.on_completion_blocker(
                &stage_id,
                &session_id,
                &fingerprint,
                repeat_count,
                Some(escalation),
            ),
            _ => unreachable!("non-recovery event sent to recovery dispatcher"),
        }
    }

    pub(super) fn finish_handoff_and_requeue(
        &mut self,
        stage_id: &str,
        session_id: &str,
        cause: &str,
        reason: SessionExitReason,
    ) -> Result<()> {
        let survivors = self.take_down_stage_agents(stage_id, session_id, reason)?;
        if !survivors.is_empty() {
            warn_survivors(stage_id, cause, &survivors);
            return Ok(());
        }

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

    /// Take down the agents of a stage `begin_handoff` latched, then park it in
    /// `NeedsHumanReview` with `review_reason` for an operator.
    ///
    /// Parked only once every agent is confirmed gone: approving the review
    /// re-queues the stage, which must not admit a second writer. The session
    /// is released so the approval finds no live worker to refuse on. The
    /// status reason is the short constant because it is logged at ERROR. The
    /// review reason carries agent-controlled pane lines and is stored only in
    /// the stage record: the transition announcement prints and notifies the
    /// reason's one-line [`review_headline`], which stops before the pane text.
    pub(super) fn finish_handoff_and_park(
        &mut self,
        stage_id: &str,
        session_id: &str,
        review_reason: String,
        reason: SessionExitReason,
    ) -> Result<()> {
        let survivors = self.take_down_stage_agents(stage_id, session_id, reason)?;
        if !survivors.is_empty() {
            warn_survivors(stage_id, "a stall park", &survivors);
            return Ok(());
        }

        let mut parked = false;
        self.update_stage(stage_id, |stage| {
            if !event_targets_current_session(stage, session_id)
                || stage.status != StageStatus::NeedsHandoff
            {
                return Ok(());
            }
            parked = true;
            stage.force_status_with_reason(StageStatus::NeedsHumanReview, "stall park");
            stage.review_reason = Some(review_reason);
            stage.release_session();
            Ok(())
        })?;
        if !parked {
            return Ok(());
        }
        self.graph
            .mark_status(stage_id, StageStatus::NeedsHumanReview)?;
        eprintln!("Stage '{stage_id}' parked for human review");
        Ok(())
    }
}

/// Why a stage stays in `NeedsHandoff`: an agent outlived its kill.
fn warn_survivors(stage_id: &str, cause: &str, survivors: &[String]) {
    eprintln!(
        "Stage '{stage_id}' stays in NeedsHandoff after {cause}: session(s) {} are still \
         alive after the kill attempt, so re-queueing would put a second agent in the \
         same worktree. Take them down with \
         'loom stage reset {stage_id} --kill-session'.",
        survivors.join(", ")
    );
}
