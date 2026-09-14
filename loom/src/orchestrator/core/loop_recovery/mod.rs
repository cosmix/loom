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
            eprintln!(
                "Stage '{stage_id}' stays in NeedsHandoff after {cause}: session(s) {} are still \
                 alive after the kill attempt, so re-queueing would put a second agent in the \
                 same worktree. Take them down with \
                 'loom stage reset {stage_id} --kill-session'.",
                survivors.join(", ")
            );
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
}
