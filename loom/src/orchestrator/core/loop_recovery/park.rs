use anyhow::{Context, Result};
use chrono::Utc;

use crate::fs::session_files::{load_session_exact, mark_session_terminal_reason};
use crate::handoff::{current_blocker, load_trusted_session_checkpoint, short_fingerprint};
use crate::models::session::{Session, SessionExitReason, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::events::CompletionEscalation;
use crate::orchestrator::signals::remove_signal;
use crate::orchestrator::terminal::native::{session_process_status, SessionProcessStatus};
use crate::subagent_lifecycle::store::{replay, ChildDisposition};

use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::Orchestrator;

impl Orchestrator {
    pub(crate) fn on_completion_blocker(
        &mut self,
        stage_id: &str,
        session_id: &str,
        fingerprint: &str,
        repeat_count: u32,
        escalation: Option<CompletionEscalation>,
    ) -> Result<()> {
        let Some(failure_code) = self.current_completion_failure(
            stage_id,
            session_id,
            fingerprint,
            escalation.as_ref(),
        )?
        else {
            return Ok(());
        };
        if escalation.is_none() {
            eprintln!(
                "Completion blocked for stage '{stage_id}' ({}; verified attempt {repeat_count})",
                short_fingerprint(fingerprint)
            );
            return Ok(());
        }
        match replay(&self.config.work_dir)
            .map(|index| index.session_child_disposition(stage_id, session_id))
            .unwrap_or_else(|error| ChildDisposition::Unknown(error.to_string()))
        {
            ChildDisposition::Active => {
                tracing::warn!(
                    stage_id,
                    session_id,
                    "Active child forbids completion takedown"
                );
                Ok(())
            }
            ChildDisposition::Unknown(reason) => {
                tracing::warn!(stage_id, session_id, %reason, "Child ownership is uncertain");
                self.escalate_completion_ownership(stage_id, session_id, fingerprint)
            }
            ChildDisposition::NoChildren => self.retire_completion_writer(
                stage_id,
                session_id,
                fingerprint,
                repeat_count,
                &failure_code,
            ),
        }
    }

    fn current_completion_failure(
        &self,
        stage_id: &str,
        session_id: &str,
        fingerprint: &str,
        escalation: Option<&CompletionEscalation>,
    ) -> Result<Option<String>> {
        let stage = self.load_stage(stage_id)?;
        if stage.status != StageStatus::Executing || stage.session.as_deref() != Some(session_id) {
            tracing::debug!(
                stage_id,
                session_id,
                "Ignoring stale completion blocker event"
            );
            return Ok(None);
        }
        if matches!(escalation, Some(CompletionEscalation::CapacityExhausted)) {
            return Ok(Some("diagnostic capacity exhausted".into()));
        }
        let checkpoint =
            load_trusted_session_checkpoint(stage_id, session_id, &self.config.work_dir)?;
        let commit = crate::handoff::completion::identity::expected_stage_commit(
            &stage,
            &self.config.repo_root,
        )?;
        let blocker = checkpoint
            .as_ref()
            .and_then(|value| current_blocker(value, &stage, Some(&commit)))
            .filter(|value| value.fingerprint == fingerprint);
        if blocker.is_none() {
            tracing::debug!(
                stage_id,
                session_id,
                "Ignoring stale completion fingerprint"
            );
        }
        Ok(blocker.map(|value| value.external_failure_code.clone()))
    }

    fn retire_completion_writer(
        &mut self,
        stage_id: &str,
        session_id: &str,
        fingerprint: &str,
        repeat_count: u32,
        failure_code: &str,
    ) -> Result<()> {
        let Some(session) = load_session_exact(&self.config.work_dir, session_id)? else {
            return self.escalate_completion_ownership(stage_id, session_id, fingerprint);
        };
        if session_process_status(&self.config.work_dir, &session) == SessionProcessStatus::Missing
        {
            return self.escalate_completion_ownership(stage_id, session_id, fingerprint);
        }
        match self.confirm_session_gone(&session) {
            Ok(true) => self.park_completion(
                stage_id,
                &session,
                fingerprint,
                repeat_count,
                failure_code,
                false,
            ),
            Ok(false) => self.kill_completion_writer(
                stage_id,
                &session,
                fingerprint,
                repeat_count,
                failure_code,
            ),
            Err(error) => {
                tracing::warn!(stage_id, session_id, %error, "Writer liveness is uncertain");
                self.escalate_completion_ownership(stage_id, session_id, fingerprint)
            }
        }
    }

    fn kill_completion_writer(
        &mut self,
        stage_id: &str,
        session: &Session,
        fingerprint: &str,
        repeat_count: u32,
        failure_code: &str,
    ) -> Result<()> {
        match self.take_down_agents(
            stage_id,
            vec![session.clone()],
            SessionExitReason::CriteriaBlocked,
        ) {
            Ok(survivors) if survivors.is_empty() => self.park_completion(
                stage_id,
                session,
                fingerprint,
                repeat_count,
                failure_code,
                true,
            ),
            Ok(_) => self.escalate_completion_ownership(stage_id, &session.id, fingerprint),
            Err(error) => {
                tracing::warn!(stage_id, session_id = %session.id, %error, "Writer takedown is uncertain");
                self.escalate_completion_ownership(stage_id, &session.id, fingerprint)
            }
        }
    }

    fn park_completion(
        &mut self,
        stage_id: &str,
        session: &Session,
        fingerprint: &str,
        repeat_count: u32,
        failure_code: &str,
        session_recorded: bool,
    ) -> Result<()> {
        if !session_recorded {
            mark_session_terminal_reason(
                &self.config.work_dir,
                &session.id,
                SessionStatus::ContextExhausted,
                SessionExitReason::CriteriaBlocked,
            )
            .context("persisting verified completion-blocker retirement")?;
        }
        remove_signal(&session.id, &self.config.work_dir)?;
        self.active_sessions.remove(stage_id);
        let reason = format!(
            "completion blocked {} after {repeat_count} verified attempts: {failure_code}; fix it, then retry or reset the stage",
            short_fingerprint(fingerprint)
        );
        self.set_completion_review(stage_id, &session.id, reason, true)
    }

    fn escalate_completion_ownership(
        &mut self,
        stage_id: &str,
        session_id: &str,
        fingerprint: &str,
    ) -> Result<()> {
        let reason = format!(
            "completion blocked {}: writer ownership unknown; confirm session {session_id} has exited before reassigning",
            short_fingerprint(fingerprint)
        );
        self.set_completion_review(stage_id, session_id, reason, false)
    }

    fn set_completion_review(
        &mut self,
        stage_id: &str,
        session_id: &str,
        reason: String,
        accumulate_time: bool,
    ) -> Result<()> {
        self.update_stage(stage_id, |stage| {
            ensure_current_writer(stage, session_id)?;
            if accumulate_time {
                stage.accumulate_attempt_time(Utc::now());
            }
            stage.force_status_with_reason(StageStatus::NeedsHumanReview, &reason);
            stage.review_reason = Some(reason.clone());
            Ok(())
        })?;
        self.graph
            .mark_status(stage_id, StageStatus::NeedsHumanReview)
    }
}

fn ensure_current_writer(stage: &Stage, session_id: &str) -> Result<()> {
    anyhow::ensure!(
        stage.status == StageStatus::Executing && stage.session.as_deref() == Some(session_id),
        "stage moved before completion blocker disposition could be persisted"
    );
    Ok(())
}
